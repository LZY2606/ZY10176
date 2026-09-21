// Face-level characterization tests for `Face::outline_glyph`: which table gets
// consulted, in which order, and what comes back. See docs/outline-glyph.md for
// the full branch diagram these tests pin down.

use std::fmt::Write;

use ttf_parser::{Face, GlyphId, Rect};

struct Builder(String);

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        write!(&mut self.0, "M {} {} ", x, y).unwrap();
    }

    fn line_to(&mut self, x: f32, y: f32) {
        write!(&mut self.0, "L {} {} ", x, y).unwrap();
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        write!(&mut self.0, "Q {} {} {} {} ", x1, y1, x, y).unwrap();
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        write!(&mut self.0, "C {} {} {} {} {} {} ", x1, y1, x2, y2, x, y).unwrap();
    }

    fn close(&mut self) {
        write!(&mut self.0, "Z ").unwrap();
    }
}

fn head() -> Vec<u8> {
    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    head.extend_from_slice(&0u32.to_be_bytes());           // fontRevision
    head.extend_from_slice(&0u32.to_be_bytes());           // checkSumAdjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
    head.extend_from_slice(&0u16.to_be_bytes());           // flags
    head.extend_from_slice(&1000u16.to_be_bytes());        // unitsPerEm
    head.extend_from_slice(&0u64.to_be_bytes());           // created
    head.extend_from_slice(&0u64.to_be_bytes());           // modified
    head.extend_from_slice(&0i16.to_be_bytes());           // xMin
    head.extend_from_slice(&0i16.to_be_bytes());           // yMin
    head.extend_from_slice(&1000i16.to_be_bytes());        // xMax
    head.extend_from_slice(&1000i16.to_be_bytes());        // yMax
    head.extend_from_slice(&0u16.to_be_bytes());           // macStyle
    head.extend_from_slice(&0u16.to_be_bytes());           // lowestRecPPEM
    head.extend_from_slice(&2i16.to_be_bytes());           // fontDirectionHint
    head.extend_from_slice(&1i16.to_be_bytes());           // indexToLocFormat: long
    head.extend_from_slice(&0i16.to_be_bytes());           // glyphDataFormat
    assert_eq!(head.len(), 54);
    head
}

fn hhea() -> Vec<u8> {
    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    hhea.extend_from_slice(&800i16.to_be_bytes());         // ascender
    hhea.extend_from_slice(&(-200i16).to_be_bytes());      // descender
    hhea.extend_from_slice(&0i16.to_be_bytes());           // lineGap
    hhea.extend_from_slice(&[0u8; 24]);                    // through metricDataFormat
    hhea.extend_from_slice(&1u16.to_be_bytes());           // numberOfHMetrics
    assert_eq!(hhea.len(), 36);
    hhea
}

fn maxp(number_of_glyphs: u16) -> Vec<u8> {
    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5
    maxp.extend_from_slice(&number_of_glyphs.to_be_bytes());
    maxp
}

