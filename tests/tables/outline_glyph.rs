//! Characterization tests for `Face::outline_glyph`.
//!
//! Unlike the per-table tests, every font here is parsed through `Face::parse` and then
//! outlined through the public `Face::outline_glyph` entry point. The tests pin down the
//! current branch selection (`gvar` -> `glyf` -> `CFF` -> `CFF2`), the exact
//! `OutlineBuilder` command stream (including the implicit closing segment), the
//! computed bounding box, and the error behaviour for malformed input.
//!
//! All fonts are synthesized in-process: no system fonts, no network and no directory
//! traversal are involved.

#![cfg(feature = "std")]

use std::fmt::Write;

use ttf_parser::{GlyphId, OutlineBuilder, Rect};

/// Records the emitted outline as a short command string, mirroring the notation used by
/// the crate-level `outline_glyph` doc example so the two stay comparable.
struct PathBuilder(String);

impl PathBuilder {
    fn outline(face: &ttf_parser::Face, glyph: u16) -> (Option<Rect>, String) {
        let mut builder = PathBuilder(String::new());
        let bbox = face.outline_glyph(GlyphId(glyph), &mut builder);
        (bbox, builder.0)
    }
}

impl OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        write!(&mut self.0, "M {x} {y} ").unwrap();
    }

    fn line_to(&mut self, x: f32, y: f32) {
        write!(&mut self.0, "L {x} {y} ").unwrap();
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        write!(&mut self.0, "Q {x1} {y1} {x} {y} ").unwrap();
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        write!(&mut self.0, "C {x1} {y1} {x2} {y2} {x} {y} ").unwrap();
    }

    fn close(&mut self) {
        self.0.push_str("Z ");
    }
}

fn rect(x_min: i16, y_min: i16, x_max: i16, y_max: i16) -> Rect {
    Rect { x_min, y_min, x_max, y_max }
}

// -------------------------------------------------------------------------------------------
// Minimal sfnt builder.
//
// `Face::parse` requires `head`, `hhea` and `maxp`; everything else (including `glyf`,
// `loca`, `CFF `, `CFF2` and `fvar`) is optional. Table records must be sorted by tag and
// may live at any 1-byte-aligned offset; only `Offset32` length arithmetic is checked.
// -------------------------------------------------------------------------------------------

fn build_sfnt(sfnt_version: u32, mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_unstable_by_key(|table| table.0);

    let mut font = Vec::new();
    font.extend_from_slice(&sfnt_version.to_be_bytes());
    font.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes()); // searchRange
    font.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
    font.extend_from_slice(&0u16.to_be_bytes()); // rangeShift

    let mut offset = 12 + 16 * tables.len() as u32;
    for (tag, data) in &tables {
        font.extend_from_slice(tag);
        font.extend_from_slice(&0u32.to_be_bytes()); // checkSum, never validated
        font.extend_from_slice(&offset.to_be_bytes());
        font.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &tables {
        font.extend_from_slice(data);
    }
    font
}

fn head_table() -> Vec<u8> {
    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // fontRevision
    head.extend_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
    head.extend_from_slice(&0u16.to_be_bytes()); // flags
    head.extend_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
    head.extend_from_slice(&0u64.to_be_bytes()); // created
    head.extend_from_slice(&0u64.to_be_bytes()); // modified
    head.extend_from_slice(&10i16.to_be_bytes()); // xMin
    head.extend_from_slice(&10i16.to_be_bytes()); // yMin
    head.extend_from_slice(&30i16.to_be_bytes()); // xMax
    head.extend_from_slice(&30i16.to_be_bytes()); // yMax
    head.extend_from_slice(&0u16.to_be_bytes()); // macStyle
    head.extend_from_slice(&8u16.to_be_bytes()); // lowestRecPPEM
    head.extend_from_slice(&2i16.to_be_bytes()); // fontDirectionHint
    head.extend_from_slice(&1i16.to_be_bytes()); // indexToLocFormat: long
    head.extend_from_slice(&0i16.to_be_bytes()); // glyphDataFormat
    head
}

fn hhea_table() -> Vec<u8> {
    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    hhea.extend_from_slice(&800i16.to_be_bytes()); // ascender
    hhea.extend_from_slice(&(-200i16).to_be_bytes()); // descender
    hhea.extend_from_slice(&0i16.to_be_bytes()); // lineGap
    hhea.extend_from_slice(&[0u8; 24]);
    hhea.extend_from_slice(&1u16.to_be_bytes()); // numberOfHMetrics
    hhea
}

fn maxp_table(number_of_glyphs: u16) -> Vec<u8> {
    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5
    maxp.extend_from_slice(&number_of_glyphs.to_be_bytes());
    maxp
}

/// Builds the mandatory `head`/`hhea`/`maxp` tables plus `glyf` and a long-format
/// `loca` table.
pub(crate) fn glyf_font(glyphs: Vec<Vec<u8>>, number_of_glyphs: u16) -> Vec<u8> {
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for glyph in &glyphs {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        glyf.extend_from_slice(glyph);
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());

    build_sfnt(
        0x0001_0000,
        vec![
            (*b"glyf", glyf),
            (*b"head", head_table()),
            (*b"hhea", hhea_table()),
            (*b"loca", loca),
            (*b"maxp", maxp_table(number_of_glyphs)),
        ],
    )
}

// All glyphs below are encoded with the simplest legal form: every point is on-curve and
// uses two-byte (i16) deltas (`X_SHORT`/`Y_SHORT` unset, the "same" flag unset), and the
// instruction length is zero. This keeps the encoders tiny at the cost of larger data.

const ON_CURVE: u8 = 0x01;

