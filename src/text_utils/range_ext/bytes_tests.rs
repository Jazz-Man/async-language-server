use crate::text_utils::RangeError;

use super::RangeExt;

type ByteRange = std::ops::Range<usize>;
type BytePosition = usize;

const TEXT: &str = ""; // Byte range & position do not need text information

const fn r(start: BytePosition, end: BytePosition) -> ByteRange {
    start..end
}

// Basic happy path tests

#[test]
fn basic_split_at() {
    let (left, right) = r(0, 10).split_at(TEXT, 5).expect("valid range");
    assert_eq!(left, r(0, 5));
    assert_eq!(right, r(5, 10));
}

#[test]
fn basic_split_off_left() {
    let left = r(0, 10).split_off_left(TEXT, 3).expect("valid range");
    assert_eq!(left, r(0, 3));
}

#[test]
fn basic_split_off_right() {
    let right = r(0, 10).split_off_right(TEXT, 7).expect("valid range");
    assert_eq!(right, r(7, 10));
}

#[test]
fn basic_shrink() {
    let shrunk = r(0, 10).shrink(2, 3).expect("valid range");
    assert_eq!(shrunk, r(2, 7));
}

#[test]
fn basic_sub() {
    let sub_range = r(0, 10).sub(TEXT, 2, 8).expect("valid range");
    assert_eq!(sub_range, r(2, 8));
}

// Edge case tests

#[test]
fn split_at_boundaries() {
    let (left, right) = r(5, 15).split_at(TEXT, 0).expect("valid range");
    assert_eq!(left, r(5, 5));
    assert_eq!(right, r(5, 15));

    let (left, right) = r(5, 15).split_at(TEXT, 10).expect("valid range");
    assert_eq!(left, r(5, 15));
    assert_eq!(right, r(15, 15));
}

#[test]
fn sub_empty_range() {
    let sub_range = r(5, 15).sub(TEXT, 3, 3).expect("valid range");
    assert_eq!(sub_range, r(8, 8));
}

// Delimiter cases live in the `sub_delimited` / `sub_delimited_tri`
// doctests in `mod.rs`; they are not duplicated here. The delimiter
// error paths are tested below.

// Boundary and error path tests

#[test]
fn split_off_boundaries() {
    assert_eq!(
        r(5, 15).split_off_left(TEXT, 0).expect("valid range"),
        r(5, 5),
    );
    assert_eq!(
        r(5, 15).split_off_right(TEXT, 10).expect("valid range"),
        r(15, 15),
    );
}

#[test]
fn out_of_range_positions_return_position_out_of_range() {
    assert_eq!(
        r(0, 10).split_at(TEXT, 11).unwrap_err(),
        RangeError::PositionOutOfRange,
    );
    assert_eq!(
        r(0, 10).sub(TEXT, 3, 11).unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[test]
fn reversed_sub_positions_return_start_after_end() {
    assert_eq!(
        r(0, 10).sub(TEXT, 7, 3).unwrap_err(),
        RangeError::StartAfterEnd,
    );
}

#[test]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    assert_eq!(
        r(0, 9).sub_delimited("one—two", '—').unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' },
    );
}

#[test]
fn mismatched_text_length_returns_text_range_mismatch() {
    assert_eq!(
        r(0, 7).sub_delimited("short", '/').unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 5,
            range_len: 7
        },
    );
}

// Relative-bound tests on ranges that do not start at zero: positions are
// counted from the range start, not from the text start.

#[test]
fn split_at_rejects_at_beyond_range_length() {
    assert_eq!(
        r(5, 10).split_at(TEXT, 6).unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[test]
fn sub_bounds_are_relative_to_range_length() {
    // from == to == len: the end-relative boundary stays in range.
    assert_eq!(r(5, 10).sub(TEXT, 5, 5).expect("valid range"), r(10, 10));
    // to == len selects the whole tail.
    assert_eq!(r(5, 10).sub(TEXT, 0, 5).expect("valid range"), r(5, 10));
    // len + 1 is out of range for either bound.
    assert_eq!(
        r(5, 10).sub(TEXT, 6, 6).unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[test]
fn sub_delimited_requires_exact_text_length() {
    assert_eq!(
        r(5, 12).sub_delimited("one/two", '/').expect("valid range"),
        (Some(r(5, 8)), Some(r(9, 12))),
    );
    assert_eq!(
        r(5, 12).sub_delimited("one/tw", '/').unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 6,
            range_len: 7
        },
    );
}

#[test]
fn sub_delimited_tri_requires_exact_text_length() {
    assert_eq!(
        r(5, 12).sub_delimited_tri("one/tw", '/', '@').unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 6,
            range_len: 7
        },
    );
}
