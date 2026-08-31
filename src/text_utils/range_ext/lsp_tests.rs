use crate::testing::{line_position, line_range};
use crate::text_utils::RangeError;

use super::RangeExt;

const T: &str = ""; // LSP range & position do not need text information
const LF: char = '\n';
const D1: char = '/';
const D2: char = '@';

// Basic happy path tests

#[test]
fn basic_split_at() {
    let (left, right) = line_range(line_position(0, 0), line_position(0, 10))
        .split_at(T, line_position(0, 5))
        .expect("valid range");
    assert_eq!(left, line_range(line_position(0, 0), line_position(0, 5)));
    assert_eq!(right, line_range(line_position(0, 5), line_position(0, 10)));
}

#[test]
fn basic_split_off_left() {
    let left = line_range(line_position(0, 0), line_position(0, 10))
        .split_off_left(T, line_position(0, 3))
        .expect("valid range");
    assert_eq!(left, line_range(line_position(0, 0), line_position(0, 3)));
}

#[test]
fn basic_split_off_right() {
    let right = line_range(line_position(0, 0), line_position(0, 10))
        .split_off_right(T, line_position(0, 7))
        .expect("valid range");
    assert_eq!(right, line_range(line_position(0, 7), line_position(0, 10)));
}

#[test]
fn basic_shrink() {
    let shrunk = line_range(line_position(0, 0), line_position(0, 5))
        .shrink(1, 2)
        .expect("valid range");
    assert_eq!(shrunk, line_range(line_position(0, 1), line_position(0, 3)));
}

#[test]
fn basic_sub() {
    let sub_range = line_range(line_position(0, 0), line_position(0, 10))
        .sub(T, line_position(0, 2), line_position(0, 8))
        .expect("valid range");
    assert_eq!(
        sub_range,
        line_range(line_position(0, 2), line_position(0, 8))
    );
}