/// A single simple (non-composite) contour through `points` in order.
/// `stored_bbox` is the bbox written into the glyph record; the parser deliberately skips
/// it and recomputes its own, so tests may pass a deliberately wrong value.
pub(crate) fn simple_glyph(points: &[(i16, i16)], stored_bbox: Rect) -> Vec<u8> {
    assert!(!points.is_empty(), "a simple glyph needs at least one point");

    let mut glyph = Vec::new();
    glyph.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
    glyph.extend_from_slice(&stored_bbox.x_min.to_be_bytes());
    glyph.extend_from_slice(&stored_bbox.y_min.to_be_bytes());
    glyph.extend_from_slice(&stored_bbox.x_max.to_be_bytes());
    glyph.extend_from_slice(&stored_bbox.y_max.to_be_bytes());
    glyph.extend_from_slice(&((points.len() as u16) - 1).to_be_bytes()); // single endpoint
    glyph.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
    glyph.resize(glyph.len() + points.len(), ON_CURVE);

    let mut prev_x = 0i16;
    for &(x, _) in points {
        glyph.extend_from_slice(&(x - prev_x).to_be_bytes());
        prev_x = x;
    }
    let mut prev_y = 0i16;
    for &(_, y) in points {
        glyph.extend_from_slice(&(y - prev_y).to_be_bytes());
        prev_y = y;
    }
    glyph
}

/// An empty glyph (numberOfContours == 0): no outline at all.
pub(crate) fn empty_glyph() -> Vec<u8> {
    let mut glyph = Vec::new();
    glyph.extend_from_slice(&0i16.to_be_bytes());
    glyph.extend_from_slice(&0i16.to_be_bytes()); // xMin
    glyph.extend_from_slice(&0i16.to_be_bytes()); // yMin
    glyph.extend_from_slice(&0i16.to_be_bytes()); // xMax
    glyph.extend_from_slice(&0i16.to_be_bytes()); // yMax
    glyph
}

const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;

#[derive(Clone, Copy)]
pub(crate) enum ComponentScale {
    Uniform(f32),
    Xy { x: f32, y: f32 },
}

pub(crate) struct Component {
    pub glyph_id: u16,
    pub dx: i16,
    pub dy: i16,
    pub scale: Option<ComponentScale>,
}

/// A composite glyph referencing `components` in order.
pub(crate) fn composite_glyph(components: &[Component]) -> Vec<u8> {
    let mut glyph = Vec::new();
    glyph.extend_from_slice(&(-1i16).to_be_bytes()); // numberOfContours < 0
    glyph.extend_from_slice(&0i16.to_be_bytes()); // xMin (ignored by outline)
    glyph.extend_from_slice(&0i16.to_be_bytes()); // yMin
    glyph.extend_from_slice(&0i16.to_be_bytes()); // xMax
    glyph.extend_from_slice(&0i16.to_be_bytes()); // yMax

    for (i, component) in components.iter().enumerate() {
        let mut flags = ARG_1_AND_2_ARE_WORDS | ARGS_ARE_XY_VALUES;
        if i + 1 < components.len() {
            flags |= MORE_COMPONENTS;
        }

        match component.scale {
            None => {}
            Some(ComponentScale::Uniform(_)) => flags |= WE_HAVE_A_SCALE,
            Some(ComponentScale::Xy { .. }) => flags |= WE_HAVE_AN_X_AND_Y_SCALE,
        }

        glyph.extend_from_slice(&flags.to_be_bytes());
        glyph.extend_from_slice(&component.glyph_id.to_be_bytes());
        glyph.extend_from_slice(&component.dx.to_be_bytes());
        glyph.extend_from_slice(&component.dy.to_be_bytes());

        let f2dot14 = |v: f32| (v * 16384.0).round() as i16;
        match component.scale {
            None => {}
            Some(ComponentScale::Uniform(s)) => {
                glyph.extend_from_slice(&f2dot14(s).to_be_bytes());
            }
            Some(ComponentScale::Xy { x, y }) => {
                glyph.extend_from_slice(&f2dot14(x).to_be_bytes());
                glyph.extend_from_slice(&f2dot14(y).to_be_bytes());
            }
        }
    }
    glyph
}

// ===========================================================================================
// glyf branch
// ===========================================================================================

#[test]
fn simple_glyf_glyph_recomputes_its_bbox_and_closes_with_a_line() {
    // A right triangle. The stored bbox deliberately lies: the parser skips the 8 bbox
    // bytes ("Skip bbox. We use calculated one.") and derives the bbox from the callbacks.
    let leaf = simple_glyph(&[(0, 0), (100, 0), (100, 200)], rect(-9, -9, 900, 900));
    let font = glyf_font(vec![empty_glyph(), leaf], 2);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 1);

    // `finish_contour` closes an all-on-curve contour with an explicit line back to the
    // start point, then emits `Z`.
    assert_eq!(path, "M 0 0 L 100 0 L 100 200 L 0 0 Z ");
    assert_eq!(bbox, Some(rect(0, 0, 100, 200)));
}

#[test]
fn glyf_table_bbox_returns_the_stored_bbox_while_outline_ignores_it() {
    // The two bbox contracts are deliberately different:
    // * `FaceTables.glyf.bbox()` returns the raw bbox stored in the glyph record;
    // * `Face::outline_glyph()` recomputes the bbox from the actual outline.
    let leaf = simple_glyph(&[(0, 0), (40, 0), (40, 60)], rect(1, 2, 3, 4));
    let font = glyf_font(vec![empty_glyph(), leaf], 2);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    assert_eq!(face.tables().glyf.unwrap().bbox(GlyphId(1)), Some(rect(1, 2, 3, 4)));

    let (bbox, _) = PathBuilder::outline(&face, 1);
    assert_eq!(bbox, Some(rect(0, 0, 40, 60)));
}

