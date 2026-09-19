use rstest::rstest;

use crate::text_utils::RangeError;

use super::RangeExt;

type ByteRange = std::ops::Range<usize>;
type BytePosition = usize;

const TEXT: &str = ""; // Byte range & position do not need text information

const fn r(start: BytePosition, end: BytePosition) -> ByteRange {
    start..end
}

/// `split_at` divides the range at a relative position: the position becomes
/// the shared boundary byte, and the degenerate boundaries yield an empty
/// half.
#[rstest]
#[case::interior(r(0, 10), 5, r(0, 5), r(5, 10))]
#[case::at_range_start(r(5, 15), 0, r(5, 5), r(5, 15))]
#[case::at_range_end(r(5, 15), 10, r(5, 15), r(15, 15))]
fn split_at_divides_the_range_at_a_relative_position(
    #[case] range: ByteRange,
    #[case] at: BytePosition,
    #[case] expected_left: ByteRange,
    #[case] expected_right: ByteRange,
) {
    let (left, right) = range.split_at(TEXT, at).expect("valid range");
    assert_eq!(left, expected_left);
    assert_eq!(right, expected_right);
}

/// Mirror rows: `split_off_left` keeps the left of the position,
/// `split_off_right` the right.
#[rstest]
fn split_off_returns_the_kept_side() {
    let left = r(0, 10).split_off_left(TEXT, 3).expect("valid range");
    assert_eq!(left, r(0, 3));

    let right = r(0, 10).split_off_right(TEXT, 7).expect("valid range");
    assert_eq!(right, r(7, 10));
}

/// `shrink` narrows the range by the given counts on each edge.
#[rstest]
fn basic_shrink() {
    let shrunk = r(0, 10).shrink(2, 3).expect("valid range");
    assert_eq!(shrunk, r(2, 7));
}

/// `sub` positions count from the range start, not from the text start:
/// interior bounds map to their absolute spots, an empty sub-range
/// collapses to a point offset by the range's own start, the len-relative
/// boundary is inclusive, and len + 1 is out of range for either bound.
#[rstest]
#[case::interior(r(0, 10), 2, 8, Ok(r(2, 8)))]
#[case::empty_collapses_to_offset_point(r(5, 15), 3, 3, Ok(r(8, 8)))]
#[case::end_relative_boundary(r(5, 10), 5, 5, Ok(r(10, 10)))]
#[case::whole_tail(r(5, 10), 0, 5, Ok(r(5, 10)))]
#[case::one_past_len(r(5, 10), 6, 6, Err(RangeError::PositionOutOfRange))]
fn sub_resolves_relative_positions(
    #[case] range: ByteRange,
    #[case] from: BytePosition,
    #[case] to: BytePosition,
    #[case] expected: Result<ByteRange, RangeError>,
) {
    assert_eq!(range.sub(TEXT, from, to), expected);
}

/// Mirror rows: `split_off_left` at the relative start yields the empty left
/// half, `split_off_right` at the relative end the empty right half.
#[rstest]
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

// Delimiter cases live in the `sub_delimited` / `sub_delimited_tri`
// doctests in `mod.rs`; they are not duplicated here. The delimiter
// error paths are tested below.

/// Out-of-range positions are rejected on both splitting paths.
#[rstest]
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

/// Reversed sub bounds are rejected, and `split_at` positions count from the
/// range start: at 6 lies beyond the `5..10` range's length.
#[rstest]
fn sub_and_split_at_reject_invalid_positions() {
    assert_eq!(
        r(0, 10).sub(TEXT, 7, 3).unwrap_err(),
        RangeError::StartAfterEnd,
    );
    assert_eq!(
        r(5, 10).split_at(TEXT, 6).unwrap_err(),
        RangeError::PositionOutOfRange,
    );
}

#[rstest]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    assert_eq!(
        r(0, 9).sub_delimited("one—two", '—').unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' },
    );
}

/// Both splitting methods validate the text length against the range,
/// including on non-zero start ranges.
#[rstest]
fn mismatched_text_length_returns_text_range_mismatch() {
    // sub_delimited validates the text length against the range.
    assert_eq!(
        r(0, 7).sub_delimited("short", '/').unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 5,
            range_len: 7
        },
    );

    // A non-zero start range validates too: the length check also fires
    // there.
    assert_eq!(
        r(5, 12).sub_delimited_tri("one/tw", '/', '@').unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 6,
            range_len: 7
        },
    );
}

/// `sub_delimited` requires the exact text of the range: matching length
/// splits around the delimiter, a mismatch errors.
#[rstest]
#[case::exact_length(
    r(5, 12),
    "one/two",
    Ok((Some(r(5, 8)), Some(r(9, 12)))),
)]
#[case::short_text(
    r(5, 12),
    "one/tw",
    Err(RangeError::TextRangeMismatch {
        text_len: 6,
        range_len: 7,
    }),
)]
fn sub_delimited_requires_exact_text_length(
    #[case] range: ByteRange,
    #[case] text: &str,
    #[case] expected: Result<(Option<ByteRange>, Option<ByteRange>), RangeError>,
) {
    assert_eq!(range.sub_delimited(text, '/'), expected);
}
