use ttf_parser::{Face, GlyphId, OutlineBuilder};

static VARIABLE: &[u8] = include_bytes!("../fonts/colr_1_variable.ttf");

// Counts emitted segments; the shape does not matter here, only that outlining ran.
#[derive(Default)]
struct SegmentCounter(usize);

impl OutlineBuilder for SegmentCounter {
    fn move_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }

    fn line_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }

    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }

    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }

    fn close(&mut self) {}
}

fn outline_every_glyph(face: &Face) -> usize {
    let mut counter = SegmentCounter::default();
    for id in 0..face.number_of_glyphs() {
        face.outline_glyph(GlyphId(id), &mut counter);
    }

    counter.0
}

// Regression test for https://github.com/harfbuzz/ttf-parser/issues/205. Applying a variation
// is what reaches `parse_variation_tuples`, which is where the struct-size `debug_assert!`
// used to sit; nothing else in the suite outlines a `gvar` glyph, so the whole path was
// uncovered and the assert only ever fired in downstream applications.
#[test]
fn applying_a_variation_outlines_glyphs_from_gvar() {
    let mut face = Face::parse(VARIABLE, 0).unwrap();
    let unvaried = outline_every_glyph(&face);

    let axis = face.variation_axes().get(0).unwrap().tag;
    face.set_variation(axis, 0.7).unwrap();

    assert_eq!(unvaried, 1270);
    assert_eq!(outline_every_glyph(&face), 1270);
}
