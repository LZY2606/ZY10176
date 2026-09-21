use ttf_parser::head::Table;
use ttf_parser::{Face, RawFaceTables};
use crate::{convert, Unit::*};

// A minimal, valid `head` table (54 bytes) with a configurable `macStyle` field.
fn head_data(mac_style: u16) -> Vec<u8> {
    convert(&[
        UInt32(0x00010000),   // version
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
        UInt16(mac_style),     // mac style
        UInt16(0),              // lowest PPEM
        Int16(0),                // font direction hint
        UInt16(0),                // index to loc format
        Int16(0),                  // glyph data format
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

// A minimal, valid version-0 `OS/2` table (78 bytes) with a configurable `fsSelection`.
fn os2_data(fs_selection: u16) -> Vec<u8> {
    convert(&[
        UInt16(0),         // version
        Int16(0),           // x avg char width
        UInt16(400),         // weight class
        UInt16(5),            // width class
        UInt16(0),              // fs type
        Raw(&[0; 8]),             // subscript metrics
        Raw(&[0; 8]),              // superscript metrics
        Int16(0),                  // strikeout size
        Int16(0),                   // strikeout position
        Int16(0),                    // family class
        Raw(&[0; 10]),                 // panose
        Raw(&[0; 16]),                  // unicode ranges
        Raw(&[0; 4]),                    // vendor ID
        UInt16(fs_selection),             // fs selection
        UInt16(0),                         // first char index
        UInt16(0),                          // last char index
        Int16(0),                            // typo ascender
        Int16(0),                             // typo descender
        Int16(0),                              // typo line gap
        UInt16(0),                              // win ascent
        UInt16(0),                               // win descent
    ])
}

// A minimal, valid `post` table (32 bytes, version 1.0) with a configurable italic angle.
fn post_data(italic_angle: f32) -> Vec<u8> {
    convert(&[
        UInt32(0x00010000), // version
        Fixed(italic_angle), // italic angle
        Int16(0),             // underline position
        Int16(0),              // underline thickness
        UInt32(0),               // is fixed pitch
        Raw(&[0; 16]),             // memory usage fields
    ])
}

#[test]
fn mac_style_italic_bit_clear() {
    let table = Table::parse(&head_data(0)).unwrap();
    assert_eq!(table.is_italic, false);
}

#[test]
fn mac_style_italic_bit_set() {
    let table = Table::parse(&head_data(1 << 1)).unwrap();
    assert_eq!(table.is_italic, true);
}

#[test]
fn mac_style_bold_bit_does_not_imply_italic() {
    let table = Table::parse(&head_data(1 << 0)).unwrap();
    assert_eq!(table.is_italic, false);
}

#[test]
fn mac_style_bold_and_italic_bits_both_set() {
    let table = Table::parse(&head_data((1 << 0) | (1 << 1))).unwrap();
    assert_eq!(table.is_italic, true);
}

// Regression test for https://github.com/harfbuzz/ttf-parser/issues/202:
// an upright font with a nonzero `post.italicAngle` and no italic flag in
// either `OS/2.fsSelection` or `head.macStyle` must not be reported as italic.
#[test]
fn is_italic_ignores_nonzero_post_italic_angle_when_no_style_flag_is_set() {
    let head = head_data(0);
    let hhea = hhea_data();
    let maxp = maxp_data();
    let os2 = os2_data(0);
    let post = post_data(-12.0);

    let face = Face::from_raw_tables(RawFaceTables {
        head: &head,
        hhea: &hhea,
        maxp: &maxp,
        os2: Some(&os2),
        post: Some(&post),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(face.is_italic(), false);
}

#[test]
fn is_italic_true_when_head_mac_style_italic_bit_is_set() {
    let head = head_data(1 << 1);
    let hhea = hhea_data();
    let maxp = maxp_data();
    let os2 = os2_data(0);

    let face = Face::from_raw_tables(RawFaceTables {
        head: &head,
        hhea: &hhea,
        maxp: &maxp,
        os2: Some(&os2),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(face.is_italic(), true);
}

#[test]
fn is_italic_true_when_os2_fs_selection_italic_bit_is_set() {
    let head = head_data(0);
    let hhea = hhea_data();
    let maxp = maxp_data();
    let os2 = os2_data(1 << 0); // ITALIC bit

    let face = Face::from_raw_tables(RawFaceTables {
        head: &head,
        hhea: &hhea,
        maxp: &maxp,
        os2: Some(&os2),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(face.is_italic(), true);
}
