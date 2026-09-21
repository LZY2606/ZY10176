# `Face::outline_glyph` anatomy

This document is a code-path reference for [`Face::outline_glyph`](../src/lib.rs).
It describes which table is selected for a glyph, how that table is parsed, which
`OutlineBuilder` callbacks fire and in which order, and where the returned
bounding box comes from. It is descriptive only: it pins down the current
implementation so that refactorings and bug fixes can prove they did not change
observable behaviour. It introduces no new public API and no new font capability,
in line with the crate's maintenance mode.

The matching executable evidence is the `outline_glyph` module in
`tests/tables/outline_glyph.rs`, parsed through the public `Face::parse` /
`Face::outline_glyph` entry points using only fonts synthesized inside the test
process.

## 1. Entry point and table selection

`Face::outline_glyph(glyph_id, builder)` returns `Option<Rect>` and forwards
drawing calls to `builder: &mut dyn OutlineBuilder`. Tables are probed in one
fixed order (`src/lib.rs`):

```text
Face::outline_glyph(glyph_id, builder)
|
+-- feature = "variable-fonts" AND self.tables.gvar.is_some() ?
|       YES -> gvar::Table::outline(glyf?, coords, glyph_id, builder)
|              |  early-returns `None` (via `?`) when the face has no glyf table
|              |  result: Option<Rect>   [always calculated, even on failure paths]
|
+-- self.tables.glyf.is_some() ?
|       YES -> glyf::Table::outline(glyph_id, builder) -> Option<Rect>
|
+-- self.tables.cff.is_some() ?
|       YES -> cff1::Table::outline(glyph_id, builder).ok()
|              result: Result<Rect, CFFError>, error folded to `None`
|
+-- feature = "variable-fonts" AND self.tables.cff2.is_some() ?
|       YES -> cff2::Table::outline(coords, glyph_id, builder).ok()
|
+-- none of the above -> None
```

Consequences worth stating explicitly:

* Presence of a `gvar` table routes the *whole face* through the gvar path,
  including glyphs that have no variation data: gvar falls back to the plain glyf
  coordinates. A `glyf` face without a `gvar` table never enters this branch.
* The check is table existence, not glyph capability. A font with a `glyf` table
  still selects the glyf branch for a glyph that has no outline there; it never falls
  through to a later branch. A face with neither outline table returns `None`.
* CFF and CFF2 failures are `Err(CFFError)` values; the entry point discards
  the error detail and reports `None`.
* The method is affected by variation axes: gvar and CFF2 receive the current
  normalized coordinates (`Face::set_variation`), glyf and CFF do not.

## 2. Pull parsing and the "check the result" contract

All four back ends stream callbacks while they parse. A malformed glyph can produce a
prefix of a path and then fail:

* glyf/gvar propagate the failure with `?`, so `outline_glyph` returns `None`
  even though the builder already received `move_to`/`line_to`/`close` calls;
* CFF/CFF2 return `Err(...)` after emitting a prefix, which the entry point folds to
  `None`. Notably, a CFF charstring missing `endchar` emits its drawing
  commands but never the final `close`, and still returns an error.

Callers must therefore treat any builder output as untrusted until the call returns
`Some(rect)`; never treat a returned `None` plus a partial builder stream as a
"degenerate but usable" outline. The tests pin both sides of this contract:

* `a_component_ring_is_stopped_by_the_depth_limit_and_returns_no_bbox`
  observes 16 completed contours paired with `None`;
* `cff_missing_endchar_emits_no_close_and_returns_no_bbox` observes
  `M 10 0 L 30 30 ` with no `Z`, paired with `None`.

## 3. The `glyf` and `gvar` back ends

### 3.1 Simple glyf glyphs

`glyf::Table::outline` creates a `glyf::Builder` carrying the user builder, an
accumulated transform (identity initially) and an empty calculated `RectF`, then
resolves the glyph bytes through `loca::Table::glyph_range` +
`glyf_table.get(range)`, and calls `outline_impl` with `depth = 0` and a fresh
visit budget.

`outline_impl` reads `numberOfContours` and **skips the eight bbox bytes**
(`s.advance(8); // Skip bbox. We use calculated one.`):

