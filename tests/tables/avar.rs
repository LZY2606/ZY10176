use ttf_parser::avar::Table;
use ttf_parser::NormalizedCoordinate;
use crate::{convert, Unit::*};

// Builds a single-axis `avar` table out of (fromCoordinate, toCoordinate) pairs.
fn single_axis(maps: &[(i16, i16)]) -> Vec<u8> {
    let mut units = vec![
        UInt16(1), // majorVersion
        UInt16(0), // minorVersion
        UInt16(0), // reserved
        UInt16(1), // axisCount

        UInt16(maps.len() as u16), // positionMapCount [axis 0]
    ];

    for (from, to) in maps {
        units.push(Int16(*from)); // fromCoordinate
        units.push(Int16(*to)); // toCoordinate
    }

    convert(&units)
}

// Maps `value` through a single-axis table, returning (result of map_coordinate, coordinate after).
fn map_single(maps: &[(i16, i16)], value: i16) -> (Option<()>, i16) {
    let data = single_axis(maps);
    let table = Table::parse(&data).unwrap();
    let mut coords = [NormalizedCoordinate::from(value)];
    let result = table.map_coordinate(&mut coords, 0);
    (result, coords[0].get())
}

#[test]
fn unsupported_version_is_rejected() {
    let data = convert(&[
        UInt16(2), // majorVersion
        UInt16(0), // minorVersion
        UInt16(0), // reserved
        UInt16(0), // axisCount
    ]);

    assert!(Table::parse(&data).is_none());
}

#[test]
fn no_axes_parses_with_empty_segment_maps() {
    let data = convert(&[
        UInt16(1), // majorVersion
        UInt16(0), // minorVersion
        UInt16(0), // reserved
        UInt16(0), // axisCount
    ]);

    let table = Table::parse(&data).unwrap();
    assert_eq!(table.segment_maps.len(), 0);
    assert!(table.segment_maps.is_empty());
}

#[test]
fn coordinates_length_mismatch_is_rejected() {
    let data = convert(&[
        UInt16(1), // majorVersion
        UInt16(0), // minorVersion
        UInt16(0), // reserved
        UInt16(2), // axisCount

        UInt16(0), // positionMapCount [axis 0]
        UInt16(0), // positionMapCount [axis 1]
    ]);

    let table = Table::parse(&data).unwrap();
    let mut coords = [NormalizedCoordinate::from(1000i16)];
    assert_eq!(table.map_coordinate(&mut coords, 0), None);
    assert_eq!(coords[0].get(), 1000);
}

#[test]
fn empty_segment_map_leaves_coordinate_unchanged() {
    assert_eq!(map_single(&[], 12345), (Some(()), 12345));
}

// `shift_coordinate` on the single-record path: 16384 - (-32768) + 32767 == 81919,
// which does not fit into an i16, so the whole mapping is rejected.
#[test]
fn single_record_shift_out_of_i16_range_is_rejected() {
    assert_eq!(map_single(&[(-32768, 32767)], 16384), (None, 16384));
}

// Same path, but the true result (16384) is representable even though the
// `value - from` intermediate (49152) is not. Must map successfully.
#[test]
fn single_record_shift_with_overflowing_intermediate_is_mapped() {
    assert_eq!(map_single(&[(-32768, -32768)], 16384), (Some(()), 16384));
}

// `shift_coordinate` on the `value <= record_0.from_coordinate` path:
// -16384 - 32767 + (-32768) == -81919, out of i16 range.
#[test]
fn first_record_shift_out_of_i16_range_is_rejected() {
    assert_eq!(
        map_single(&[(32767, -32768), (32767, 32767)], -16384),
        (None, -16384)
    );
}

// `shift_coordinate` on the `value >= curr_from` path: 16384 - 0 + 32767 == 49151,
// out of i16 range.
#[test]
fn current_record_shift_out_of_i16_range_is_rejected() {
    assert_eq!(
        map_single(&[(-16384, -16384), (0, 32767)], 16384),
        (None, 16384)
    );
}