#[test]
fn two_level_composite_chains_transforms_in_parent_before_child_order() {
    // gid 1 = translate(10,20) then scale 1.5 applied to gid 0;
    // gid 2 = translate(30,40) then xy-scale (1.5,1.25) applied to gid 1.
    //
    // `outline_impl` combines matrices with
    // `Transform::combine(builder.transform, component.transform)`, i.e.
    // accumulated_parent * this_component, so the child matrix is applied to the
    // point first and the accumulated parent transform second.
    let leaf = simple_glyph(&[(0, 0), (10, 0), (10, 10)], rect(0, 0, 10, 10));
    let inner = composite_glyph(&[Component {
        glyph_id: 0,
        dx: 10,
        dy: 20,
        scale: Some(ComponentScale::Uniform(1.5)),
    }]);
    let outer = composite_glyph(&[Component {
        glyph_id: 1,
        dx: 30,
        dy: 40,
        scale: Some(ComponentScale::Xy { x: 1.5, y: 1.25 }),
    }]);
    let font = glyf_font(vec![leaf, inner, outer], 3);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 2);

    // With combine(A, B).f = A.b*B.e + A.d*B.f + A.f (b/c are zero here):
    //   (0,0)   -> inner (10,20)  -> outer (45,65)
    //   (10,0)  -> inner (25,20)  -> outer (67.5,65)
    //   (10,10) -> inner (25,35)  -> outer (67.5,83.75)
    // F2DOT14 represents both 1.5 (24576/16384) and 1.25 (20480/16384) exactly.
    assert_eq!(path, "M 45 65 L 67.5 65 L 67.5 83.75 L 45 65 Z ");
    assert_eq!(bbox, Some(rect(45, 65, 67, 83)));
}

#[test]
fn component_xy_offset_is_not_rescaled_by_a_later_parent_scale() {
    // Counter-example for the "offset is scaled" misconception. The component's own
    // offset lives in its own matrix (`e`/`f`), and matrix multiplication places it in the
    // *unscaled* coordinate space: combine(A, B).e = A.a*B.e + A.c*B.f + A.e.
    //
    // gid 1 = translate(100,200) then xy-scale (1.5,1) applied to a unit triangle.
    // If the offset were naively scaled, the vertex would land at (150,200); the
    // implemented ordering leaves the translate untouched: (100,200).
    let leaf = simple_glyph(&[(0, 0), (1, 0), (1, 1)], rect(0, 0, 1, 1));
    let comp = composite_glyph(&[Component {
        glyph_id: 0,
        dx: 100,
        dy: 200,
        scale: Some(ComponentScale::Xy { x: 1.5, y: 1.0 }),
    }]);
    let font = glyf_font(vec![leaf, comp], 2);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 1);

    assert_eq!(path, "M 100 200 L 101.5 200 L 101.5 201 L 100 200 Z ");
    assert_eq!(bbox, Some(rect(100, 200, 101, 201)));
}

#[test]
fn a_component_ring_is_stopped_by_the_depth_limit_and_returns_no_bbox() {
    // gid 1 first draws the leaf once, then references gid 2, which references gid 1:
    // a cycle preceded by one real contour. The depth guard
    // (`depth >= MAX_COMPONENTS`) stops the cycle after 32 frames, `?` propagates the
    // failure and no Rect is ever returned even though contours were already emitted.
    let leaf = simple_glyph(&[(0, 0), (5, 0), (5, 5)], rect(0, 0, 5, 5));
    let to_two = composite_glyph(&[
        Component { glyph_id: 0, dx: 0, dy: 0, scale: None },
        Component { glyph_id: 2, dx: 0, dy: 0, scale: None },
    ]);
    let to_one = composite_glyph(&[Component { glyph_id: 1, dx: 0, dy: 0, scale: None }]);
    let font = glyf_font(vec![leaf, to_two, to_one], 3);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 1);

    assert_eq!(bbox, None, "a truncated recursive outline must not produce a bbox");
    // Pull parsing: the leaf is redrawn once per even-numbered level (0, 2, .. 30, so
    // 16 times) before the depth guard stops the cycle. The important contract is the
    // pairing: `bbox == None` even though `path` is non-empty, so callers must discard
    // the commands when the result is `None`.
    let leaf_contour = "M 0 0 L 5 0 L 5 5 L 0 0 Z ";
    assert_eq!(path, leaf_contour.repeat(16));
}

#[test]
fn empty_glyf_glyph_and_unknown_glyph_id_have_no_outline() {
    // gid 1 is a zero-contour glyph; gid 2 does not exist (3 offsets, 2 glyphs).
    let font = glyf_font(vec![empty_glyph(), empty_glyph()], 2);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    for glyph in [1u16, 2] {
        let (bbox, path) = PathBuilder::outline(&face, glyph);
        assert_eq!(bbox, None, "glyph {glyph} must have no bbox");
        assert_eq!(path, "", "glyph {glyph} must emit no commands");
    }
}

