// tree_sitter::Point/Range — same Ts... naming as tree_sitter.rs, the Ts... half of the RangeExt family.
use rstest::rstest;
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

/// `split_at` divides the range at a position relative to the range start:
/// the position becomes the shared boundary byte and point, and the
/// degenerate boundaries yield an empty half.
#[rstest]
#[case::mid_row(
    r(0, p(0, 0), 5, p(0, 5)),
    "hello",
    p(0, 2),
    r(0, p(0, 0), 2, p(0, 2)),
    r(2, p(0, 2), 5, p(0, 5))
)]
#[case::multiline_boundary(
    r(0, p(0, 0), 7, p(1, 3)),
    "one\ntwo",
    p(1, 1),
    r(0, p(0, 0), 5, p(1, 1)),
    r(5, p(1, 1), 7, p(1, 3))
)]
// Points are offset by the range start: the position is relative to the
// range, not taken as absolute text coordinates.
#[case::offset_by_range_start(
    r(5, p(1, 3), 10, p(2, 2)),
    "ab\ncd",
    p(0, 1),
    r(5, p(1, 3), 6, p(1, 4)),
    r(6, p(1, 4), 10, p(2, 2))
)]
#[case::at_range_start(
    r(0, p(0, 0), 5, p(0, 5)),
    "hello",
    p(0, 0),
    r(0, p(0, 0), 0, p(0, 0)),
    r(0, p(0, 0), 5, p(0, 5))
)]
#[case::at_range_end(
    r(0, p(0, 0), 5, p(0, 5)),
    "hello",
    p(0, 5),
    r(0, p(0, 0), 5, p(0, 5)),
    r(5, p(0, 5), 5, p(0, 5))
)]
#[case::newline_boundary(
    r(0, p(0, 0), 11, p(1, 5)),
    "line1\nline2",
    p(1, 0),
    r(0, p(0, 0), 6, p(1, 0)),
    r(6, p(1, 0), 11, p(1, 5))
)]
fn split_at_divides_the_range_at_a_relative_position(
    #[case] range: TsRange,
    #[case] text: &str,
    #[case] at: TsPosition,
    #[case] expected_left: TsRange,
    #[case] expected_right: TsRange,
) {
    let (left, right) = range.split_at(text, at).expect("valid range");
    assert_eq!(left, expected_left);
    assert_eq!(right, expected_right);
}

/// Mirror rows: `split_off_left` keeps the left of the position,
/// `split_off_right` the right.
#[rstest]
fn split_off_returns_the_kept_side() {
    let text = "hello";
    let left = r(0, p(0, 0), 5, p(0, 5))
        .split_off_left(text, p(0, 3))
        .expect("valid range");
    assert_eq!(left, r(0, p(0, 0), 3, p(0, 3)));

    let right = r(0, p(0, 0), 5, p(0, 5))
        .split_off_right(text, p(0, 2))
        .expect("valid range");
    assert_eq!(right, r(2, p(0, 2), 5, p(0, 5)));
}

/// shrink narrows a single-line range by the given byte counts on each
/// edge; a multiline range has no single-line edge to shrink from and is
/// rejected.
#[rstest]
#[case::single_line(r(0, p(0, 0), 5, p(0, 5)), Ok(r(1, p(0, 1), 3, p(0, 3))))]
#[case::multiline_rejected(
    r(0, p(0, 0), 2, p(1, 0)), // spans "a\nb"
    Err(RangeError::NotSingleLine),
)]
fn shrink(#[case] range: TsRange, #[case] expected: Result<TsRange, RangeError>) {
    assert_eq!(range.shrink(1, 2), expected);
}