#[test]
fn basic_sub_delimited() {
    let (left, right) = line_range(line_position(0, 0), line_position(0, 7))
        .sub_delimited("one/two", D1)
        .expect("valid range");
    assert_eq!(
        left,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(
        right,
        Some(line_range(line_position(0, 4), line_position(0, 7)))
    );
}

#[test]
fn basic_sub_delimited_tri() {
    let (first, second, third) = line_range(line_position(0, 0), line_position(0, 13))
        .sub_delimited_tri("one/two@three", D1, D2)
        .expect("valid range");
    assert_eq!(
        first,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(
        second,
        Some(line_range(line_position(0, 4), line_position(0, 7)))
    );
    assert_eq!(
        third,
        Some(line_range(line_position(0, 8), line_position(0, 13)))
    );
}

// Edge case tests

#[test]
fn split_at_boundaries() {
    let (left, right) = line_range(line_position(1, 5), line_position(1, 15))
        .split_at(T, line_position(0, 0))
        .expect("valid range");
    assert_eq!(left, line_range(line_position(1, 5), line_position(1, 5)));
    assert_eq!(right, line_range(line_position(1, 5), line_position(1, 15)));

    let (left, right) = line_range(line_position(1, 5), line_position(1, 15))
        .split_at(T, line_position(0, 10))
        .expect("valid range");
    assert_eq!(left, line_range(line_position(1, 5), line_position(1, 15)));
    assert_eq!(
        right,
        line_range(line_position(1, 15), line_position(1, 15))
    );
}

#[test]
fn split_at_multiline() {
    let (left, right) = line_range(line_position(0, 0), line_position(2, 5))
        .split_at(T, line_position(1, 3))
        .expect("valid range");
    assert_eq!(left, line_range(line_position(0, 0), line_position(1, 3)));
    assert_eq!(right, line_range(line_position(1, 3), line_position(2, 5)));
}

#[test]
fn sub_empty_range() {
    let sub_range = line_range(line_position(1, 5), line_position(1, 15))
        .sub(T, line_position(0, 3), line_position(0, 3))
        .expect("valid range");
    assert_eq!(
        sub_range,
        line_range(line_position(1, 8), line_position(1, 8))
    );
}

#[test]
fn sub_multiline() {
    let sub_range = line_range(line_position(0, 0), line_position(2, 10))
        .sub(T, line_position(0, 5), line_position(1, 3))
        .expect("valid range");
    assert_eq!(
        sub_range,
        line_range(line_position(0, 5), line_position(1, 3))
    );
}

#[test]
fn sub_delimited_delimiter_at_start() {
    let (left, right) = line_range(line_position(0, 0), line_position(0, 4))
        .sub_delimited("/abc", D1)
        .expect("valid range");
    assert_eq!(left, None);
    assert_eq!(
        right,
        Some(line_range(line_position(0, 1), line_position(0, 4)))
    );
}

#[test]
fn sub_delimited_delimiter_at_end() {
    let (left, right) = line_range(line_position(0, 0), line_position(0, 4))
        .sub_delimited("abc/", D1)
        .expect("valid range");
    assert_eq!(
        left,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(right, None);
}

#[test]
fn sub_delimited_no_delimiter() {
    let (left, right) = line_range(line_position(0, 0), line_position(0, 3))
        .sub_delimited("abc", D1)
        .expect("valid range");
    assert_eq!(
        left,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(right, None);
}

#[test]
fn sub_delimited_empty_text() {
    let (left, right) = line_range(line_position(0, 0), line_position(0, 0))
        .sub_delimited(T, D1)
        .expect("valid range");
    assert_eq!(left, None);
    assert_eq!(right, None);
}

#[test]
fn sub_delimited_multiline() {
    let (left, right) = line_range(line_position(0, 0), line_position(1, 3))
        .sub_delimited("abc\ndef", LF)
        .expect("valid range");
    assert_eq!(
        left,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(
        right,
        Some(line_range(line_position(1, 0), line_position(1, 3)))
    );
}

#[test]
fn sub_delimited_tri_partial() {
    let (first, second, third) = line_range(line_position(0, 0), line_position(0, 7))
        .sub_delimited_tri("one/two", D1, D2)
        .expect("valid range");
    assert_eq!(
        first,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(
        second,
        Some(line_range(line_position(0, 4), line_position(0, 7)))
    );
    assert_eq!(third, None);
}

#[test]
fn sub_delimited_tri_no_delimiters() {
    let (first, second, third) = line_range(line_position(0, 0), line_position(0, 3))
        .sub_delimited_tri("abc", D1, D2)
        .expect("valid range");
    assert_eq!(
        first,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(second, None);
    assert_eq!(third, None);
}

#[test]
fn sub_delimited_tri_multiline() {
    let (first, second, third) = line_range(line_position(0, 0), line_position(2, 3))
        .sub_delimited_tri("one\ntwo\n@@@", LF, D2)
        .expect("valid range");
    assert_eq!(
        first,
        Some(line_range(line_position(0, 0), line_position(0, 3)))
    );
    assert_eq!(
        second,
        Some(line_range(line_position(1, 0), line_position(2, 0)))
    );
    assert_eq!(
        third,
        Some(line_range(line_position(2, 1), line_position(2, 3)))
    );
}

// Boundary and error path tests

#[test]
fn split_off_boundaries() {
    let range = line_range(line_position(1, 5), line_position(1, 15));
    assert_eq!(
        range
            .split_off_left(T, line_position(0, 0))
            .expect("valid range"),
        line_range(line_position(1, 5), line_position(1, 5))
    );
    assert_eq!(
        range
            .split_off_right(T, line_position(0, 10))
            .expect("valid range"),
        line_range(line_position(1, 15), line_position(1, 15))
    );
}

#[test]
fn out_of_range_positions_return_position_out_of_range() {
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 10))
            .split_at(T, line_position(0, 11))
            .unwrap_err(),
        RangeError::PositionOutOfRange
    );
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 10))
            .sub(T, line_position(0, 3), line_position(0, 11))
            .unwrap_err(),
        RangeError::PositionOutOfRange
    );
}

#[test]
fn reversed_sub_positions_return_start_after_end() {
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 10))
            .sub(T, line_position(0, 7), line_position(0, 3))
            .unwrap_err(),
        RangeError::StartAfterEnd
    );
}

#[test]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 7))
            .sub_delimited("one—two", '—')
            .unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' }
    );
}

#[test]
fn shrink_requires_a_single_line_range() {
    let multiline = line_range(line_position(0, 0), line_position(1, 0)); // spans "a\nb"
    assert_eq!(
        multiline.shrink(1, 1).unwrap_err(),
        RangeError::NotSingleLine
    );
}