#[test]
fn a_truncated_simple_glyf_glyph_is_rejected_without_a_partial_bbox() {
    // numberOfContours = 3, but the record ends after the endpoints header. The eager
    // check happens in `parse_simple_outline`:
    //   `Stream::read_array16::<u16>(3)` bounds the whole 6-byte span against the record
    //   before the lazy iterators are ever constructed, so this fails immediately rather
    //   than streaming a fraction of the points.
    let mut truncated = Vec::new();
    truncated.extend_from_slice(&3i16.to_be_bytes());
    truncated.extend_from_slice(&0i16.to_be_bytes()); // xMin
    truncated.extend_from_slice(&0i16.to_be_bytes()); // yMin
    truncated.extend_from_slice(&10i16.to_be_bytes()); // xMax
    truncated.extend_from_slice(&10i16.to_be_bytes()); // yMax
    truncated.extend_from_slice(&0u16.to_be_bytes()); // endPts[0]
    truncated.extend_from_slice(&4u16.to_be_bytes()); // endPts[1]
    // endPts[2] and everything afterwards is missing.
    let font = glyf_font(vec![empty_glyph(), truncated], 2);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 1);
    assert_eq!(bbox, None);
    assert_eq!(path, "");
}

// ===========================================================================================
// CFF branch
// ===========================================================================================

const CFF_HMOVETO: u8 = 22;
const CFF_RLINETO: u8 = 5; // lineto/rlineto/hlineto/vlineto share opcode 5
const CFF_RMOVETO: u8 = 21;
const CFF_RRCURVETO: u8 = 8;
const CFF_ENDCHAR: u8 = 14;

/// Encodes a CFF number. Only the one-byte and short-int (28 + i16) forms are needed by
/// the synthetic charstrings below.
fn cff_int(value: i16) -> Vec<u8> {
    if (-107..=107).contains(&value) {
        vec![(value + 139) as u8]
    } else {
        let mut out = vec![28];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }
}

/// A CFF INDEX with two-byte object offsets and one-byte offset values.
fn cff_index16(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(objects.len() as u16).to_be_bytes());
    if objects.is_empty() {
        return out;
    }
    out.push(1); // offSize
    let mut offset = 1u8;
    out.push(offset);
    for object in objects {
        offset += object.len() as u8;
        out.push(offset);
    }
    for object in objects {
        out.extend_from_slice(object);
    }
    out
}

/// Builds a minimal CFF table wrapping `char_strings` (default charset, default 0.001
/// FontMatrix, no Private DICT). The default ISOAdobe charset covers `.notdef`/`space`,
/// which is enough: glyphs are addressed by GID and charset mapping never affects
/// `outline_glyph`.
fn cff_table(char_strings: Vec<Vec<u8>>) -> Vec<u8> {
    // Fixed layout: header(4) | empty Name INDEX(2) | TopDICT INDEX | empty String
    // INDEX(2) | empty GlobalSubr INDEX(2) | CharStrings INDEX.
    //
    // The Top DICT is a one-byte number plus operator 17, so its INDEX is 7 bytes and
    // the CharStrings offset is 4 + 2 + 7 + 2 + 2 = 17. 17 is encoded in one byte, which
    // keeps the layout self-consistent without any back-patching.
    const CHAR_STRINGS_OFFSET: u8 = 17;

    let mut top_dict = cff_int(i16::from(CHAR_STRINGS_OFFSET));
    top_dict.push(17); // CharStrings offset operator
    assert_eq!(top_dict.len(), 2);

    let mut table = vec![1u8, 0, 4, 4];
    table.extend_from_slice(&cff_index16(&[])); // Name INDEX
    table.extend_from_slice(&cff_index16(&[top_dict])); // Top DICT INDEX
    table.extend_from_slice(&cff_index16(&[])); // String INDEX
    table.extend_from_slice(&cff_index16(&[])); // Global Subr INDEX
    assert_eq!(table.len(), usize::from(CHAR_STRINGS_OFFSET));
    table.extend_from_slice(&cff_index16(&char_strings));
    table
}

fn cff_font(char_strings: Vec<Vec<u8>>) -> Vec<u8> {
    let number_of_glyphs = char_strings.len() as u16;
    let cff = cff_table(char_strings);

    build_sfnt(
        0x4F54_544F, // 'OTTO'
        vec![
            (*b"CFF ", cff),
            (*b"head", head_table()),
            (*b"hhea", hhea_table()),
            (*b"maxp", maxp_table(number_of_glyphs)),
        ],
    )
}

#[test]
fn cff_charstring_outlines_a_triangle_and_closes_on_endchar() {
    // 100 0 hmoveto ; 50 50 0 100 -50 -150 rlineto ; endchar
    let mut triangle = Vec::new();
    triangle.extend_from_slice(&cff_int(100));
    triangle.push(CFF_HMOVETO);
    for value in [50i16, 50, 0, 100, -50, -150] {
        triangle.extend_from_slice(&cff_int(value));
    }
    triangle.push(CFF_RLINETO);
    triangle.push(CFF_ENDCHAR);

    let font = cff_font(vec![
        // gid 0: empty (.notdef) charstring, just endchar -> no contour.
        vec![CFF_ENDCHAR],
        // gid 1: another empty glyph so gid 2 is a "real" third glyph in the face.
        vec![CFF_ENDCHAR],
        triangle,
    ]);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 2);

    // Type2 paths are relative; the implicit closing segment is not emitted as a
    // drawing command by ttf-parser — `endchar` only calls `close()`. The CFF builder,
    // unlike glyf, never adds a synthetic line back to the contour's start.
    assert_eq!(path, "M 100 0 L 150 50 L 150 150 L 100 0 Z ");
    assert_eq!(bbox, Some(rect(100, 0, 150, 150)));

    // An endchar-only charstring is a glyph without an outline: ZeroBBox at table
    // level, surfaced as `None` by `outline_glyph`.
    let (empty_bbox, empty_path) = PathBuilder::outline(&face, 0);
    assert_eq!(empty_bbox, None);
    assert_eq!(empty_path, "");
}