/// `sub` resolves positions relative to the range start — by byte offset
/// within the row and by row offset across lines: interior bounds map to
/// their absolute spots, an empty sub-range collapses to a point offset by
/// the range's own start, and the exact end-of-text position resolves to
/// the range's end byte.
#[rstest]
#[case::interior(
    r(0, p(0, 0), 5, p(0, 5)),
    "hello",
    p(0, 1),
    p(0, 4),
    r(1, p(0, 1), 4, p(0, 4))
)]
#[case::empty_collapses_to_offset_point(
    r(10, p(1, 5), 15, p(1, 10)),
    "hello",
    p(0, 2),
    p(0, 2),
    r(12, p(1, 7), 12, p(1, 7))
)]
#[case::multiline_end(
    r(0, p(0, 0), 13, p(2, 5)),
    "one\ntwo\nthree",
    p(0, 2),
    p(1, 1),
    r(2, p(0, 2), 5, p(1, 1))
)]
#[case::spans_rows_end_to_end(
    r(0, p(0, 0), 17, p(2, 5)),
    "line1\nline2\nline3",
    p(0, 3),
    p(2, 2),
    r(3, p(0, 3), 14, p(2, 2))
)]
// Points are offset by the range start row, not absolute text coordinates.
#[case::offset_by_range_start_row(
    r(0, p(2, 0), 5, p(3, 2)),
    "ab\ncd",
    p(1, 0),
    p(1, 2),
    r(3, p(3, 0), 5, p(3, 2))
)]
#[case::mid_text(
    r(0, p(0, 0), 8, p(2, 2)),
    "ab\ncd\nef",
    p(0, 1),
    p(1, 1),
    r(1, p(0, 1), 4, p(1, 1))
)]
// The exact end-of-text position resolves to the range's end byte.
#[case::exact_end_of_text(
    r(0, p(0, 0), 8, p(2, 2)),
    "ab\ncd\nef",
    p(2, 2),
    p(2, 2),
    r(8, p(2, 2), 8, p(2, 2))
)]
fn sub_resolves_relative_positions(
    #[case] range: TsRange,
    #[case] text: &str,
    #[case] from: TsPosition,
    #[case] to: TsPosition,
    #[case] expected: TsRange,
) {
    let sub_range = range.sub(text, from, to).expect("valid range");
    assert_eq!(sub_range, expected);
}

/// Any position component outside its text is rejected — a column past its
/// row's text, a row past the last line, a column coinciding with the end
/// point on an empty row — even though later rows exist.
#[rstest]
#[case::end_past_column(r(0, p(0, 0), 5, p(0, 5)), "hello", p(0, 1), p(0, 9))]
#[case::start_past_column(r(0, p(0, 0), 5, p(0, 5)), "hello", p(0, 9), p(0, 9))]
// A position past its row's text is nowhere in the text even though later
// rows exist.
#[case::past_its_own_row(r(0, p(0, 0), 8, p(2, 2)), "ab\ncd\nef", p(0, 3), p(1, 0))]
// The end-of-text check must match row and column exactly: a position whose
// column coincides with the end point's, on an empty row, is still nowhere
// in the text.
#[case::empty_row_column_exactness(r(0, p(0, 0), 6, p(2, 2)), "ab\n\ncd", p(1, 2), p(2, 0))]
// A row past the last line is equally out of range.
#[case::past_last_row(r(0, p(0, 0), 5, p(0, 5)), "hello", p(0, 1), p(1, 5))]
fn sub_positions_beyond_the_text_return_position_out_of_range(
    #[case] range: TsRange,
    #[case] text: &str,
    #[case] from: TsPosition,
    #[case] to: TsPosition,
) {
    assert_eq!(
        range.sub(text, from, to).unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[rstest]
fn reversed_sub_positions_return_start_after_end() {
    let text = "hello";
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .sub(text, p(0, 3), p(0, 1))
            .unwrap_err(),
        RangeError::StartAfterEnd,
    );
}

/// `sub_delimited` splits around the delimiter: both sides are Some when
/// the delimiter is interior, an absent side is None — a delimiter at the
/// start leaves no left side, at the end no right side, an absent delimiter
/// leaves the whole range on the left, and empty text has neither.
#[rstest]
#[case::single_byte_delimiter(
    r(0, p(0, 0), 7, p(0, 7)),
    "one/two",
    D1,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    Some(r(4, p(0, 4), 7, p(0, 7)))
)]
#[case::newline_delimiter(
    r(0, p(0, 0), 7, p(1, 3)),
    "abc\ndef",
    LF,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    Some(r(4, p(1, 0), 7, p(1, 3)))
)]
#[case::multiline_complex(
    r(0, p(0, 0), 22, p(2, 3)),
    "start\nfirst/second\nend",
    D1,
    Some(r(0, p(0, 0), 11, p(1, 5))),
    Some(r(12, p(1, 6), 22, p(2, 3)))
)]
#[case::delimiter_at_start(
    r(0, p(0, 0), 4, p(0, 4)),
    "/abc",
    D1,
    None,
    Some(r(1, p(0, 1), 4, p(0, 4)))
)]
// Delimiter at the end: no right side.
#[case::delimiter_at_end(
    r(0, p(0, 0), 4, p(0, 4)),
    "abc/",
    D1,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    None
)]
// No delimiter at all: the whole range is the left side.
#[case::no_delimiter(
    r(0, p(0, 0), 3, p(0, 3)),
    "abc",
    D1,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    None
)]
#[case::empty_text(r(0, p(0, 0), 0, p(0, 0)), "", D1, None, None)]
fn sub_delimited_splits_around_the_delimiter(
    #[case] range: TsRange,
    #[case] text: &str,
    #[case] delimiter: char,
    #[case] expected_left: Option<TsRange>,
    #[case] expected_right: Option<TsRange>,
) {
    let (left, right) = range.sub_delimited(text, delimiter).expect("valid range");
    assert_eq!(left, expected_left);
    assert_eq!(right, expected_right);
}