* `numberOfContours > 0`: `parse_simple_outline` builds the point iterators and
  every point goes through `Builder::push_point`, which converts TrueType on/off-curve
  runs into `move_to` / `line_to` / `quad_to` calls. The first point of an
  all-on-curve contour is a `move_to`; `finish_contour` always closes with an
  explicit segment back to the start point (a `line_to` for an all-on-curve
  contour) and then calls `builder.close()`.
* `numberOfContours == 0`: no drawing calls; the bbox stays default and the result
  is `None`.
* `numberOfContours < 0`: composite glyph (below).

The point stream is decoded lazily by three iterators, but the spans that feed
them are resolved up front (see section 6): endpoints via
`Stream::read_array16`, and the flags/coordinate lengths by walking every repeat
flag in `resolve_coords_len` before any point is emitted.

### 3.2 Composite glyphs and transform multiplication

For each component produced by `CompositeGlyphIter`, `outline_impl`:

1. resolves the component glyph through the same `loca` range + glyf slice,
2. computes `Transform::combine(builder.transform, component.transform)`,
3. builds a child `glyf::Builder` with that transform and the *parent's bbox*,
4. recurses with `depth + 1` and the shared `&mut budget`,
5. copies the child bbox back into the parent.

`Transform::combine(ts1, ts2) = ts1 * ts2`, i.e. the accumulated parent
matrix first, this component's matrix second. A child point is therefore mapped as
`parent * component * p`: the component's own `a..d` linear part applies to the
point first, and the accumulated parent transform applies second. Component offsets
(`e`, `f`) live inside the component matrix and, by the matrix product, are in the
*unscaled* coordinate space: `combine(A,B).e = A.a*B.e + A.c*B.f + A.e`.
A parent scale never rescales a child component's own offset. The tests
`two_level_composite_chains_transforms_in_parent_before_child_order` and
`component_xy_offset_is_not_rescaled_by_a_later_parent_scale` pin this order
with numeric counter-examples (including an `x`/`y` scale, which is the smallest
case that distinguishes the two orderings).

Component scale values are parsed as F2DOT14 signed fixed point (`i16`); values
like 2.0 are not representable (the bit pattern of 2.0 is −2.0), while 1.5
and 1.25 are exact — the tests use representable values deliberately.

Point-number matching arguments (the `ARGS_ARE_XY_VALUES` flag unset) are not
used for alignment (README documents this limitation); the two argument bytes are
still consumed so the following components stay aligned in the stream.

### 3.3 Recursion depth and total-visit budget

Two independent limits guard composite recursion (`src/tables/glyf.rs`):

* `MAX_COMPONENTS = 32` bounds *depth*: at entry, `depth >= MAX_COMPONENTS`
  returns `None`. A chain of 31 nested components still outlines
  (`a_depth_chain_at_the_component_limit_still_outlines`); a chain of 32
  fails (`a_depth_chain_one_frame_past_the_component_limit_is_rejected`). A
  reference cycle (`a_component_ring_...`) is just a chain to this guard,
  producing repeated contours followed by `None`, never an infinite loop.
* `MAX_COMPONENT_VISITS = 100_000` bounds the *total number of component
  glyph visits per top-level call*, independently of depth or fan-out. The depth
  limit alone allows `branching^depth` visits using only `depth + 1` distinct
  glyphs; the budget is a `u32` decremented with `checked_sub`, so its
  exhaustion also surfaces as `None`. The exponential blow-up test lives in
  `tests/tables/glyf.rs` (`shared_component_fan_out_...`).

### 3.4 `gvar`

`gvar::Table::outline` reuses `glyf::Builder`, the same depth and the same
visit budget, but coordinates come from the default glyf points plus variation
deltas (`outline_var_impl`). For a composite glyph, gvar data carries a
translation adjustment per component; it is combined **only when the component uses
`ARGS_ARE_XY_VALUES`**, as
`Transform::combine(transform, new_translate(dx,dy))` before the component
matrix is combined. The stored glyf bbox is skipped there as well (it describes
the default instance, not the current coordinates).

The gvar entry does not propagate `outline_var_impl`'s return value and ends
with `b.bbox.to_rect()`, so its exact failure behaviour is worth pinning down
(the tests cover both cases with synthesized fonts carrying an empty gvar table, so no
deltas are applied):

* at the top level, an unknown glyph id or a malformed glyph record returns `None`
  before any point is emitted;