#[test]
fn cff_missing_endchar_emits_no_close_and_returns_no_bbox() {
    // A line without endchar: the parser streams both commands, then
    // `parse_char_string` reports `MissingEndChar` before returning a bbox. Hence the
    // builder holds a partial, unclosed contour while the API result is `None`.
    let mut unterminated = Vec::new();
    unterminated.extend_from_slice(&cff_int(10));
    unterminated.push(CFF_HMOVETO);
    unterminated.extend_from_slice(&cff_int(20));
    unterminated.extend_from_slice(&cff_int(30));
    unterminated.push(CFF_RLINETO);

    let font = cff_font(vec![vec![CFF_ENDCHAR], unterminated]);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 1);

    assert_eq!(bbox, None, "a missing endchar must not yield a bbox");
    assert_eq!(
        path, "M 10 0 L 30 30 ",
        "partial commands are observable, but never paired with a bbox"
    );
}

#[test]
fn cff_control_points_participate_in_the_returned_bbox() {
    // 0 0 rmoveto ; 100 100 100 0 0 -100 rrcurveto ; endchar.
    // The returned bbox includes the off-curve control point (100,100) and is
    // therefore conservative rather than a tight cubic bound.
    let mut curve = Vec::new();
    curve.extend_from_slice(&cff_int(0));
    curve.extend_from_slice(&cff_int(0));
    curve.push(CFF_RMOVETO);
    for value in [100i16, 100, 100, 0, 0, -100] {
        curve.extend_from_slice(&cff_int(value));
    }
    curve.push(CFF_RRCURVETO);
    curve.push(CFF_ENDCHAR);

    let font = cff_font(vec![vec![CFF_ENDCHAR], curve]);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 1);

    assert_eq!(path, "M 0 0 C 100 100 200 100 200 0 Z ");
    assert_eq!(bbox, Some(rect(0, 0, 200, 100))); // y=100 comes only from the control point
}

// ===========================================================================================
// CFF2 branch (variable fonts)
// ===========================================================================================

#[cfg(feature = "variable-fonts")]
mod cff2_variable {
    use super::*;
    use ttf_parser::Tag;

    const CFF2_HMOVETO: u8 = 22;
    const CFF2_RLINETO: u8 = 5;
    const CFF2_BLEND: u8 = 16;

    fn cs_int(value: i32) -> Vec<u8> {
        if (-107..=107).contains(&value) {
            vec![(value + 139) as u8]
        } else if (108..=1131).contains(&value) {
            let n = value - 108;
            vec![((n >> 8) + 247) as u8, (n & 0xFF) as u8]
        } else {
            let mut out = vec![28];
            out.extend_from_slice(&(value as i16).to_be_bytes());
            out
        }
    }

    fn dict_int(value: i32) -> Vec<u8> {
        let mut out = vec![29];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }

