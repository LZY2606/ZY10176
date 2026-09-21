use std::num::NonZeroU16;
use ttf_parser::head::IndexToLocationFormat;
use ttf_parser::loca::Table;
use ttf_parser::GlyphId;
use crate::{convert, Unit::*};

macro_rules! nzu16 {
    ($n:expr) => { NonZeroU16::new($n).unwrap() };
}

// The largest `maxp.numGlyphs`. A font with that many glyphs needs `MAX_GLYPHS + 1` loca offsets.
const MAX_GLYPHS: u16 = u16::MAX;
const MAX_OFFSETS: u32 = MAX_GLYPHS as u32 + 1;

// Short format stores `offset / 2`, so entry `i` denotes the byte offset `i * 2`.
fn short_offsets(count: u32) -> Vec<u8> {
    convert(&(0..count).map(|i| UInt16(i as u16)).collect::<Vec<_>>())
}

fn long_offsets(count: u32) -> Vec<u8> {
    convert(&(0..count).map(|i| UInt32(i * 4)).collect::<Vec<_>>())
}

#[test]
fn short_format_simple_case() {
    let data = convert(&[
        UInt16(0), // offset [0] -> 0
        UInt16(5), // offset [1] -> 10
        UInt16(10), // offset [2] -> 20
        UInt16(15), // offset [3] -> 30
    ]);

    let table = Table::parse(nzu16!(3), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 4);
    assert!(!table.is_empty());
    assert_eq!(table.glyph_range(GlyphId(0)), Some(0..10));
    assert_eq!(table.glyph_range(GlyphId(1)), Some(10..20));
    assert_eq!(table.glyph_range(GlyphId(2)), Some(20..30));
    assert_eq!(table.glyph_range(GlyphId(3)), None);
}

#[test]
fn long_format_simple_case() {
    let data = convert(&[
        UInt32(0), // offset [0]
        UInt32(12), // offset [1]
        UInt32(48), // offset [2]
    ]);

    let table = Table::parse(nzu16!(2), IndexToLocationFormat::Long, &data).unwrap();
    assert_eq!(table.len(), 3);
    assert_eq!(table.glyph_range(GlyphId(0)), Some(0..12));
    assert_eq!(table.glyph_range(GlyphId(1)), Some(12..48));
    assert_eq!(table.glyph_range(GlyphId(2)), None);
}

#[test]
fn glyph_zero_on_a_single_glyph_table() {
    let data = convert(&[
        UInt16(0), // offset [0] -> 0
        UInt16(7), // offset [1] -> 14
    ]);

    let table = Table::parse(nzu16!(1), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table.glyph_range(GlyphId(0)), Some(0..14));
    assert_eq!(table.glyph_range(GlyphId(1)), None);
}

#[test]
fn empty_table_has_no_glyph_ranges() {
    let short = Table::parse(nzu16!(1), IndexToLocationFormat::Short, &[]).unwrap();
    assert_eq!(short.len(), 0);
    assert!(short.is_empty());
    assert_eq!(short.glyph_range(GlyphId(0)), None);

    let long = Table::parse(nzu16!(1), IndexToLocationFormat::Long, &[]).unwrap();
    assert_eq!(long.len(), 0);
    assert!(long.is_empty());
    assert_eq!(long.glyph_range(GlyphId(0)), None);
}

// A single offset is not enough to form any range.
#[test]
fn table_with_one_offset_has_no_glyph_ranges() {
    let data = convert(&[
        UInt16(0), // offset [0]
    ]);

    let table = Table::parse(nzu16!(1), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 1);
    assert_eq!(table.glyph_range(GlyphId(0)), None);
}

// Fewer offsets than `maxp.numGlyphs + 1` is tolerated; the trailing glyphs simply have no range.
#[test]
fn fewer_offsets_than_glyphs_is_truncated_not_rejected() {
    let data = convert(&[
        UInt16(0), // offset [0] -> 0
        UInt16(5), // offset [1] -> 10
        UInt16(10), // offset [2] -> 20
    ]);

    let table = Table::parse(nzu16!(9), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 3);
    assert_eq!(table.glyph_range(GlyphId(1)), Some(10..20));
    assert_eq!(table.glyph_range(GlyphId(2)), None);
}