* a component cycle that reaches the depth guard, and a composite graph that
  exhausts the visit budget, both return `None`: the failure happens inside the
  recursive frame while the *parent's* builder bbox is still the empty default
  (`RectF::to_rect` maps that to `None`), and the accumulated bbox is only
  copied back to the parent on the success path after the recursive call returns;
* successfully processed components and simple glyphs, including earlier siblings drawn
  before a later component fails, do extend the builder — but only a fully successful
  run ends in `Some(rect)`.

So a bbox observed from this path always describes glyphs whose parsing completed;
a stopped recursive traversal never presents a partially accumulated bbox as the
result.

## 4. The CFF and CFF2 back ends

### 4.1 CFF (CFF1)

`cff1::Table::outline` looks the glyph charstring up in the CharStrings INDEX
(`NoGlyph` if absent) and runs `parse_char_string`:

* the CFF-side `Builder` (`src/tables/cff/mod.rs`) applies the optional font
  matrix (see section 5.3), extends its `RectF` from each emitted point and
  forwards `move_to`/`line_to`/`curve_to` to the user builder;
* contours are closed by the charstring move logic: after the first contour,
  another move-to emits `builder.close()` before the new `move_to`, and
  `endchar` emits a final `close` unless no contour was started.
* CFF does **not** synthesize a closing segment: unlike glyf's
  `finish_contour`, `endchar` calls `close()` alone, with no line/curve back to
  the start point. The tests assert the different streams directly
  (`cff_charstring_outlines_a_triangle_and_closes_on_endchar` vs the glyf
  tests).
* after parsing, success requires `has_endchar` (`MissingEndChar` otherwise) and
  a non-default bbox (`ZeroBBox` for an endchar-only `.notdef`-style
  charstring); `RectF::to_rect` reports `BboxOverflow` if a coordinate does not
  fit `i16`.

Subroutines and `seac` accents recurse into `_parse_char_string`; nesting is
bounded by `STACK_LIMIT = 10` (`NestingLimitReached`) and *total*
subroutine invocations by `MAX_SUBROUTINE_CALLS = 4096`
(`SubroutineCallLimitReached`) — the same depth-versus-total-work split as
glyf components.

### 4.2 CFF2

`cff2::Table::outline(coords, glyph_id, builder)` parses the charstring with
the current normalized coordinates. Differences from CFF1:

* there is no width, no FontMatrix transform (the inner builder has `transform:
  None`) and no `endchar` operator — a complete charstring emits no final
  `close` at all (`cff2_blend_moves_with_the_axis_and_never_emits_a_close`
  asserts a stream ending in `L ... ` with no `Z`);
* `blend` (op 16) reads region scalars from the embedded ItemVariationStore on
  first use, adding `delta * scalar` to each blended operand; scalars resolve
  lazily so a table without a vstore outlines static glyphs fine, while a `blend`
  without a store fails `InvalidItemVariationDataIndex`;
* the same stack/subroutine limits apply (`STACK_LIMIT = 10`,
  `MAX_SUBROUTINE_CALLS = 4096`), with the per-glyph scalar buffer
  capped at 64 regions (`BlendRegionsLimitReached`);
* an endchar-only/empty charstring still fails with `ZeroBBox`, surfacing as
  `None` through `Face`.

One embedded ItemVariationStore quirk the synthetic test has to encode (and which
is easy to get wrong): `cff2::Table::parse` skips the store's u16 length
field before calling `ItemVariationStore::parse`, whose `data` slice therefore
starts at the length field while it reads `format` first, and the stored
`regionListOffset`/data offsets are then resolved against that slice. The
RegionList sits between the fixed header and the ItemVariationData; the test
builder comments the exact arithmetic.

## 5. Three different bounding boxes

These are three distinct contracts. Do not conflate them.

### 5.1 The bbox stored inside the font

* glyf records carry an `xMin/yMin/xMax/yMax` quad. It is reachable through
  the low-level API `face.tables().glyf.unwrap().bbox(glyph_id)` and is
  returned verbatim. The code never uses it while outlining
  (`s.advance(8)` in `outline_impl`/`outline_var_impl`) because it can be
  malformed and, for a variable font, describes the default instance.
  `glyf_table_bbox_returns_the_stored_bbox_while_outline_ignores_it`
  stores a deliberately wrong quad and proves the two answers differ.