/// Assembles an sfnt font. Table records are sorted by tag as the spec requires.
fn sfnt(sfnt_version: &[u8; 4], mut tables: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by_key(|(tag, _)| *tag);

    let mut font = Vec::new();
    font.extend_from_slice(sfnt_version);
    font.extend_from_slice(&(tables.len() as u16).to_be_bytes()); // numTables
    font.extend_from_slice(&0u16.to_be_bytes()); // searchRange
    font.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
    font.extend_from_slice(&0u16.to_be_bytes()); // rangeShift

    let mut offset = 12 + 16 * tables.len() as u32;
    for (tag, data) in &tables {
        font.extend_from_slice(*tag);
        font.extend_from_slice(&0u32.to_be_bytes()); // checkSum, unchecked
        font.extend_from_slice(&offset.to_be_bytes());
        font.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &tables {
        font.extend_from_slice(data);
    }
    font
}

// One closed contour (10,10) -> (30,10) -> (30,30).
fn leaf_glyph() -> Vec<u8> {
    let mut glyph = Vec::new();
    glyph.extend_from_slice(&1i16.to_be_bytes());  // numberOfContours
    glyph.extend_from_slice(&10i16.to_be_bytes()); // xMin
    glyph.extend_from_slice(&10i16.to_be_bytes()); // yMin
    glyph.extend_from_slice(&30i16.to_be_bytes()); // xMax
    glyph.extend_from_slice(&30i16.to_be_bytes()); // yMax
    glyph.extend_from_slice(&2u16.to_be_bytes());  // endPtsOfContours[0], so 3 points
    glyph.extend_from_slice(&0u16.to_be_bytes());  // instructionLength
    // ON_CURVE | X_SHORT | Y_SHORT | X_POSITIVE_SHORT | Y_POSITIVE_SHORT
    glyph.extend_from_slice(&[0x37, 0x37, 0x37]);
    glyph.extend_from_slice(&[10, 20, 0]); // x deltas
    glyph.extend_from_slice(&[10, 0, 20]); // y deltas
    glyph
}

fn glyf_and_loca(glyphs: &[Vec<u8>]) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for glyph in glyphs {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        glyf.extend_from_slice(glyph);
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
    (glyf, loca)
}

/// A minimal CFF table with a single charstring.
fn cff_table(char_string: &[u8]) -> Vec<u8> {
    // Header (4) + empty Name INDEX (2) + Top DICT INDEX (2+1+2+6) + empty String
    // INDEX (2) + empty Global Subr INDEX (2) = 21.
    let char_strings_offset: i32 = 21;

    let mut cff = Vec::new();
    cff.extend_from_slice(&[1, 0, 4, 4]); // major, minor, hdrSize, offSize
    cff.extend_from_slice(&0u16.to_be_bytes()); // Name INDEX: count = 0

    // Top DICT INDEX with a single entry: `charStringsOffset 17`.
    cff.extend_from_slice(&1u16.to_be_bytes()); // count
    cff.push(1); // offset size
    cff.extend_from_slice(&[1, 7]); // offsets
    cff.push(29); // 5-byte DICT integer
    cff.extend_from_slice(&char_strings_offset.to_be_bytes());
    cff.push(17); // CharStrings offset operator

    cff.extend_from_slice(&0u16.to_be_bytes()); // String INDEX: count = 0
    cff.extend_from_slice(&0u16.to_be_bytes()); // Global Subr INDEX: count = 0

    assert_eq!(cff.len(), char_strings_offset as usize);
    cff.extend_from_slice(&1u16.to_be_bytes()); // CharStrings INDEX: count = 1
    cff.push(1); // offset size
    cff.extend_from_slice(&[1, char_string.len() as u8 + 1]);
    cff.extend_from_slice(char_string);
    cff
}

// `100 100 rmoveto, 50 50 rlineto, endchar` as a Type 2 charstring.
fn line_char_string() -> Vec<u8> {
    const RMOVE_TO: u8 = 21;
    const RLINE_TO: u8 = 5;
    const ENDCHAR: u8 = 14;
    fn int(value: i32) -> u8 {
        (value + 139) as u8 // -107..=107 single-byte form
    }
    vec![int(100), int(100), RMOVE_TO, int(50), int(50), RLINE_TO, ENDCHAR]
}

#[test]
fn cff_charstring_outlines_through_face() {
    let data = sfnt(
        b"OTTO",
        vec![
            (b"CFF ", cff_table(&line_char_string())),
            (b"head", head()),
            (b"hhea", hhea()),
            (b"maxp", maxp(1)),
        ],
    );
    let face = Face::parse(&data, 0).unwrap();

    let mut builder = Builder(String::new());
    let bbox = face.outline_glyph(GlyphId(0), &mut builder);

    // CFF1 closes the contour at `endchar`.
    assert_eq!(builder.0, "M 100 100 L 150 150 Z ");
    assert_eq!(bbox, Some(Rect { x_min: 100, y_min: 100, x_max: 150, y_max: 150 }));
}

// `Face::outline_glyph` consults `glyf` before `CFF `, so when both are present the
// `glyf` outline wins and the charstring is never interpreted.
#[test]
fn glyf_takes_precedence_over_cff() {
    let (glyf, loca) = glyf_and_loca(&[leaf_glyph()]);
    let data = sfnt(
        &0x0001_0000u32.to_be_bytes(),
        vec![
            (b"CFF ", cff_table(&line_char_string())),
            (b"glyf", glyf),
            (b"head", head()),
            (b"hhea", hhea()),
            (b"loca", loca),
            (b"maxp", maxp(1)),
        ],
    );
    let face = Face::parse(&data, 0).unwrap();

    let mut builder = Builder(String::new());
    let bbox = face.outline_glyph(GlyphId(0), &mut builder);

    // The glyf contour, not the CFF line at (100,100) -> (150,150).
    assert_eq!(builder.0, "M 10 10 L 30 10 L 30 30 L 10 10 Z ");
    assert_eq!(bbox, Some(Rect { x_min: 10, y_min: 10, x_max: 30, y_max: 30 }));
}

// With neither `gvar`/`glyf` nor `CFF `/`CFF2` there is nothing to outline.
#[test]
fn font_without_outline_sources_returns_none() {
    let data = sfnt(
        &0x0001_0000u32.to_be_bytes(),
        vec![(b"head", head()), (b"hhea", hhea()), (b"maxp", maxp(1))],
    );
    let face = Face::parse(&data, 0).unwrap();

    let mut builder = Builder(String::new());
    assert_eq!(face.outline_glyph(GlyphId(0), &mut builder), None);
    assert_eq!(builder.0, "");
}