// More offsets than `maxp.numGlyphs + 1` are ignored.
#[test]
fn extra_offsets_are_ignored() {
    let data = convert(&[
        UInt16(0), // offset [0] -> 0
        UInt16(5), // offset [1] -> 10
        UInt16(10), // offset [2] -> 20
        UInt16(15), // offset [3] -> 30
    ]);

    let table = Table::parse(nzu16!(1), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table.glyph_range(GlyphId(0)), Some(0..10));
    assert_eq!(table.glyph_range(GlyphId(1)), None);
}

#[test]
fn trailing_partial_offset_is_ignored() {
    let data = convert(&[
        UInt16(0), // offset [0] -> 0
        UInt16(5), // offset [1] -> 10
        UInt8(0), // a dangling half offset
    ]);

    let table = Table::parse(nzu16!(3), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table.glyph_range(GlyphId(0)), Some(0..10));
    assert_eq!(table.glyph_range(GlyphId(1)), None);
}

// 'The offsets must be in ascending order.'
#[test]
fn descending_offsets_are_rejected() {
    let data = convert(&[
        UInt16(10), // offset [0] -> 20
        UInt16(5), // offset [1] -> 10
    ]);

    let table = Table::parse(nzu16!(1), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table.glyph_range(GlyphId(0)), None);
}

// An empty range means an empty glyph, which has no outline.
#[test]
fn equal_offsets_are_rejected() {
    let data = convert(&[
        UInt32(24), // offset [0]
        UInt32(24), // offset [1]
    ]);

    let table = Table::parse(nzu16!(1), IndexToLocationFormat::Long, &data).unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table.glyph_range(GlyphId(0)), None);
}

// A well-formed 65535-glyph font has 65536 offsets. Indexing them requires more than a u16.
#[test]
fn max_glyphs_short_format_parses_all_offsets() {
    let data = short_offsets(MAX_OFFSETS);
    assert_eq!(data.len(), 131_072);

    let table = Table::parse(nzu16!(MAX_GLYPHS), IndexToLocationFormat::Short, &data).unwrap();
    assert_eq!(table.len(), 65536);
}

#[test]
fn max_glyphs_long_format_parses_all_offsets() {
    let data = long_offsets(MAX_OFFSETS);
    assert_eq!(data.len(), 262_144);

    let table = Table::parse(nzu16!(MAX_GLYPHS), IndexToLocationFormat::Long, &data).unwrap();
    assert_eq!(table.len(), 65536);
}

// Both ends of the index range: glyph 65534 is the last one with a range,
// and glyph 65535 is past the end because its range would need offset [65536].
#[test]
fn max_glyphs_short_format_boundary_glyphs() {
    let data = short_offsets(MAX_OFFSETS);
    let table = Table::parse(nzu16!(MAX_GLYPHS), IndexToLocationFormat::Short, &data).unwrap();

    assert_eq!(table.glyph_range(GlyphId(0)), Some(0..2));
    assert_eq!(table.glyph_range(GlyphId(65533)), Some(131_066..131_068));
    assert_eq!(table.glyph_range(GlyphId(65534)), Some(131_068..131_070));
    assert_eq!(table.glyph_range(GlyphId(65535)), None);
}

#[test]
fn max_glyphs_long_format_boundary_glyphs() {
    let data = long_offsets(MAX_OFFSETS);
    let table = Table::parse(nzu16!(MAX_GLYPHS), IndexToLocationFormat::Long, &data).unwrap();

    assert_eq!(table.glyph_range(GlyphId(0)), Some(0..4));
    assert_eq!(table.glyph_range(GlyphId(65534)), Some(262_136..262_140));
    assert_eq!(table.glyph_range(GlyphId(65535)), None);
}

// The same font body, but with the spec-mandated last offset missing.
// Glyph 65534 then has no range, while everything below it still works.
#[test]
fn max_glyphs_with_a_missing_last_offset() {
    let data = short_offsets(MAX_OFFSETS - 1);
    let table = Table::parse(nzu16!(MAX_GLYPHS), IndexToLocationFormat::Short, &data).unwrap();

    assert_eq!(table.len(), 65535);
    assert_eq!(table.glyph_range(GlyphId(65533)), Some(131_066..131_068));
    assert_eq!(table.glyph_range(GlyphId(65534)), None);
    assert_eq!(table.glyph_range(GlyphId(65535)), None);
}
