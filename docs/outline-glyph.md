# `Face::outline_glyph`: dispatch, callbacks, and bounding boxes

This document dissects the code path from `Face::outline_glyph` down to the
per-table parsers, the `OutlineBuilder` callback contract, and the bounding box
that is returned. It describes current behavior as pinned down by the
characterization tests in `tests/tables/` (`glyf.rs`, `cff1.rs`, `cff2.rs`,
`gvar.rs`, `outline.rs`). It is a maintenance document: it does not define new
public capabilities.

## Dispatch order

`Face::outline_glyph` (`src/lib.rs`) tries outline sources in a fixed order and
returns the first one that applies:

```
Face::outline_glyph(glyph_id, builder)
│
├─ [variable-fonts] tables.gvar present?
│     └─ gvar::Table::outline(tables.glyf?, coords, glyph_id, builder)
│        — note: `tables.glyf?` means a font with `gvar` but no `glyf`
│          returns None here, even if CFF/CFF2 exist
│
├─ tables.glyf present?
│     └─ glyf::Table::outline(glyph_id, builder)
│
├─ tables.cff present?
│     └─ cff1::Table::outline(glyph_id, builder).ok()
│        — CFFError is discarded; any failure becomes None
│
├─ [variable-fonts] tables.cff2 present?
│     └─ cff2::Table::outline(coords, glyph_id, builder).ok()
│
└─ None (no outline source, or the source rejected the glyph)
```

The order is observable: a font containing both `glyf` and `CFF ` outlines from
`glyf` and never interprets the charstrings
(`outline::glyf_takes_precedence_over_cff`).

`Face::glyph_bounding_box` is exactly `outline_glyph` with a dummy builder;
there is no cheaper bbox path through the public `Face` API.

## The `glyf` path

```
glyf::Table::outline                       src/tables/glyf.rs
├─ loca::Table::glyph_range(glyph_id)      src/tables/loca.rs   (lazy bounds check)
├─ glyf::Table::get → glyf bytes           (slice::get bounds check)
└─ outline_impl(depth = 0, budget = MAX_COMPONENT_VISITS)
   ├─ depth >= MAX_COMPONENTS (32) → None  (recursion limit)
   ├─ budget.checked_sub(1)?               (total-visit limit, 100_000)
   ├─ numberOfContours > 0 → simple glyph:
   │     parse_simple_outline
   │     ├─ read_array16 endpoints         (eager bounds check)
   │     ├─ resolve_coords_len             (walks flags eagerly, incl. repeats)
   │     └─ GlyphPointsIter → Builder::push_point per point
   │        → move_to / line_to / quad_to callbacks
   │        → finish_contour: line back to the contour start, then close()
   └─ numberOfContours < 0 → composite glyph:
         CompositeGlyphIter (lazy, one component record at a time)
         └─ per component: Transform::combine(outer, inner), recurse
```

The `gvar` path (`src/tables/gvar.rs`, `outline_var_impl`) mirrors this
skeleton with the same `MAX_COMPONENTS`/`MAX_COMPONENT_VISITS` limits, but
applies variation deltas to every point (simple glyphs) or to component
offsets (composite glyphs, only when `ARGS_ARE_XY_VALUES` is set) before
pushing them.

## The CFF / CFF2 paths

`cff1::Table::outline` / `cff2::Table::outline` (`src/tables/cff/`) interpret
the charstring for `glyph_id`:

- subroutine nesting is bounded by `STACK_LIMIT` (10);
- total subroutine calls are bounded by `MAX_SUBROUTINE_CALLS` (4_096), which
  is what stops `fanout^depth` amplification (`cff1::subr_call_budget_*`,
  `cff2::*_subroutine_call_budget_bounds_fanout_amplification`);
- the operand stack is fixed-size (48 entries for CFF1, 513 for CFF2);
- CFF2 `blend` operands are scaled by region scalars computed from the current
  variation coordinates (`cff2::blend_scales_deltas_by_the_current_variation_coordinates`);
  with no coordinates the scalars default to 1.0.

## Callback and contour-closing contract

`OutlineBuilder` receives only `move_to`, `line_to`, `quad_to`, `curve_to` and
`close`. Contour closing differs per source — this is intentional and the
tests pin it down:

- `glyf`/`gvar`: every contour is closed explicitly. If the last point is not
  the first, a `line_to` back to the contour's start point is emitted first,
  then `close()`. Off-curve points at a contour boundary produce implied
  on-curve midpoints (`quad_to` segments), per the TrueType spec.
- `CFF `: `close()` is emitted at `endchar` (and before any non-first
  `move_to`). No segment back to the start is inserted.
- `CFF2`: has no `endchar`, so the last contour is never closed
  (`cff2::minimal_glyph_outlines` expects no trailing `Z`). `close()` is only
  emitted before a non-first `move_to`.

