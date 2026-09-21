use ttf_parser::os2::panose::{FamilyType, Panose};
use ttf_parser::FromData as _;

// Digit order for Latin Text (family 2):
// family, serif, weight, proportion, contrast, stroke, arm, letterform, midline, x-height.
const LATIN_TEXT_UPRIGHT: [u8; 10] = [2, 0, 0, 0, 0, 0, 0, 2, 0, 0];
const LATIN_TEXT_OBLIQUE: [u8; 10] = [2, 0, 0, 0, 0, 0, 0, 11, 0, 0];
const LATIN_TEXT_BOLD: [u8; 10] = [2, 0, 8, 0, 0, 0, 0, 0, 0, 0];
const LATIN_TEXT_MONOSPACED: [u8; 10] = [2, 0, 0, 9, 0, 0, 0, 0, 0, 0];

// Digit order for Latin Handwritten (family 3):
// family, tool, weight, spacing, aspect, contrast, topology, form, finials, x-ascent.
const HANDWRITTEN_OBLIQUE: [u8; 10] = [3, 0, 0, 0, 0, 0, 0, 7, 0, 0];
// Form values 10 and above are the "exaggerated" upright forms, not oblique. Unlike
// Latin Text, the oblique range here does not run to the end.
const HANDWRITTEN_EXAGGERATED: [u8; 10] = [3, 0, 0, 0, 0, 0, 0, 11, 0, 0];
const HANDWRITTEN_MONOSPACED: [u8; 10] = [3, 0, 0, 3, 0, 0, 0, 0, 0, 0];

const ANY_FIT: [u8; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
const NO_FIT: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

#[test]
fn family_type_is_read_from_the_first_digit() {
    assert_eq!(Panose(ANY_FIT).family_type(), FamilyType::Any);
    assert_eq!(Panose(NO_FIT).family_type(), FamilyType::NoFit);
    assert_eq!(Panose(LATIN_TEXT_UPRIGHT).family_type(), FamilyType::LatinText);
    assert_eq!(
        Panose(HANDWRITTEN_OBLIQUE).family_type(),
        FamilyType::LatinHandwritten
    );
}

#[test]
fn family_type_reports_an_undefined_value_verbatim() {
    assert_eq!(Panose([200; 10]).family_type(), FamilyType::Other(200));
}

#[test]
fn parse_accepts_any_ten_bytes_and_preserves_them() {
    let panose = Panose::parse(&NO_FIT).unwrap();
    assert_eq!(panose.0, NO_FIT);

    // Out-of-range digits are kept rather than rejected, so one junk digit does not
    // discard the classification.
    let junk = [2, 255, 255, 255, 255, 255, 255, 255, 255, 255];
    assert_eq!(Panose::parse(&junk).unwrap().0, junk);
}

#[test]
fn parse_rejects_fewer_than_ten_bytes() {
    assert_eq!(Panose::parse(&[2, 0, 0]), None);
}

#[test]
fn is_italic_is_true_for_latin_text_oblique_letterforms() {
    assert_eq!(Panose(LATIN_TEXT_OBLIQUE).is_italic(), true);
    assert_eq!(Panose(LATIN_TEXT_UPRIGHT).is_italic(), false);
}

#[test]
fn is_italic_is_true_only_within_the_handwritten_oblique_range() {
    assert_eq!(Panose(HANDWRITTEN_OBLIQUE).is_italic(), true);
    assert_eq!(Panose(HANDWRITTEN_EXAGGERATED).is_italic(), false);
}

#[test]
fn is_italic_is_false_when_the_family_does_not_classify_letterform() {
    assert_eq!(Panose(ANY_FIT).is_italic(), false);
    assert_eq!(Panose(NO_FIT).is_italic(), false);
}

#[test]
fn is_bold_reads_the_weight_digit() {
    assert_eq!(Panose(LATIN_TEXT_BOLD).is_bold(), true);
    assert_eq!(Panose(LATIN_TEXT_UPRIGHT).is_bold(), false);
}

#[test]
fn is_bold_is_false_for_any_and_no_fit_whose_digits_are_meaningless() {
    // Both fixtures carry an 8 in some position; neither is a weight.
    assert_eq!(Panose(ANY_FIT).is_bold(), false);
    assert_eq!(Panose(NO_FIT).is_bold(), false);
}

#[test]
fn is_monospaced_uses_the_value_for_the_family() {
    // Latin Text encodes monospaced as 9, Latin Handwritten as 3, at the same position.
    assert_eq!(Panose(LATIN_TEXT_MONOSPACED).is_monospaced(), true);
    assert_eq!(Panose(HANDWRITTEN_MONOSPACED).is_monospaced(), true);

    // The other family's value at that position must not be accepted.
    let text_with_handwritten_value = [2, 0, 0, 3, 0, 0, 0, 0, 0, 0];
    assert_eq!(Panose(text_with_handwritten_value).is_monospaced(), false);

    let handwritten_with_text_value = [3, 0, 0, 9, 0, 0, 0, 0, 0, 0];
    assert_eq!(Panose(handwritten_with_text_value).is_monospaced(), false);
}
