use crate::text_utils::RangeError;

use super::RangeExt;

type ByteRange = std::ops::Range<usize>;
type BytePosition = usize;

const T: &str = ""; // Byte range & position do not need text information

const fn r(start: BytePosition, end: BytePosition) -> ByteRange {
    start..end
}

// Basic happy path tests

#[test]
fn basic_split_at() {
    let (left, right) = r(0, 10).split_at(T, 5).expect("valid range");
    assert_eq!(left, r(0, 5));
    assert_eq!(right, r(5, 10));
}

#[test]
fn basic_split_off_left() {
    let left = r(0, 10).split_off_left(T, 3).expect("valid range");
    assert_eq!(left, r(0, 3));
}

#[test]
fn basic_split_off_right() {
    let right = r(0, 10).split_off_right(T, 7).expect("valid range");
    assert_eq!(right, r(7, 10));
}

#[test]
fn basic_shrink() {
    let shrunk = r(0, 10).shrink(2, 3).expect("valid range");
    assert_eq!(shrunk, r(2, 7));
}

#[test]
fn basic_sub() {
    let sub_range = r(0, 10).sub(T, 2, 8).expect("valid range");
    assert_eq!(sub_range, r(2, 8));
}

// Edge case tests

#[test]
fn split_at_boundaries() {
    let (left, right) = r(5, 15).split_at(T, 0).expect("valid range");
    assert_eq!(left, r(5, 5));
    assert_eq!(right, r(5, 15));

    let (left, right) = r(5, 15).split_at(T, 10).expect("valid range");
    assert_eq!(left, r(5, 15));
    assert_eq!(right, r(15, 15));
}

#[test]
fn sub_empty_range() {
    let sub_range = r(5, 15).sub(T, 3, 3).expect("valid range");
    assert_eq!(sub_range, r(8, 8));
}

// Delimiter cases live in the `sub_delimited` / `sub_delimited_tri`
// doctests in `mod.rs`; they are not duplicated here. The delimiter
// error paths are tested below.

// Boundary and error path tests

#[test]
fn split_off_boundaries() {
    assert_eq!(r(5, 15).split_off_left(T, 0).expect("valid range"), r(5, 5));
    assert_eq!(
        r(5, 15).split_off_right(T, 10).expect("valid range"),
        r(15, 15)
    );
}

#[test]
fn out_of_range_positions_return_position_out_of_range() {
    assert_eq!(
        r(0, 10).split_at(T, 11).unwrap_err(),
        RangeError::PositionOutOfRange
    );
    assert_eq!(
        r(0, 10).sub(T, 3, 11).unwrap_err(),
        RangeError::PositionOutOfRange
    );
}

#[test]
fn reversed_sub_positions_return_start_after_end() {
    assert_eq!(
        r(0, 10).sub(T, 7, 3).unwrap_err(),
        RangeError::StartAfterEnd
    );
}

#[test]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    assert_eq!(
        r(0, 9).sub_delimited("one—two", '—').unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' }
    );
}

#[test]
fn mismatched_text_length_returns_text_range_mismatch() {
    assert_eq!(
        r(0, 7).sub_delimited("short", '/').unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 5,
            range_len: 7
        }
    );
}
