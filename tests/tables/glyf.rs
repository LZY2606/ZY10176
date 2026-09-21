use std::fmt::Write;

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

#[test]
fn endless_loop() {
    let data = b"\x00\x01\x00\x00\x00\x0f\x00\x10\x00PTT-W\x002h\xd7\x81x\x00\
    \x00\x00?L\xbaN\x00c\x9a\x9e\x8f\x96\xe3\xfeu\xff\x00\xb2\x00@\x03\x00\xb8\
    cvt 5:\x00\x00\x00\xb5\xf8\x01\x00\x03\x9ckEr\x92\xd7\xe6\x98M\xdc\x00\x00\
    \x03\xe0\x00\x00\x00dglyf\"\t\x15`\x00\x00\x03\xe0\x00\x00\x00dglyf\"\t\x15\
    `\x00\x00\x00 \x00\x00\x00\xfc\x97\x9fmx\x87\xc9\xc8\xfe\x00\x00\xbad\xff\
    \xff\xf1\xc8head\xc7\x17\xce[\x00\x00\x00\xfc\x00\x00\x006hhea\x03\xc6\x05\
    \xe4\x00\x00\x014\x00\x00\x00$hmtx\xc9\xfdq\xed\x00\x00\xb5\xf8\x01\x00\x03\
    \x9ckEr\x92\xd7\xe6\xdch\x00\x00\xc9d\x00\x00\x04 loca\x00M\x82\x11\x00\x00\
    \x00\x06\x00\x00\x00\xa0maxp\x17\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00 name\
    \xf4\xd6\xfe\xad\x00OTTO\x00\x02gpost5;5\xe1\x00\x00\xb0P\x00\x00\x01\xf0perp%\
    \xb0{\x04\x93D\x00\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x01\x00\x00\xe1!yf%1\
    \x08\x95\x00\x00\x00\x00\x00\xaa\x06\x80fmtx\x02\x00\x00\x00\x00\x00\x00\x00\
    \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\
    \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00a\xcc\xff\
    \xce\x03CCCCCCCCC\x00\x00\x00\x00\x00C\x00\x00\x00\x00\xb5\xf8\x01\x00\x00\x9c";

    let face = ttf_parser::Face::parse(data, 0).unwrap();
    let _ = face.outline_glyph(ttf_parser::GlyphId(0), &mut Builder(String::new()));
}
// `MAX_COMPONENTS` bounds only the *depth* of a component chain, never its fan-out.
// A composite glyph whose components all point at one shared child glyph forces
// `branching^depth` component visits while using only `depth + 1` distinct glyphs and
// staying far below the depth cap. `MAX_COMPONENT_VISITS` bounds the total instead. ~keep

/// Counts drawing commands without accumulating a multi-megabyte path string.
#[derive(Default)]
struct CountingBuilder {
    contours: u32,
}

impl ttf_parser::OutlineBuilder for CountingBuilder {
    fn move_to(&mut self, _x: f32, _y: f32) {
        self.contours += 1;
    }
    fn line_to(&mut self, _x: f32, _y: f32) {}
    fn quad_to(&mut self, _x1: f32, _y1: f32, _x: f32, _y: f32) {}
    fn curve_to(&mut self, _x1: f32, _y1: f32, _x2: f32, _y2: f32, _x: f32, _y: f32) {}
    fn close(&mut self) {}
}

