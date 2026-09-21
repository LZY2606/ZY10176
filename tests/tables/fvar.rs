use ttf_parser::fvar::Table;
use ttf_parser::{Face, RawFaceTables, Tag};
use crate::{convert, Unit::*};

// The `fvar` header this parser reads is 10 bytes (version, axesArrayOffset, reserved,
// axisCount); the full spec header is 16, so the axes array starts at 16.
const AXES_ARRAY_OFFSET: u16 = 16;

// Builds an `fvar` table with `count` axes, tagged `a000`, `a001`, ... in order.
// Every axis spans -1.0 ..= 1.0 with a default of 0.0, so `normalized_value` is the identity.
fn fvar_data(count: u16) -> Vec<u8> {
    let mut units = vec![
        UInt32(0x00010000),      // version
        UInt16(AXES_ARRAY_OFFSET), // axesArrayOffset
        UInt16(0),                  // reserved
        UInt16(count),               // axisCount
        UInt16(20),                   // axisSize
        UInt16(0),                     // instanceCount
        UInt16(0),                      // instanceSize
    ];

    for i in 0..count {
        units.push(Raw(axis_tag(i)));
        units.push(Fixed(-1.0)); // minValue
        units.push(Fixed(0.0));  // defaultValue
        units.push(Fixed(1.0));  // maxValue
        units.push(UInt16(0));   // flags
        units.push(UInt16(0));   // axisNameID
    }

    convert(&units)
}

// `Unit::Raw` needs a `&'static [u8]`, so the tags are a table rather than formatted.
// 64 axes is the storage limit, and the tests reach one past it.
fn axis_tag(index: u16) -> &'static [u8] {
    const TAGS: &[u8; 4 * 66] = b"\
        a000a001a002a003a004a005a006a007a008a009a010a011a012a013a014a015\
        a016a017a018a019a020a021a022a023a024a025a026a027a028a029a030a031\
        a032a033a034a035a036a037a038a039a040a041a042a043a044a045a046a047\
        a048a049a050a051a052a053a054a055a056a057a058a059a060a061a062a063\
        a064a065";

    let start = usize::from(index) * 4;
    &TAGS[start..start + 4]
}

fn head_data() -> Vec<u8> {
    convert(&[
        UInt32(0x00010000),  // version
        Fixed(1.0),           // font revision
        UInt32(0),             // checksum adjustment
        UInt32(0x5F0F3CF5),   // magic number
        UInt16(0),             // flags
        UInt16(1000),          // units per EM
        Raw(&[0; 8]),           // created
        Raw(&[0; 8]),           // modified
        Int16(0),               // x min
        Int16(0),               // y min
        Int16(0),               // x max
        Int16(0),               // y max
        UInt16(0),              // mac style
        UInt16(0),              // lowest PPEM
        Int16(0),               // font direction hint
        UInt16(0),              // index to loc format
        Int16(0),               // glyph data format
    ])
}

fn hhea_data() -> Vec<u8> {
    convert(&[
        UInt32(0x00010000), // version
        Int16(0),            // ascender
        Int16(0),            // descender
        Int16(0),            // line gap
        Raw(&[0; 24]),        // metrics unused by `hhea::Table`
        UInt16(0),             // number of h-metrics
    ])
}

fn maxp_data() -> Vec<u8> {
    convert(&[
        Fixed(0.3125), // version 0.5
        UInt16(1),     // number of glyphs
    ])
}

// Sets the *first* axis of a `count`-axis face and reports whether it took effect.
// Returns (result of set_variation, coordinate 0 after the call).
fn set_first_axis(count: u16) -> (Option<()>, i16) {
    let head = head_data();
    let hhea = hhea_data();
    let maxp = maxp_data();
    let fvar = fvar_data(count);

    let mut face = Face::from_raw_tables(RawFaceTables {
        head: &head,
        hhea: &hhea,
        maxp: &maxp,
        fvar: Some(&fvar),
        ..Default::default()
    })
    .unwrap();

    let result = face.set_variation(Tag::from_bytes(b"a000"), 1.0);
    (result, face.variation_coordinates()[0].get())
}

#[test]
fn parses_every_axis_record() {
    let data = fvar_data(3);
    let table = Table::parse(&data).unwrap();

    assert_eq!(table.axes.len(), 3);
    assert_eq!(table.axes.get(0).unwrap().tag, Tag::from_bytes(b"a000"));
    assert_eq!(table.axes.get(2).unwrap().tag, Tag::from_bytes(b"a002"));
    assert_eq!(table.axes.get(1).unwrap().min_value, -1.0);
    assert_eq!(table.axes.get(1).unwrap().def_value, 0.0);
    assert_eq!(table.axes.get(1).unwrap().max_value, 1.0);
}

#[test]
fn zero_axes_is_not_a_variable_font() {
    assert!(Table::parse(&fvar_data(0)).is_none());
}

#[test]
fn set_variation_works_below_the_coordinate_limit() {
    let (result, coord) = set_first_axis(63);
    assert_eq!(result, Some(()));
    assert_eq!(coord, 16384);
}

// Regression test for https://github.com/harfbuzz/ttf-parser/issues/237: the guard in
// `set_variation` used `>=` while the coordinate array holds exactly `MAX_VAR_COORDS`
// entries and every other site clamps inclusively, so a 64-axis face could not be varied
// at all even though the rest of the crate supports it.
#[test]
fn set_variation_works_at_exactly_the_coordinate_limit() {
    let (result, coord) = set_first_axis(64);
    assert_eq!(result, Some(()));
    assert_eq!(coord, 16384);
}

#[test]
fn set_variation_rejects_a_face_with_more_axes_than_the_limit() {
    let (result, coord) = set_first_axis(65);
    assert_eq!(result, None);
    assert_eq!(coord, 0);
}

#[test]
fn set_variation_returns_none_for_an_unknown_axis_on_a_full_face() {
    let head = head_data();
    let hhea = hhea_data();
    let maxp = maxp_data();
    let fvar = fvar_data(64);

    let mut face = Face::from_raw_tables(RawFaceTables {
        head: &head,
        hhea: &hhea,
        maxp: &maxp,
        fvar: Some(&fvar),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(face.set_variation(Tag::from_bytes(b"zzzz"), 1.0), None);
    assert_eq!(face.has_non_default_variation_coordinates(), false);
}