/// `sub_delimited_tri` slices three consecutive segments around the two
/// delimiters (the second fixed at `@`): a missing delimiter leaves the
/// remaining segments None.
#[rstest]
#[case::both_delimiters(
    r(0, p(0, 0), 13, p(0, 13)),
    "one/two@three",
    D1,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    Some(r(4, p(0, 4), 7, p(0, 7))),
    Some(r(8, p(0, 8), 13, p(0, 13)))
)]
#[case::newline_first_delimiter(
    r(0, p(0, 0), 11, p(2, 3)),
    "one\ntwo\n@@@",
    LF,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    Some(r(4, p(1, 0), 8, p(2, 0))),
    Some(r(9, p(2, 1), 11, p(2, 3)))
)]
// A non-zero start range: the remainder is sliced relative to the range's
// own start byte.
#[case::non_zero_start_relative(
    r(5, p(1, 3), 10, p(1, 8)),
    "a/b@c",
    D1,
    Some(r(5, p(1, 3), 6, p(1, 4))),
    Some(r(7, p(1, 5), 8, p(1, 6))),
    Some(r(9, p(1, 7), 10, p(1, 8)))
)]
#[case::third_missing(
    r(0, p(0, 0), 7, p(0, 7)),
    "one/two",
    D1,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    Some(r(4, p(0, 4), 7, p(0, 7))),
    None
)]
#[case::no_delimiters(
    r(0, p(0, 0), 3, p(0, 3)),
    "abc",
    D1,
    Some(r(0, p(0, 0), 3, p(0, 3))),
    None,
    None
)]
fn sub_delimited_tri_slices_three_segments(
    #[case] range: TsRange,
    #[case] text: &str,
    #[case] delim0: char,
    #[case] expected_first: Option<TsRange>,
    #[case] expected_second: Option<TsRange>,
    #[case] expected_third: Option<TsRange>,
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

/// All splitting paths validate the text length against the range.
#[rstest]
fn mismatched_text_length_returns_text_range_mismatch() {
    // sub_delimited validates the text length against the range.
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

    // split_at enforces the same convention.
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

    // A non-zero start range validates too: the length check also fires there.
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

#[rstest]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    let text = "one—two";
    assert_eq!(
        r(0, p(0, 0), 9, p(0, 9))
            .sub_delimited(text, '—')
            .unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' },
    );
}

/// `split_at` validates the text length on non-zero start ranges too: the
/// range must be the exact `5..(5 + text.len())` tail of a larger text.
#[rstest]
#[case::matching_length(
    "one/two",
    Ok((r(5, p(0, 3), 7, p(0, 5)), r(7, p(0, 5), 12, p(0, 10)))),
)]
#[case::mismatched_length(
    "one/tw",
    Err(RangeError::TextRangeMismatch {
        text_len: 6,
        range_len: 7,
    }),
)]
fn split_at_validates_text_length_on_nonzero_start_ranges(
    #[case] text: &str,
    #[case] expected: Result<(TsRange, TsRange), RangeError>,
) {
    assert_eq!(
        r(5, p(0, 3), 12, p(0, 10)).split_at(text, p(0, 2)),
        expected,
    );
}

/// A column past the row's text or a row past the last line is out of range.
#[rstest]
#[case::past_column(r(0, p(0, 0), 5, p(0, 5)), "hello", p(0, 9))]
#[case::past_last_row(r(0, p(0, 0), 5, p(0, 5)), "hello", p(2, 0))]
fn split_at_beyond_the_text_returns_position_out_of_range(
    #[case] range: TsRange,
    #[case] text: &str,
    #[case] at: TsPosition,
) {
    assert_eq!(
        range.split_at(text, at).unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}