// Glyph 0: one closed contour (10,10) -> (30,10) -> (30,30).
fn diamond_leaf_glyph() -> Vec<u8> {
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

// A composite glyph with `branching` components, all referencing `child`.
fn diamond_composite_glyph(child: u16, branching: u16) -> Vec<u8> {
    const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
    const ARGS_ARE_XY_VALUES: u16 = 0x0002;
    const MORE_COMPONENTS: u16 = 0x0020;

    let mut glyph = Vec::new();
    glyph.extend_from_slice(&(-1i16).to_be_bytes()); // numberOfContours < 0
    glyph.extend_from_slice(&10i16.to_be_bytes());   // xMin
    glyph.extend_from_slice(&10i16.to_be_bytes());   // yMin
    glyph.extend_from_slice(&30i16.to_be_bytes());   // xMax
    glyph.extend_from_slice(&30i16.to_be_bytes());   // yMax

    for i in 0..branching {
        let mut flags = ARG_1_AND_2_ARE_WORDS | ARGS_ARE_XY_VALUES;
        if i + 1 < branching {
            flags |= MORE_COMPONENTS;
        }
        glyph.extend_from_slice(&flags.to_be_bytes());
        glyph.extend_from_slice(&child.to_be_bytes());
        glyph.extend_from_slice(&0i16.to_be_bytes()); // dx
        glyph.extend_from_slice(&0i16.to_be_bytes()); // dy
    }
    glyph
}

/// Builds a font where glyph 0 is a simple leaf and glyphs `1..=depth` are each
/// composite with `branching` components, all pointing at the glyph below them.
/// Outlining glyph `depth` costs `branching^depth` component visits.
fn diamond_font(branching: u16, depth: u16) -> Vec<u8> {
    let mut glyphs = vec![diamond_leaf_glyph()];
    for level in 1..=depth {
        glyphs.push(diamond_composite_glyph(level - 1, branching));
    }

    let number_of_glyphs = glyphs.len() as u16;

    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for glyph in &glyphs {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        glyf.extend_from_slice(glyph);
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());

    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    head.extend_from_slice(&0u32.to_be_bytes());           // fontRevision
    head.extend_from_slice(&0u32.to_be_bytes());           // checkSumAdjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
    head.extend_from_slice(&0u16.to_be_bytes());           // flags
    head.extend_from_slice(&1000u16.to_be_bytes());        // unitsPerEm
    head.extend_from_slice(&0u64.to_be_bytes());           // created
    head.extend_from_slice(&0u64.to_be_bytes());           // modified
    head.extend_from_slice(&10i16.to_be_bytes());          // xMin
    head.extend_from_slice(&10i16.to_be_bytes());          // yMin
    head.extend_from_slice(&30i16.to_be_bytes());          // xMax
    head.extend_from_slice(&30i16.to_be_bytes());          // yMax
    head.extend_from_slice(&0u16.to_be_bytes());           // macStyle
    head.extend_from_slice(&0u16.to_be_bytes());           // lowestRecPPEM
    head.extend_from_slice(&2i16.to_be_bytes());           // fontDirectionHint
    head.extend_from_slice(&1i16.to_be_bytes());           // indexToLocFormat: long
    head.extend_from_slice(&0i16.to_be_bytes());           // glyphDataFormat
    assert_eq!(head.len(), 54);

    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    hhea.extend_from_slice(&800i16.to_be_bytes());         // ascender
    hhea.extend_from_slice(&(-200i16).to_be_bytes());      // descender
    hhea.extend_from_slice(&0i16.to_be_bytes());           // lineGap
    hhea.extend_from_slice(&[0u8; 24]);                    // through metricDataFormat
    hhea.extend_from_slice(&1u16.to_be_bytes());           // numberOfHMetrics
    assert_eq!(hhea.len(), 36);

    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5
    maxp.extend_from_slice(&number_of_glyphs.to_be_bytes());

    // Table records must be sorted by tag.
    let tables: [(&[u8; 4], Vec<u8>); 5] = [
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"loca", loca),
        (b"maxp", maxp),
    ];

    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // sfntVersion
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

#[test]
fn shared_component_fan_out_is_bounded_by_the_visit_budget() {
    // branching=3, depth=24 needs only 25 glyphs and nests 24 deep, comfortably
    // below the depth cap of MAX_COMPONENTS=32, so the depth guard never fires.
    // Unguarded it costs (3^25 - 1) / 2 = 423_644_304_721 component visits, which
    // is hours of work. MAX_COMPONENT_VISITS stops it after 100_000, so this test
    // finishes in milliseconds and `checked_sub` bails out to `None`.
    const BRANCHING: u16 = 3;
    const DEPTH: u16 = 24;
    // The guarded run is ~100_000 visits; a bound this loose still fails loudly
    // if the budget is ever dropped.
    const TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

    let data = diamond_font(BRANCHING, DEPTH);
    let face = ttf_parser::Face::parse(&data, 0).unwrap();

    let mut builder = CountingBuilder::default();
    let started_at = std::time::Instant::now();
    let bbox = face.outline_glyph(ttf_parser::GlyphId(DEPTH), &mut builder);
    let elapsed = started_at.elapsed();

    assert_eq!(bbox, None);
    assert!(elapsed < TIME_LIMIT, "outlining took {:?}", elapsed);
}

#[test]
fn shared_component_fan_out_within_the_visit_budget_still_outlines() {
    // branching=2, depth=3 is 15 visits total, so the budget must not interfere.
    // The leaf is reached 2^3 = 8 times and every visit draws its contour.
    const BRANCHING: u16 = 2;
    const DEPTH: u16 = 3;

    let data = diamond_font(BRANCHING, DEPTH);
    let face = ttf_parser::Face::parse(&data, 0).unwrap();

    let mut builder = CountingBuilder::default();
    let bbox = face.outline_glyph(ttf_parser::GlyphId(DEPTH), &mut builder);

    assert_eq!(bbox, Some(ttf_parser::Rect { x_min: 10, y_min: 10, x_max: 30, y_max: 30 }));
    assert_eq!(builder.contours, 8);
}

#[test]
fn diamond_leaf_glyph_outlines_a_single_contour() {
    // Proves the hand-built font is sound rather than merely being rejected.
    let data = diamond_font(2, 1);
    let face = ttf_parser::Face::parse(&data, 0).unwrap();

    let mut builder = Builder(String::new());
    let bbox = face.outline_glyph(ttf_parser::GlyphId(0), &mut builder);

    // The glyf builder closes a contour with an explicit line back to its start.
    assert_eq!(builder.0, "M 10 10 L 30 10 L 30 30 L 10 10 Z ");
    assert_eq!(bbox, Some(ttf_parser::Rect { x_min: 10, y_min: 10, x_max: 30, y_max: 30 }));
}