// The interpolation branch: denom == 65535 and
// k == (32767 - -32768) * (16384 - -32768) + 32767 == 3_221_209_087,
// which overflows i32. The mathematically correct answer is -32768 + 49152 == 16384.
#[test]
fn interpolation_with_overflowing_intermediate_is_mapped() {
    assert_eq!(
        map_single(&[(-32768, -32768), (32767, 32767)], 16384),
        (Some(()), 16384)
    );
}

// A value above every `fromCoordinate` clamps onto the last segment and is shifted by it,
// rather than being interpolated.
#[test]
fn value_above_every_segment_uses_the_last_one() {
    assert_eq!(
        map_single(&[(-16384, -16384), (0, 0), (100, 50)], 16384),
        (Some(()), 16334)
    );
}

// An in-i16-range result outside the normalized -16384..16384 range is clamped, not rejected.
#[test]
fn mapped_value_is_clamped_to_the_normalized_range() {
    assert_eq!(map_single(&[(0, 300)], 16384), (Some(()), 16384));
    assert_eq!(map_single(&[(0, -300)], -16384), (Some(()), -16384));
}

// Regression guard: a spec-conforming, strictly increasing 4-entry map must keep
// producing exactly these values. Nothing here overflows, so it pins that the
// arithmetic widening did not change well-formed-font behaviour.
#[test]
fn spec_conforming_map_produces_stable_values() {
    let maps = [(-16384, -16384), (0, 0), (8192, 12000), (16384, 16384)];

    assert_eq!(map_single(&maps, -16384), (Some(()), -16384));
    assert_eq!(map_single(&maps, -8192), (Some(()), -8192));
    assert_eq!(map_single(&maps, -1), (Some(()), -1));
    assert_eq!(map_single(&maps, 0), (Some(()), 0));
    assert_eq!(map_single(&maps, 4096), (Some(()), 6000));
    assert_eq!(map_single(&maps, 8192), (Some(()), 12000));
    assert_eq!(map_single(&maps, 12288), (Some(()), 14192));
    assert_eq!(map_single(&maps, 16384), (Some(()), 16384));
}

// Only the axis at `coordinate_index` is mapped; the others are left alone.
#[test]
fn only_the_requested_axis_is_mapped() {
    let data = convert(&[
        UInt16(1), // majorVersion
        UInt16(0), // minorVersion
        UInt16(0), // reserved
        UInt16(2), // axisCount

        UInt16(1), // positionMapCount [axis 0]
        Int16(0), // fromCoordinate
        Int16(1000), // toCoordinate

        UInt16(1), // positionMapCount [axis 1]
        Int16(0), // fromCoordinate
        Int16(2000), // toCoordinate
    ]);

    let table = Table::parse(&data).unwrap();
    assert_eq!(table.segment_maps.len(), 2);

    let mut coords = [
        NormalizedCoordinate::from(100i16),
        NormalizedCoordinate::from(100i16),
    ];
    assert_eq!(table.map_coordinate(&mut coords, 1), Some(()));
    assert_eq!(coords[0].get(), 100);
    assert_eq!(coords[1].get(), 2100);
}

// A rejected mapping must not have written a partial result into the coordinate.
#[test]
fn rejected_mapping_leaves_all_coordinates_untouched() {
    let data = convert(&[
        UInt16(1), // majorVersion
        UInt16(0), // minorVersion
        UInt16(0), // reserved
        UInt16(2), // axisCount

        UInt16(1), // positionMapCount [axis 0]
        Int16(0), // fromCoordinate
        Int16(1000), // toCoordinate

        UInt16(1), // positionMapCount [axis 1]
        Int16(-32768), // fromCoordinate
        Int16(32767), // toCoordinate
    ]);

    let table = Table::parse(&data).unwrap();
    let mut coords = [
        NormalizedCoordinate::from(16384i16),
        NormalizedCoordinate::from(16384i16),
    ];
    assert_eq!(table.map_coordinate(&mut coords, 1), None);
    assert_eq!(coords[0].get(), 16384);
    assert_eq!(coords[1].get(), 16384);
}