Because `ttf-parser` is a pull parser, the builder may already have received
segments when a later parse error makes `outline_glyph` return `None`. The
partial output must be discarded; only the `Option<Rect>` return value tells
you whether the outline is complete.

## Three different bounding boxes

Do not treat these as one contract:

1. **Stored bbox** — the `xMin/yMin/xMax/yMax` in the `glyf` glyph header.
   Exposed via `glyf::Table::bbox`. It is font-provided data and can be
   malformed; `outline_glyph` ignores it entirely
   (`glyf::outline_computes_the_bbox_and_ignores_the_stored_one`).
2. **Computed bbox** — what `outline_glyph` returns: the union of all points
   actually emitted to the builder, accumulated in `RectF` and converted with
   `RectF::to_rect` (which fails, yielding `None`, if a coordinate does not
   fit into `i16`). For CFF/CFF2 and variable fonts this is the only bbox
   that exists.
3. **Transformed bbox** — for composite glyphs, points are transformed before
   the bbox is extended, so the returned bbox encloses the *transformed*
   outline. It is not the transform of the child's bbox (transforming a bbox
   corner-wise would be wrong under rotation/skew) and not the union of the
   stored child bboxes.

Related but distinct: `Face::global_bounding_box` is the font-wide stored bbox
from the `head` table, and `gvar` glyphs never consult the stored `glyf` bbox
because it describes the default variation only.

## Transform composition order

Nested component transforms compose as `Transform::combine(outer, inner)`
(`src/lib.rs`), i.e. a point is mapped by the inner (child) transform first
and the outer (parent) transform second: `p → outer(inner(p))`.

Consequences, each with a characterization test:

- An inner component's `dx/dy` offset **is** scaled by an outer component's
  scale. Leaf point (10,10), inner translate (10,4), outer scale 1.5 lands at
  (30,21), not at (25,19)
  (`glyf::nested_component_transforms_scale_inner_offsets`).
- A component's **own** `dx/dy` is **not** scaled by its own scale flag: the
  offset is applied after the scale within the same record, so (10,10) with
  scale 1.5 and offset (10,4) lands at (25,19)
  (`glyf::a_components_own_offset_is_not_scaled_by_its_own_scale`).

## Resource bounds

All recursion and amplification is capped; exceeding a cap fails the whole
outline (`None`/`CFFError`), never a partial result:

| Bound | Value | Guards against |
| --- | --- | --- |
| `glyf::MAX_COMPONENTS` | 32 | component-chain depth, incl. reference cycles (`glyf::component_cycle_is_bounded_by_the_recursion_limit`) |
| `glyf::MAX_COMPONENT_VISITS` | 100 000 | shared-child fan-out `branching^depth` (`glyf::shared_component_fan_out_*`) |
| CFF `STACK_LIMIT` | 10 | subroutine nesting depth |
| CFF `MAX_SUBROUTINE_CALLS` | 4 096 | subroutine fan-out amplification |
| CFF argument stack | 48 / 513 (CFF1 / CFF2) | operand overflow |

Point counts are bounded by the format itself: `endPtsOfContours` is `u16`, so
a simple glyph has at most 65 535 points, and `resolve_coords_len` rejects
flag-repeat runs that exceed the declared point count before any coordinate
byte is read.

### Eager vs. lazy bounds checks

Parsing never panics on malformed data; checks live at two levels:

- **Eager** (`Stream::read_bytes`, `read_array16`, `read_array32` in
  `src/parser.rs`): the whole byte range is validated up front. Used for the
  `loca` offset array at table parse time, the `endPtsOfContours` array, and
  the flag/coordinate area sizing in `parse_simple_outline`.
- **Lazy** (`LazyArray16/32::get`, `slice::get`): each element access is
  checked individually. Used by `loca::Table::glyph_range` per glyph, by
  `glyf::Table::get` for the glyph byte range, by `EndpointsIter` per contour
  end, and by `CompositeGlyphIter` per component record.

The practical difference: a font can be parsed (`Face::parse`) even when a
glyph it contains is truncated — the failure surfaces lazily, only when that
glyph is outlined, and only for that glyph.

## Failure semantics

`outline_glyph` is all-or-nothing with respect to its return value: any parse
failure, limit violation, or bbox overflow anywhere in the recursion
propagates `None` to the caller. A failed outline never returns a bbox
computed from a partially parsed outline. (The builder, as noted above, may
hold partial segments — discard them.)

## Complexity and compatibility notes

- Outlining is O(points + component visits), with both terms capped as above;
  no heap allocation happens on the `glyf`/CFF paths, keeping the `no_std`
  core free of `alloc`. The `gvar` path allocates only when the `gvar-alloc`
  feature is enabled and a glyph exceeds 32 variation tuples.
- Stack usage is bounded but not tight: a composite variable glyph nests up to
  32 frames, each holding a variation-tuple buffer (~80 KiB worst case).
- The dispatch order and the contour-closing differences above are observable
  behavior that downstream code may rely on; the characterization tests exist
  to keep them stable.
