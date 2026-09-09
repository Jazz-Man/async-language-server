// tree_sitter::Point/Range — same Ts... naming as tree_sitter.rs, the Ts... half of the RangeExt family.
use tree_sitter::{Point as TsPosition, Range as TsRange};

use crate::text_utils::RangeError;

use super::RangeExt;

const LF: char = '\n';
const D1: char = '/';
const D2: char = '@';

const fn r(
    start_byte: usize,
    start_position: TsPosition,
    end_byte: usize,
    end_position: TsPosition,
) -> TsRange {
    TsRange {
        start_byte,
        start_point: start_position,
        end_byte,
        end_point: end_position,
    }
}

const fn p(line: usize, column: usize) -> TsPosition {
    TsPosition { row: line, column }
}

// Basic happy path tests

#[test]
fn basic_split_at() {
    let text = "hello";
    let (left, right) = r(0, p(0, 0), 5, p(0, 5))
        .split_at(text, p(0, 2))
        .expect("valid range");
    assert_eq!(left, r(0, p(0, 0), 2, p(0, 2)));
    assert_eq!(right, r(2, p(0, 2), 5, p(0, 5)));
}

#[test]
fn basic_split_off_left() {
    let text = "hello";
    let left = r(0, p(0, 0), 5, p(0, 5))
        .split_off_left(text, p(0, 3))
        .expect("valid range");
    assert_eq!(left, r(0, p(0, 0), 3, p(0, 3)));
}

#[test]
fn basic_split_off_right() {
    let text = "hello";
    let right = r(0, p(0, 0), 5, p(0, 5))
        .split_off_right(text, p(0, 2))
        .expect("valid range");
    assert_eq!(right, r(2, p(0, 2), 5, p(0, 5)));
}

#[test]
fn basic_shrink() {
    let shrunk = r(0, p(0, 0), 5, p(0, 5)).shrink(1, 2).expect("valid range");
    assert_eq!(shrunk, r(1, p(0, 1), 3, p(0, 3)));
}

#[test]
fn basic_sub() {
    let text = "hello";
    let sub_range = r(0, p(0, 0), 5, p(0, 5))
        .sub(text, p(0, 1), p(0, 4))
        .expect("valid range");
    assert_eq!(sub_range, r(1, p(0, 1), 4, p(0, 4)));
}