* CFF/CFF2 have no per-glyph bbox at all; `head.yMin/xMin/...` only
  provides `Face::global_bounding_box`.

### 5.2 The computed callback bbox

* Every outline back end keeps a `RectF` extended from the coordinates of every
  emitted `move_to`/`line_to`/quadratic/cubic segment — **including
  off-curve control points** (each `quad_to`/`curve_to` extends from the
  control point coordinates as well as the endpoint).
* The bbox is therefore conservative, not a tight curve bound:
  `cff_control_points_participate_in_the_returned_bbox` returns y_max = 100
  contributed only by a cubic control point.
* The result is the i16-rounded `Rect` via `RectF::to_rect`; coordinates
  outside `i16` make the CFF/CFF2 result `BboxOverflow` (surfaced as
  `None`) while glyf coordinates are native i16s.
* `Face::glyph_bounding_box` is a shorthand for this computed bbox
  (`DummyOutline`); the doc comment on it states the glyf stored bbox is
  intentionally ignored.

### 5.3 The transformed bbox

* In glyf, `Builder.move_to/line_to/quad_to` apply the accumulated component
  transform **before** extending the bbox and before forwarding the point, so the
  returned bbox is the transformed-coordinate bound (see the two-level and
  offset-not-rescaled tests for numeric instances).
* In CFF, the optional `(units_per_em, FontMatrix)` transform (default
  0.001 at 1000 upem is the identity) is applied per point in the inner
  builder, and that transformed point feeds the bbox. CFF2 applies no matrix.

## 6. Eager vs lazy array bounds checks

Bounds checks sit at two different moments:

* **Eager, at parse/stream time** — a whole fixed-size span is validated before an
  array/iterator exists:
  * `Stream::read_bytes(len)` bounds `len` against the remaining bytes *before*
    adding it to the offset (`src/parser.rs`), so a font-derived length can
    neither over-read nor overflow pointer arithmetic on 32-bit targets;
  * `Stream::read_array16`/`read_array32` wrap that check after
  `count * T::SIZE` (with `checked_mul` on the 32-bit path), so asking for 65535
  endpoints from a 10-byte record fails immediately
  (`a_glyph_declaring_more_points_than_its_record_holds_is_rejected_eagerly`,
  and the truncated-glyph test);
  * glyf's `resolve_coords_len` walks every flag repeat up front, rejecting repeat
  counts that exceed the remaining point total, and only then are the coordinate slices
  handed to the streaming iterators;
  * `loca::Table::parse` eagerly clamps the requested `maxp.numGlyphs + 1`
  offset count down to the bytes actually present in the table.
* **Lazy, at element access** — `LazyArray16::get`/`LazyArray32::get`
  re-validate on each index: `index < len()` (`len = data.len() / T::SIZE`,
  so a trailing half element is invisible) and a concrete
  `data.get(start..end)` slice. `loca::glyph_range` additionally checks
  `glyph_id + 1 < len` and rejects an empty/non-ascending range.
  `lazy_array16_checks_each_access_against_its_constructed_span` pins the
  per-access behaviour from outside the crate.

So the split is: fixed-size tables are validated eagerly as one span, while
per-element decoding (endpoints in `EndpointsIter`, flags in `FlagsIter`,
coordinate deltas in `CoordsIter`, lazy table arrays elsewhere) is checked at each
access and falls back to zeros/skips rather than over-reading.

## 7. Resource limits summary (malicious input)

| Limit | Location | Bounds | Failure |
| --- | --- | --- | --- |
| composite depth | `glyf::MAX_COMPONENTS = 32` | recursion frames | `None` |
| composite visits | `glyf::MAX_COMPONENT_VISITS = 100_000` | total component glyph visits per outline, any fan-out/depth | `None` |
| point count | u16 endpoints + eager spans | endpoints table eagerly bounded to the record; total points = last endpoint + 1 (`checked_add`), 65535 max; flag/coord lengths resolved up front | `None`, no points streamed |
| CFF subroutine depth | `STACK_LIMIT = 10` | nested local/global subroutine / seac frames | `NestingLimitReached` → `None` |
| CFF/CFF2 subroutine calls | `MAX_SUBROUTINE_CALLS = 4096` | total invocations per glyph | `SubroutineCallLimitReached` → `None` |
| CFF/CFF2 operand stack | 48 / 513 entries | stack length per charstring/operator | `ArgumentsStackLimitReached`/`InvalidArgumentsStackLength` |
| CFF2 scalars | 64 regions | scalars pushed per `blend` | `BlendRegionsLimitReached` |