    fn cff2_index(objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(objects.len() as u32).to_be_bytes());
        if objects.is_empty() {
            return out;
        }
        out.push(1); // offSize
        let mut offset = 1u8;
        out.push(offset);
        for object in objects {
            offset += object.len() as u8;
            out.push(offset);
        }
        for object in objects {
            out.extend_from_slice(object);
        }
        out
    }

    /// One ItemVariationStore with one axis, one region active over [0, 1] with peak 1
    /// (scalar is exactly the normalized coordinate) and one ItemVariationData with a
    /// single byte delta row.
    ///
    /// ItemVariationData: itemCount=1, wordDeltaCount=0, regionIndexCount=1,
    ///                    regionIndex[0]=0, deltaSet[0]=[delta] (one i8).
    fn item_variation_store(delta: i8) -> Vec<u8> {
        // Layout quirk pinned down empirically against `ItemVariationStore::parse`:
        // `cff2::Table::parse` skips the u16 length first; the captured `data` slice
        // therefore starts at the length field while the parser has already consumed the
        // format u16. The stored regionListOffset (12) and the ItemVariationData offset
        // (22) are resolved against that length-relative slice, so the RegionList sits at
        // data offset 12 (= format+10) and the data subtable at data offset 22
        // (= format+20, right after the 10-byte RegionList).
        let mut inner = Vec::new();
        inner.extend_from_slice(&1u16.to_be_bytes()); // format
        inner.extend_from_slice(&12u32.to_be_bytes()); // variationRegionListOffset
        inner.extend_from_slice(&1u16.to_be_bytes()); // itemVariationDataCount
        inner.extend_from_slice(&22u32.to_be_bytes()); // itemVariationDataOffsets[0]

        // VariationRegionList (at data offset 12): one axis, one region active over
        // [0, 1] with peak 1, so the scalar equals the normalized coordinate itself.
        inner.extend_from_slice(&1u16.to_be_bytes()); // axisCount
        inner.extend_from_slice(&1u16.to_be_bytes()); // regionCount
        inner.extend_from_slice(&0i16.to_be_bytes()); // startCoord = 0
        inner.extend_from_slice(&0x4000i16.to_be_bytes()); // peakCoord = 1.0
        inner.extend_from_slice(&0x4000i16.to_be_bytes()); // endCoord = 1.0

        // ItemVariationData (at data offset 22):
        inner.extend_from_slice(&1u16.to_be_bytes()); // itemCount
        inner.extend_from_slice(&0u16.to_be_bytes()); // shortDeltaCount
        inner.extend_from_slice(&1u16.to_be_bytes()); // regionIndexCount
        inner.extend_from_slice(&0u16.to_be_bytes()); // regionIndex[0] = 0
        inner.push(delta as u8); // the single delta set, one i8

        let mut data = Vec::new();
        data.extend_from_slice(&(inner.len() as u16).to_be_bytes()); // length covers the whole store
        data.extend_from_slice(&inner);
        data
    }

    /// Builds a complete CFF2 table: header, Top DICT, empty Global Subr INDEX,
    /// VariationStore, FDArray + Private DICT (both empty-but-valid), CharStrings INDEX.
    fn cff2_table(char_strings: Vec<Vec<u8>>, delta: i8) -> Vec<u8> {
        const HEADER_LEN: usize = 5;
        const TOP_DICT_LEN: usize = 19; // 6 + 6 + 7
        const FONT_DICT_LEN: usize = 11;
        const FD_ARRAY_LEN: usize = 4 + 1 + 2 + FONT_DICT_LEN;
        const PRIVATE_DICT_LEN: usize = 6;

        let global_subrs = cff2_index(&[]);
        let vstore = item_variation_store(delta);
        let local_subrs = cff2_index(&[]);
        let char_strings = cff2_index(&char_strings);

        let global_subrs_offset = HEADER_LEN + TOP_DICT_LEN;
        let vstore_offset = global_subrs_offset + global_subrs.len();
        let font_dict_index_offset = vstore_offset + vstore.len();
        let private_dict_offset = font_dict_index_offset + FD_ARRAY_LEN;
        let local_subrs_offset = private_dict_offset + PRIVATE_DICT_LEN;
        let char_strings_offset = local_subrs_offset + local_subrs.len();

        let mut top_dict = Vec::new();
        top_dict.extend_from_slice(&dict_int(char_strings_offset as i32));
        top_dict.push(17); // CharStrings offset
        top_dict.extend_from_slice(&dict_int(vstore_offset as i32));
        top_dict.push(24); // vstore offset
        top_dict.extend_from_slice(&dict_int(font_dict_index_offset as i32));
        top_dict.extend_from_slice(&[12, 36]); // FDArray offset (two-byte operator 1236)
        assert_eq!(top_dict.len(), TOP_DICT_LEN);

        let mut font_dict = Vec::new();
        font_dict.extend_from_slice(&dict_int(PRIVATE_DICT_LEN as i32));
        font_dict.extend_from_slice(&dict_int(private_dict_offset as i32));
        font_dict.push(18); // Private size and offset
        assert_eq!(font_dict.len(), FONT_DICT_LEN);

        let mut private_dict = Vec::new();
        private_dict.extend_from_slice(&dict_int(PRIVATE_DICT_LEN as i32));
        private_dict.push(19); // Local subrs offset points at the dict itself (unused)
        assert_eq!(private_dict.len(), PRIVATE_DICT_LEN);

        let mut data = vec![2u8, 0, HEADER_LEN as u8];
        data.extend_from_slice(&(TOP_DICT_LEN as u16).to_be_bytes()); // topDictLength
        data.extend_from_slice(&top_dict);
        data.extend_from_slice(&global_subrs);
        data.extend_from_slice(&vstore);
        data.extend_from_slice(&cff2_index(&[font_dict]));
        assert_eq!(data.len(), private_dict_offset);
        data.extend_from_slice(&private_dict);
        data.extend_from_slice(&local_subrs);
        data.extend_from_slice(&char_strings);
        data
    }

    fn fvar_table() -> Vec<u8> {
        let mut fvar = Vec::new();
        fvar.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
        fvar.extend_from_slice(&16u16.to_be_bytes()); // axesArrayOffset
        fvar.extend_from_slice(&0u16.to_be_bytes()); // reserved
        fvar.extend_from_slice(&1u16.to_be_bytes()); // axisCount
        fvar.extend_from_slice(&0u16.to_be_bytes()); // axisSize
        fvar.extend_from_slice(&0u16.to_be_bytes()); // instanceCount
        fvar.extend_from_slice(&0u16.to_be_bytes()); // instanceSize
        fvar.extend_from_slice(b"wght"); // tag
        fvar.extend_from_slice(&0i32.to_be_bytes()); // minValue (Fixed 0.0)
        fvar.extend_from_slice(&0i32.to_be_bytes()); // defaultValue
        fvar.extend_from_slice(&0x0001_0000i32.to_be_bytes()); // maxValue 1.0
        fvar.extend_from_slice(&0u16.to_be_bytes()); // flags
        fvar.extend_from_slice(&256u16.to_be_bytes()); // axisNameID
        fvar
    }

    fn cff2_font(char_strings: Vec<Vec<u8>>, delta: i8) -> Vec<u8> {
        let number_of_glyphs = 3u16;
        let cff2 = cff2_table(char_strings, delta);
        build_sfnt(
            0x4F54_544F, // 'OTTO'
            vec![
                (*b"CFF2", cff2),
                (*b"fvar", fvar_table()),
                (*b"head", head_table()),
                (*b"hhea", hhea_table()),
                (*b"maxp", maxp_table(number_of_glyphs)),
            ],
        )
    }

    // ---- gvar branch helpers (empty per-glyph variation data) ----

    /// Minimal gvar table with one axis, no shared tuples and empty per-glyph
    /// variation data (all short offsets zero). It parses, so the gvar branch is taken,
    /// but contributes no deltas.
    fn minimal_gvar(number_of_glyphs: u16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
        data.extend_from_slice(&1u16.to_be_bytes()); // axisCount
        data.extend_from_slice(&0u16.to_be_bytes()); // sharedTupleCount
        data.extend_from_slice(&0u32.to_be_bytes()); // sharedTupleOffset
        data.extend_from_slice(&number_of_glyphs.to_be_bytes()); // glyphVariationDataCount
        data.extend_from_slice(&0u16.to_be_bytes()); // flags: short offsets
        data.extend_from_slice(&20u32.to_be_bytes()); // glyphVariationDataArrayOffset
        for _ in 0..number_of_glyphs + 1 {
            data.extend_from_slice(&0u16.to_be_bytes()); // empty data for every glyph
        }
        data
    }

    /// Packs glyph records plus an empty gvar and the mandatory tables into a font.
    /// gvar requires a version 1.0 `maxp` (with the 28 trailing zero bytes).
    pub(crate) fn glyf_font_with_gvar(glyphs: Vec<Vec<u8>>) -> Vec<u8> {
        let number_of_glyphs = glyphs.len() as u16;
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for glyph in &glyphs {
            loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
            glyf.extend_from_slice(glyph);
        }
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());

        let mut maxp = Vec::new();
        maxp.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
        maxp.extend_from_slice(&number_of_glyphs.to_be_bytes());
        maxp.extend_from_slice(&[0u8; 28]);

        build_sfnt(
            0x0001_0000,
            vec![
                (*b"glyf", glyf),
                (*b"gvar", minimal_gvar(number_of_glyphs)),
                (*b"head", head_table()),
                (*b"hhea", hhea_table()),
                (*b"loca", loca),
                (*b"maxp", maxp),
            ],
        )
    }

    #[test]
    fn cff2_blend_moves_with_the_axis_and_never_emits_a_close() {
        // Charstring (k = 1 region):
        //   100 10 1 blend  hmoveto   (stack is value, delta, n; x = 100 + 10 * scalar)
        //   50 50 rlineto
        // CFF2 charstrings have no endchar, so the parser emits no `Z` at all.
        let mut char_string = Vec::new();
        char_string.extend_from_slice(&cs_int(100));
        char_string.extend_from_slice(&cs_int(10));
        char_string.extend_from_slice(&cs_int(1));
        char_string.push(CFF2_BLEND);
        char_string.push(CFF2_HMOVETO);
        char_string.extend_from_slice(&cs_int(50));
        char_string.extend_from_slice(&cs_int(50));
        char_string.push(CFF2_RLINETO);

        let font = cff2_font(vec![vec![], vec![], char_string], 10);
        let mut face = ttf_parser::Face::parse(&font, 0).unwrap();

        // Default instance: coordinate 0, scalar 0, delta contributes nothing.
        let (bbox, path) = PathBuilder::outline(&face, 2);
        assert_eq!(path, "M 100 0 L 150 50 ");
        assert_eq!(bbox, Some(rect(100, 0, 150, 50)));

        // Full wght instance: coordinate 1, scalar 1, the +10 delta applies once.
        face.set_variation(Tag::from_bytes(b"wght"), 1.0).unwrap();
        let (varied_bbox, varied_path) = PathBuilder::outline(&face, 2);
        assert_eq!(varied_path, "M 110 0 L 160 50 ");
        assert_eq!(varied_bbox, Some(rect(110, 0, 160, 50)));
    }

    #[test]
    fn face_with_only_glyf_never_reaches_cff2_and_an_outline_table_font_still_outlines() {
        // A font carrying the minimum required tables plus glyf/loca selects the glyf
        // branch even if a glyph record is absent for the requested id.
        let leaf = simple_glyph(&[(0, 0), (7, 0), (7, 9)], rect(0, 0, 7, 9));
        let font = glyf_font(vec![empty_glyph(), leaf], 2);
        let face = ttf_parser::Face::parse(&font, 0).unwrap();
        assert!(face.tables().glyf.is_some());

        let (bbox, path) = PathBuilder::outline(&face, 1);
        assert_eq!(path, "M 0 0 L 7 0 L 7 9 L 0 0 Z ");
        assert_eq!(bbox, Some(rect(0, 0, 7, 9)));
    }

    #[test]
    fn gvar_ring_returns_no_bbox_like_the_glyf_branch() {
        // gid 1 draws the leaf, then references gid 2, which references gid 1; gvar is
        // present and parses, so this goes through the gvar branch with no deltas. The
        // depth guard stops the recursion and no bbox is returned.
        let leaf = simple_glyph(&[(0, 0), (5, 0), (5, 5)], rect(0, 0, 5, 5));
        let to_two = composite_glyph(&[
            Component { glyph_id: 0, dx: 0, dy: 0, scale: None },
            Component { glyph_id: 2, dx: 0, dy: 0, scale: None },
        ]);
        let to_one = composite_glyph(&[Component { glyph_id: 1, dx: 0, dy: 0, scale: None }]);
        let font = glyf_font_with_gvar(vec![leaf, to_two, to_one]);
        let face = ttf_parser::Face::parse(&font, 0).unwrap();
        assert!(face.tables().gvar.is_some());

        let (bbox, _path) = PathBuilder::outline(&face, 1);
        assert_eq!(
            bbox, None,
            "the gvar path must not turn a depth-limited partial outline into a bbox"
        );
    }
}