#[test]
fn basic_sub_delimited() {
    let text = "one/two";
    let (left, right) = r(0, p(0, 0), 7, p(0, 7))
        .sub_delimited(text, D1)
        .expect("valid range");
    assert_eq!(left, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(right, Some(r(4, p(0, 4), 7, p(0, 7))));
}

#[test]
fn basic_sub_delimited_tri() {
    let text = "one/two@three";
    let (first, second, third) = r(0, p(0, 0), text.len(), p(0, text.len()))
        .sub_delimited_tri(text, D1, D2)
        .expect("valid range");
    assert_eq!(first, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(second, Some(r(4, p(0, 4), 7, p(0, 7))));
    assert_eq!(third, Some(r(8, p(0, 8), 13, p(0, 13))));
}

// Edge case tests

#[test]
fn split_at_boundaries() {
    let text = "hello";

    let (left, right) = r(0, p(0, 0), 5, p(0, 5))
        .split_at(text, p(0, 0))
        .expect("valid range");
    assert_eq!(left, r(0, p(0, 0), 0, p(0, 0)));
    assert_eq!(right, r(0, p(0, 0), 5, p(0, 5)));

    let (left, right) = r(0, p(0, 0), 5, p(0, 5))
        .split_at(text, p(0, 5))
        .expect("valid range");
    assert_eq!(left, r(0, p(0, 0), 5, p(0, 5)));
    assert_eq!(right, r(5, p(0, 5), 5, p(0, 5)));
}

#[test]
fn split_at_multiline() {
    let text = "one\ntwo";
    let (left, right) = r(0, p(0, 0), 7, p(1, 3))
        .split_at(text, p(1, 1))
        .expect("valid range");
    assert_eq!(left, r(0, p(0, 0), 5, p(1, 1)));
    assert_eq!(right, r(5, p(1, 1), 7, p(1, 3)));
}

#[test]
fn sub_empty_range() {
    let text = "hello";
    let sub_range = r(10, p(1, 5), 15, p(1, 10))
        .sub(text, p(0, 2), p(0, 2))
        .expect("valid range");
    assert_eq!(sub_range, r(12, p(1, 7), 12, p(1, 7)));
}

#[test]
fn sub_multiline() {
    let text = "one\ntwo\nthree";
    let sub_range = r(0, p(0, 0), text.len(), p(2, 5))
        .sub(text, p(0, 2), p(1, 1))
        .expect("valid range");
    assert_eq!(sub_range, r(2, p(0, 2), 5, p(1, 1)));
}

#[test]
fn sub_delimited_delimiter_at_start() {
    let text = "/abc";
    let (left, right) = r(0, p(0, 0), 4, p(0, 4))
        .sub_delimited(text, D1)
        .expect("valid range");
    assert_eq!(left, None);
    assert_eq!(right, Some(r(1, p(0, 1), 4, p(0, 4))));
}

#[test]
fn sub_delimited_delimiter_at_end() {
    let text = "abc/";
    let (left, right) = r(0, p(0, 0), 4, p(0, 4))
        .sub_delimited(text, D1)
        .expect("valid range");
    assert_eq!(left, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(right, None);
}

#[test]
fn sub_delimited_no_delimiter() {
    let text = "abc";
    let (left, right) = r(0, p(0, 0), 3, p(0, 3))
        .sub_delimited(text, D1)
        .expect("valid range");
    assert_eq!(left, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(right, None);
}

#[test]
fn sub_delimited_empty_text() {
    let text = "";
    let (left, right) = r(0, p(0, 0), 0, p(0, 0))
        .sub_delimited(text, D1)
        .expect("valid range");
    assert_eq!(left, None);
    assert_eq!(right, None);
}

#[test]
fn sub_delimited_multiline() {
    let text = "abc\ndef";
    let (left, right) = r(0, p(0, 0), 7, p(1, 3))
        .sub_delimited(text, LF)
        .expect("valid range");
    assert_eq!(left, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(right, Some(r(4, p(1, 0), 7, p(1, 3))));
}

#[test]
fn sub_delimited_tri_partial() {
    let text = "one/two";
    let (first, second, third) = r(0, p(0, 0), 7, p(0, 7))
        .sub_delimited_tri(text, D1, D2)
        .expect("valid range");
    assert_eq!(first, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(second, Some(r(4, p(0, 4), 7, p(0, 7))));
    assert_eq!(third, None);
}

#[test]
fn sub_delimited_tri_no_delimiters() {
    let text = "abc";
    let (first, second, third) = r(0, p(0, 0), 3, p(0, 3))
        .sub_delimited_tri(text, D1, D2)
        .expect("valid range");
    assert_eq!(first, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(second, None);
    assert_eq!(third, None);
}

#[test]
fn sub_delimited_tri_multiline() {
    let text = "one\ntwo\n@@@";
    let (first, second, third) = r(0, p(0, 0), text.len(), p(2, 3))
        .sub_delimited_tri(text, LF, D2)
        .expect("valid range");
    assert_eq!(first, Some(r(0, p(0, 0), 3, p(0, 3))));
    assert_eq!(second, Some(r(4, p(1, 0), 8, p(2, 0))));
    assert_eq!(third, Some(r(9, p(2, 1), 11, p(2, 3))));
}

// Tree-sitter specific multiline tests

#[test]
fn split_at_newline_boundary() {
    let text = "line1\nline2";
    let (left, right) = r(0, p(0, 0), text.len(), p(1, 5))
        .split_at(text, p(1, 0))
        .expect("valid range");
    assert_eq!(left, r(0, p(0, 0), 6, p(1, 0)));
    assert_eq!(right, r(6, p(1, 0), 11, p(1, 5)));
}

#[test]
fn sub_across_multiple_lines() {
    let text = "line1\nline2\nline3";
    let sub_range = r(0, p(0, 0), text.len(), p(2, 5))
        .sub(text, p(0, 3), p(2, 2))
        .expect("valid range");
    assert_eq!(sub_range, r(3, p(0, 3), 14, p(2, 2)));
}

#[test]
fn sub_delimited_complex_multiline() {
    let text = "start\nfirst/second\nend";
    let (left, right) = r(0, p(0, 0), text.len(), p(2, 3))
        .sub_delimited(text, D1)
        .expect("valid range");
    assert_eq!(left, Some(r(0, p(0, 0), 11, p(1, 5))));
    assert_eq!(right, Some(r(12, p(1, 6), 22, p(2, 3))));
}

// Boundary and error path tests

#[test]
fn split_off_boundaries() {
    let text = "hello";
    let range = r(0, p(0, 0), 5, p(0, 5));
    assert_eq!(
        range.split_off_left(text, p(0, 0)).expect("valid range"),
        r(0, p(0, 0), 0, p(0, 0)),
    );
    assert_eq!(
        range.split_off_right(text, p(0, 5)).expect("valid range"),
        r(5, p(0, 5), 5, p(0, 5)),
    );
}

#[test]
fn reversed_sub_positions_return_start_after_end() {
    let text = "hello";
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .sub(text, p(0, 3), p(0, 1))
            .unwrap_err(),
        RangeError::StartAfterEnd,
    );
}

#[test]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    let text = "one—two";
    assert_eq!(
        r(0, p(0, 0), 9, p(0, 9))
            .sub_delimited(text, '—')
            .unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' },
    );
}

#[test]
fn mismatched_text_length_returns_text_range_mismatch() {
    let text = "short";
    assert_eq!(
        r(0, p(0, 0), 7, p(0, 7))
            .sub_delimited(text, D1)
            .unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 5,
            range_len: 7
        },
    );
}

#[test]
fn shrink_requires_a_single_line_range() {
    let multiline = r(0, p(0, 0), 2, p(1, 0)); // spans "a\nb"
    assert_eq!(
        multiline.shrink(1, 1).unwrap_err(),
        RangeError::NotSingleLine,
    );
}

#[test]
fn split_at_beyond_the_text_returns_position_out_of_range() {
    let text = "hello";
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .split_at(text, p(0, 9))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );
    // A row past the last line is equally out of range.
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .split_at(text, p(2, 0))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[test]
fn sub_positions_beyond_the_text_return_position_out_of_range() {
    let text = "hello";
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .sub(text, p(0, 1), p(0, 9))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .sub(text, p(0, 9), p(0, 9))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[test]
fn split_at_mismatched_text_length_returns_text_range_mismatch() {
    let text = "short";
    assert_eq!(
        r(0, p(0, 0), 7, p(0, 7))
            .split_at(text, p(0, 2))
            .unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 5,
            range_len: 7
        },
    );
}

// Relative-position tests on ranges that do not start at zero: points are
// offset by the range start, not taken as absolute text coordinates.

#[test]
fn split_at_validates_text_length_on_nonzero_start_ranges() {
    let text = "one/two";
    let end_byte = 5 + text.len();
    let (left, right) = r(5, p(0, 3), end_byte, p(0, 10))
        .split_at(text, p(0, 2))
        .expect("valid range");
    assert_eq!(left, r(5, p(0, 3), 7, p(0, 5)));
    assert_eq!(right, r(7, p(0, 5), end_byte, p(0, 10)));

    assert_eq!(
        r(5, p(0, 3), end_byte, p(0, 10))
            .split_at("one/tw", p(0, 2))
            .unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 6,
            range_len: 7
        },
    );
}

#[test]
fn split_at_offsets_columns_by_range_start_column() {
    let text = "ab\ncd";
    let (left, right) = r(5, p(1, 3), 10, p(2, 2))
        .split_at(text, p(0, 1))
        .expect("valid range");
    assert_eq!(left, r(5, p(1, 3), 6, p(1, 4)));
    assert_eq!(right, r(6, p(1, 4), 10, p(2, 2)));
}

#[test]
fn sub_offsets_rows_by_range_start() {
    let text = "ab\ncd";
    let sub_range = r(0, p(2, 0), 5, p(3, 2))
        .sub(text, p(1, 0), p(1, 2))
        .expect("valid range");
    assert_eq!(sub_range, r(3, p(3, 0), 5, p(3, 2)));
}

#[test]
fn sub_resolves_positions_in_and_at_the_end_of_text() {
    let text = "ab\ncd\nef";
    let range = r(0, p(0, 0), 8, p(2, 2));

    // Mid-text positions resolve to their byte offsets.
    let sub_range = range.sub(text, p(0, 1), p(1, 1)).expect("valid range");
    assert_eq!(sub_range, r(1, p(0, 1), 4, p(1, 1)));

    // The exact end-of-text position resolves to the range's end byte.
    let sub_range = range.sub(text, p(2, 2), p(2, 2)).expect("valid range");
    assert_eq!(sub_range, r(8, p(2, 2), 8, p(2, 2)));
}

#[test]
fn sub_rejects_end_of_text_position_mismatches() {
    // A position past its row's text is nowhere in the text even though
    // later rows exist.
    let text = "ab\ncd\nef";
    assert_eq!(
        r(0, p(0, 0), 8, p(2, 2))
            .sub(text, p(0, 3), p(1, 0))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );

    // The end-of-text check must match row and column exactly: a position
    // whose column coincides with the end point's, on an empty row, is
    // still nowhere in the text.
    let text = "ab\n\ncd";
    assert_eq!(
        r(0, p(0, 0), 6, p(2, 2))
            .sub(text, p(1, 2), p(2, 0))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );

    // A row past the last line is equally out of range.
    let text = "hello";
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .sub(text, p(0, 1), p(1, 5))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[test]
fn sub_delimited_tri_validates_text_length_on_nonzero_start_ranges() {
    let text = "one/tw";
    assert_eq!(
        r(5, p(0, 3), 12, p(0, 10))
            .sub_delimited_tri(text, D1, D2)
            .unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 6,
            range_len: 7
        },
    );
}

#[test]
fn sub_delimited_tri_slices_remainder_from_nonzero_start() {
    let text = "a/b@c";
    let (first, second, third) = r(5, p(1, 3), 10, p(1, 8))
        .sub_delimited_tri(text, D1, D2)
        .expect("valid range");
    assert_eq!(first, Some(r(5, p(1, 3), 6, p(1, 4))));
    assert_eq!(second, Some(r(7, p(1, 5), 8, p(1, 6))));
    assert_eq!(third, Some(r(9, p(1, 7), 10, p(1, 8))));
}