## 8. Complexity

Let `p` be the number of outline points (glyf) or emitted charstring
segments (CFF), `c` the number of component glyphs visited (glyf), `s` the
number of subroutine invocations (CFF/CFF2), and `k` the number of regions
active for one `blend` (CFF2):

* point/segment work is `O(p)`; the calculated bbox update is constant per
  emitted point/control point;
* composite work is `O(c + p)` with `c <= 100_000` visits and depth `<= 32`
  regardless of the reference graph's fan-out;
* CFF/CFF2 charstring work is `O(bytes + s)` with `s <= 4096`; each
  `blend` adds `O(n * k)` stack work and `k <= 64`;
* memory is constant-size stack buffers only: no heap allocation on these paths
  (gvar tuple buffers are stack arrays; `gvar-alloc` only affects gvar tuple
  *parsing* for Apple-style fonts). The core path stays allocation-free in
  `no_std` builds.

## 9. Compatibility trade-offs

* Everything described is existing, observable behaviour; this change set only adds
  tests and this document. No public signature, feature, platform or table-order
  semantics change.
* Depth/visit caps are intentionally tighter than the spec's nominal maxima
  (the spec even allows up to 4095 gvar tuples); they trade rare-but-legal
  fonts for guaranteed linear work on attacker-controlled input. Real fonts measured
  during the hardening stayed well below the caps.
* F2DOT14 component scales are signed i16 fixed point; synthetic tests must use
  representable values (±1.99994 range, 1.5/1.25 exact). This is a format
  limit, not an implementation choice.
* The pull-parser contract (partial callbacks plus `None`) is preserved and now
  documented; changing it to "buffer until success" would be an API-behaviour
  change and is out of scope.
* The exact gvar composite-failure mechanics (section 3.4) are pinned down with
  synthesized-font tests rather than guessed from the `Option<()>` return type.

## 10. Executable index

Tests live in `tests/tables/outline_glyph.rs` and build their fonts in-process
(no system fonts, no network, no clock/directory assumptions beyond a single
millisecond-order resource test in the existing glyf suite):

* simple glyf, stored-vs-computed bbox, eager truncation:
  `simple_glyf_glyph_recomputes_its_bbox_and_closes_with_a_line`,
  `glyf_table_bbox_returns_the_stored_bbox_while_outline_ignores_it`,
  `a_truncated_simple_glyf_glyph_is_rejected_without_a_partial_bbox`
* two-level components, offset/scale ordering, F2DOT14-exact values:
  `two_level_composite_chains_transforms_in_parent_before_child_order`,
  `component_xy_offset_is_not_rescaled_by_a_later_parent_scale`
* component cycle / depth / fan-out (glyf and gvar branches):
  `a_component_ring_is_stopped_by_the_depth_limit_and_returns_no_bbox`,
  `a_depth_chain_at_the_component_limit_still_outlines`,
  `a_depth_chain_one_frame_past_the_component_limit_is_rejected`,
  `gvar_ring_returns_no_bbox_like_the_glyf_branch`,
  `gvar_fan_out_budget_exhaustion_returns_no_bbox`
* CFF charstring, cubic control-point bbox, malformed tail:
  `cff_charstring_outlines_a_triangle_and_closes_on_endchar`,
  `cff_control_points_participate_in_the_returned_bbox`,
  `cff_missing_endchar_emits_no_close_and_returns_no_bbox`
* CFF2 variation and no-close ordering:
  `cff2_variable::cff2_blend_moves_with_the_axis_and_never_emits_a_close`
* no-outline glyphs and branch presence: `empty_glyf_glyph_and_unknown_glyph_id_have_no_outline`,
  `face_with_only_glyf_never_reaches_cff2_and_an_outline_table_font_still_outlines`
* point/array resource bounds:
  `a_glyph_declaring_more_points_than_its_record_holds_is_rejected_eagerly`,
  `lazy_array16_checks_each_access_against_its_constructed_span`