#[cfg(feature = "variable-fonts")]
#[test]
fn gvar_fan_out_budget_exhaustion_returns_no_bbox() {
    // Empirical companion to the glyf fan-out budget test (`tests/tables/glyf.rs`), this
    // time through the gvar branch: an empty per-glyph gvar table parses and adds no
    // deltas, while the branching^depth component graph exhausts MAX_COMPONENT_VISITS.
    // The returned contract must still be `None`, never a bbox from the contours already
    // streamed.
    const BRANCHING: u16 = 3;
    const DEPTH: u16 = 24;
    const ARG_WORDS: u16 = 0x0001;
    const ARG_XY: u16 = 0x0002;
    const MORE: u16 = 0x0020;

    let mut glyphs = vec![simple_glyph(
        &[(10, 10), (30, 10), (30, 30)],
        rect(10, 10, 30, 30),
    )];
    for level in 1..=DEPTH {
        let mut glyph = Vec::new();
        glyph.extend_from_slice(&(-1i16).to_be_bytes());
        glyph.extend_from_slice(&10i16.to_be_bytes());
        glyph.extend_from_slice(&10i16.to_be_bytes());
        glyph.extend_from_slice(&30i16.to_be_bytes());
        glyph.extend_from_slice(&30i16.to_be_bytes());
        for i in 0..BRANCHING {
            let mut flags = ARG_WORDS | ARG_XY;
            if i + 1 < BRANCHING {
                flags |= MORE;
            }
            glyph.extend_from_slice(&flags.to_be_bytes());
            glyph.extend_from_slice(&(level - 1).to_be_bytes());
            glyph.extend_from_slice(&0i16.to_be_bytes());
            glyph.extend_from_slice(&0i16.to_be_bytes());
        }
        glyphs.push(glyph);
    }

    let font = cff2_variable::glyf_font_with_gvar(glyphs);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, _) = PathBuilder::outline(&face, DEPTH);
    assert_eq!(bbox, None);
}

