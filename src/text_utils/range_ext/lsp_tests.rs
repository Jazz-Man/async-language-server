use async_lsp::lsp_types::{Position, Range};
use rstest::rstest;

use crate::testing::{line_position, line_range};
use crate::text_utils::RangeError;

use super::RangeExt;

const TEXT: &str = ""; // LSP range & position do not need text information
const LF: char = '\n';
const D1: char = '/';
const D2: char = '@';

/// `split_at` divides the range at a relative position: the position becomes
/// the shared boundary of the two halves, and the degenerate boundaries
/// yield an empty half.
#[rstest]
#[case::shared_boundary(
    line_range(line_position(0, 0), line_position(0, 10)),
    line_position(0, 5),
    line_range(line_position(0, 0), line_position(0, 5)),
    line_range(line_position(0, 5), line_position(0, 10))
)]
#[case::multiline_boundary(
    line_range(line_position(0, 0), line_position(2, 5)),
    line_position(1, 3),
    line_range(line_position(0, 0), line_position(1, 3)),
    line_range(line_position(1, 3), line_position(2, 5))
)]
#[case::at_range_start(
    line_range(line_position(1, 5), line_position(1, 15)),
    line_position(0, 0),
    line_range(line_position(1, 5), line_position(1, 5)),
    line_range(line_position(1, 5), line_position(1, 15))
)]
#[case::at_range_end(
    line_range(line_position(1, 5), line_position(1, 15)),
    line_position(0, 10),
    line_range(line_position(1, 5), line_position(1, 15)),
    line_range(line_position(1, 15), line_position(1, 15))
)]
fn split_at_divides_the_range_at_a_relative_position(
    #[case] range: Range,
    #[case] at: Position,
    #[case] expected_left: Range,
    #[case] expected_right: Range,
) {
    let (left, right) = range.split_at(TEXT, at).expect("valid range");
    assert_eq!(left, expected_left);
    assert_eq!(right, expected_right);
}

/// Mirror rows: `split_off_left` keeps the left of the position,
/// `split_off_right` the right.
#[rstest]
fn split_off_returns_the_kept_side() {
    let left = line_range(line_position(0, 0), line_position(0, 10))
        .split_off_left(TEXT, line_position(0, 3))
        .expect("valid range");
    assert_eq!(left, line_range(line_position(0, 0), line_position(0, 3)));

    let right = line_range(line_position(0, 0), line_position(0, 10))
        .split_off_right(TEXT, line_position(0, 7))
        .expect("valid range");
    assert_eq!(right, line_range(line_position(0, 7), line_position(0, 10)));
}

/// shrink narrows a single-line range by the given character counts on
/// each edge; a multiline range has no single-line edge to shrink from and
/// is rejected.
#[rstest]
#[case::single_line(
    line_range(line_position(0, 0), line_position(0, 5)),
    Ok(line_range(line_position(0, 1), line_position(0, 3)))
)]
#[case::multiline_rejected(
    line_range(line_position(0, 0), line_position(1, 0)), // spans "a\nb"
    Err(RangeError::NotSingleLine),
)]
fn shrink(#[case] range: Range, #[case] expected: Result<Range, RangeError>) {
    assert_eq!(range.shrink(1, 2), expected);
}

/// `sub` resolves positions relative to the range start: interior bounds
/// map to their absolute spots, an empty sub-range collapses to a point
/// offset by the range's own start, and the offsets carry across lines.
#[rstest]
#[case::interior(
    line_range(line_position(0, 0), line_position(0, 10)),
    line_position(0, 2),
    line_position(0, 8),
    line_range(line_position(0, 2), line_position(0, 8))
)]
#[case::empty_collapses_to_offset_point(
    line_range(line_position(1, 5), line_position(1, 15)),
    line_position(0, 3),
    line_position(0, 3),
    line_range(line_position(1, 8), line_position(1, 8))
)]
#[case::multiline_end(
    line_range(line_position(0, 0), line_position(2, 10)),
    line_position(0, 5),
    line_position(1, 3),
    line_range(line_position(0, 5), line_position(1, 3))
)]
// Relative positions are offset by the range start across lines: line 2
// past a (5, 0) start lands on line 7.
#[case::offset_by_range_start_across_lines(
    line_range(line_position(5, 0), line_position(7, 10)),
    line_position(2, 3),
    line_position(2, 8),
    line_range(line_position(7, 3), line_position(7, 8))
)]
fn sub_resolves_relative_positions(
    #[case] range: Range,
    #[case] from: Position,
    #[case] to: Position,
    #[case] expected: Range,
) {
    let sub_range = range.sub(TEXT, from, to).expect("valid range");
    assert_eq!(sub_range, expected);
}

/// `sub_delimited` splits around the delimiter: both sides are Some when
/// the delimiter is interior, an absent side is None — a delimiter at the
/// start leaves no left side, at the end no right side, an absent delimiter
/// leaves the whole range on the left, and empty text has neither.
#[rstest]
#[case::single_byte_delimiter(
    line_range(line_position(0, 0), line_position(0, 7)),
    "one/two",
    D1,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    Some(line_range(line_position(0, 4), line_position(0, 7)))
)]
#[case::newline_delimiter(
    line_range(line_position(0, 0), line_position(1, 3)),
    "abc\ndef",
    LF,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    Some(line_range(line_position(1, 0), line_position(1, 3)))
)]
#[case::delimiter_at_start(
    line_range(line_position(0, 0), line_position(0, 4)),
    "/abc",
    D1,
    None,
    Some(line_range(line_position(0, 1), line_position(0, 4)))
)]
// Delimiter at the end: no right side.
#[case::delimiter_at_end(
    line_range(line_position(0, 0), line_position(0, 4)),
    "abc/",
    D1,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    None
)]
// No delimiter at all: the whole range is the left side.
#[case::no_delimiter(
    line_range(line_position(0, 0), line_position(0, 3)),
    "abc",
    D1,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    None
)]
#[case::empty_text(
    line_range(line_position(0, 0), line_position(0, 0)),
    TEXT,
    D1,
    None,
    None
)]
fn sub_delimited_splits_around_the_delimiter(
    #[case] range: Range,
    #[case] text: &str,
    #[case] delimiter: char,
    #[case] expected_left: Option<Range>,
    #[case] expected_right: Option<Range>,
) {
    let (left, right) = range.sub_delimited(text, delimiter).expect("valid range");
    assert_eq!(left, expected_left);
    assert_eq!(right, expected_right);
}

/// `sub_delimited_tri` slices three consecutive segments around the two
/// delimiters (the second fixed at `@`): a missing delimiter leaves the
/// remaining segments None.
#[rstest]
#[case::all_delimiters(
    line_range(line_position(0, 0), line_position(0, 13)),
    "one/two@three",
    D1,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    Some(line_range(line_position(0, 4), line_position(0, 7))),
    Some(line_range(line_position(0, 8), line_position(0, 13)))
)]
#[case::third_missing(
    line_range(line_position(0, 0), line_position(0, 7)),
    "one/two",
    D1,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    Some(line_range(line_position(0, 4), line_position(0, 7))),
    None
)]
#[case::second_and_third_missing(
    line_range(line_position(0, 0), line_position(0, 3)),
    "abc",
    D1,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    None,
    None
)]
#[case::multiline_delimiters(
    line_range(line_position(0, 0), line_position(2, 3)),
    "one\ntwo\n@@@",
    LF,
    Some(line_range(line_position(0, 0), line_position(0, 3))),
    Some(line_range(line_position(1, 0), line_position(2, 0))),
    Some(line_range(line_position(2, 1), line_position(2, 3)))
)]
fn sub_delimited_tri_slices_three_segments(
    #[case] range: Range,
    #[case] text: &str,
    #[case] delim0: char,
    #[case] expected_first: Option<Range>,
    #[case] expected_second: Option<Range>,
    #[case] expected_third: Option<Range>,
) {
    let (first, second, third) = range
        .sub_delimited_tri(text, delim0, D2)
        .expect("valid range");
    assert_eq!(first, expected_first);
    assert_eq!(second, expected_second);
    assert_eq!(third, expected_third);
}

/// Mirror rows: `split_off_left` at the relative start yields the empty left
/// half, `split_off_right` at the relative end the empty right half.
#[rstest]
fn split_off_boundaries() {
    let range = line_range(line_position(1, 5), line_position(1, 15));
    assert_eq!(
        range
            .split_off_left(TEXT, line_position(0, 0))
            .expect("valid range"),
        line_range(line_position(1, 5), line_position(1, 5)),
    );
    assert_eq!(
        range
            .split_off_right(TEXT, line_position(0, 10))
            .expect("valid range"),
        line_range(line_position(1, 15), line_position(1, 15)),
    );
}

/// Out-of-range positions are rejected on both splitting paths.
#[rstest]
fn out_of_range_positions_return_position_out_of_range() {
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 10))
            .split_at(TEXT, line_position(0, 11))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 10))
            .sub(TEXT, line_position(0, 3), line_position(0, 11))
            .unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[rstest]
fn reversed_sub_positions_return_start_after_end() {
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 10))
            .sub(TEXT, line_position(0, 7), line_position(0, 3))
            .unwrap_err(),
        RangeError::StartAfterEnd,
    );
}

#[rstest]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    assert_eq!(
        line_range(line_position(0, 0), line_position(0, 7))
            .sub_delimited("one—two", '—')
            .unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' },
    );
}