// ===========================================================================================
// Resource bounds
// ===========================================================================================

#[test]
fn a_glyph_declaring_more_points_than_its_record_holds_is_rejected_eagerly() {
    // The point count is derived from the last endpoint (`endpoints.last() + 1`) and the
    // endpoints array itself is bounds-checked eagerly by
    // `Stream::read_array16::<u16>(number_of_contours)`: 65535 endpoints require 131070
    // bytes, but this record contains only the u16. The font must parse (table parsing is
    // lazy) while outlining returns `None` immediately, without streaming any points and
    // without touching the (absent) flags/coordinate sections.
    let mut huge = Vec::new();
    huge.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
    huge.extend_from_slice(&0i16.to_be_bytes()); // xMin
    huge.extend_from_slice(&0i16.to_be_bytes()); // yMin
    huge.extend_from_slice(&10i16.to_be_bytes()); // xMax
    huge.extend_from_slice(&10i16.to_be_bytes()); // yMax
    huge.extend_from_slice(&65534u16.to_be_bytes()); // the single endpoint claims 65535 points
    // instructionLength, flags and all coordinates are missing.

    let font = glyf_font(vec![empty_glyph(), huge], 2);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, 1);
    assert_eq!(bbox, None);
    assert_eq!(path, "");
}

#[test]
fn a_depth_chain_at_the_component_limit_still_outlines() {
    // Documents the inclusive boundary: 32 is the maximum *depth* (`depth >=
    // MAX_COMPONENTS` stops at 32). A chain of 31 nested components (depth values 0
    // through 31) is therefore accepted, while the ring test above proves depth 32 is
    // rejected. Here every level translates by +1 on x, so the leaf's first vertex ends
    // up at x = 31.
    let leaf = simple_glyph(&[(0, 0), (2, 0), (2, 2)], rect(0, 0, 2, 2));

    let mut glyphs = vec![leaf];
    for level in 0..31 {
        glyphs.push(composite_glyph(&[Component {
            glyph_id: level,
            dx: 1,
            dy: 0,
            scale: None,
        }]));
    }
    let number_of_glyphs = glyphs.len() as u16;
    let font = glyf_font(glyphs, number_of_glyphs);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, path) = PathBuilder::outline(&face, number_of_glyphs - 1);

    assert_eq!(path, "M 31 0 L 33 0 L 33 2 L 31 0 Z ");
    assert_eq!(bbox, Some(rect(31, 0, 33, 2)));
}

#[test]
fn a_depth_chain_one_frame_past_the_component_limit_is_rejected() {
    // 32 nested components reach depth 32 and are stopped; the bbox is `None`.
    let leaf = simple_glyph(&[(0, 0), (2, 0), (2, 2)], rect(0, 0, 2, 2));

    let mut glyphs = vec![leaf];
    for level in 0..32 {
        glyphs.push(composite_glyph(&[Component {
            glyph_id: level,
            dx: 0,
            dy: 0,
            scale: None,
        }]));
    }
    let number_of_glyphs = glyphs.len() as u16;
    let font = glyf_font(glyphs, number_of_glyphs);
    let face = ttf_parser::Face::parse(&font, 0).unwrap();

    let (bbox, _) = PathBuilder::outline(&face, number_of_glyphs - 1);
    assert_eq!(bbox, None);
}

#[test]
fn lazy_array16_checks_each_access_against_its_constructed_span() {
    // `LazyArray16::get` performs two checks on every access:
    //   1. `index < len()` where len derives from the slice length, and
    //   2. `self.data.get(start..end)` re-checks the span.
    // This is the lazy, per-element counterpart to the eager whole-span check in
    // `Stream::read_array16`, which `parse_simple_outline` uses for the endpoints table.
    use ttf_parser::LazyArray16;

    let three = LazyArray16::<u16>::new(&[0, 1, 0, 2, 0, 3]);
    assert_eq!(three.len(), 3);
    assert_eq!(three.get(0), Some(1));
    assert_eq!(three.get(2), Some(3));
    assert_eq!(three.get(3), None, "index past the length is rejected per access");
    assert_eq!(three.last(), Some(3));

    // Four bytes back exactly two u16s; a third element is rejected on access even
    // though its index fits in a u16.
    let two = LazyArray16::<u16>::new(&[0u8, 1, 0, 2]);
    assert_eq!(two.len(), 2);
    assert_eq!(two.get(2), None);

    // A trailing half element rounds the length down and is never readable.
    let truncated = LazyArray16::<u16>::new(&[0u8, 1, 0]);
    assert_eq!(truncated.len(), 1);
    assert_eq!(truncated.get(1), None);
}
