mod bytes;
mod lsp;

#[cfg(test)]
mod bytes_tests;
#[cfg(test)]
mod lsp_tests;
#[cfg(feature = "tree-sitter")]
mod tree_sitter;
#[cfg(all(test, feature = "tree-sitter"))]
mod tree_sitter_tests;

use crate::error::RangeError;

fn check_delimiter(delimiter: char) -> Result<(), RangeError> {
    if delimiter.len_utf8() == 1 {
        Ok(())
    } else {
        Err(RangeError::DelimiterNotSingleByte { delimiter })
    }
}

fn check_text_length(text_len: usize, range_len: usize) -> Result<(), RangeError> {
    if text_len == range_len {
        Ok(())
    } else {
        Err(RangeError::TextRangeMismatch {
            text_len,
            range_len,
        })
    }
}

/// The three optional subranges produced by [`RangeExt::sub_delimited_tri`].
///
/// Written out, this type trips `clippy::type_complexity` in the trait
/// signature, so the tuple is factored into a definition.
type TriSubranges<T> = (Option<T>, Option<T>, Option<T>);

/// Extensions for splitting, shrinking, and delimiting ranges.
///
/// Works with different kinds of ranges:
///
/// 1. Byte ranges
/// 2. LSP protocol ranges
/// 3. Tree-sitter ranges
///
/// Provides methods for:
///
/// - Splitting ranges into parts
/// - Expanding and shrinking ranges
/// - Creating subranges based on positions and/or string delimiters
///
/// # Examples
///
/// ```
/// use async_language_server::text_utils::RangeExt;
///
/// let (left, right) = (0..7).split_at("one/two", 3).expect("position is inside the range");
/// assert_eq!(left, 0..3);
/// assert_eq!(right, 3..7);
///
/// assert_eq!((0..7).shrink(1, 2).expect("single-line range"), 1..5);
/// assert_eq!((0..7).sub("one/two", 1, 5).expect("positions are inside the range"), 1..5);
/// ```
pub trait RangeExt: Sized {
    /// The position type used by this kind of range.
    type Position;

    /// Splits the given range into two parts at the specified position.
    ///
    /// - The `text` parameter must be the exact text corresponding to this range.
    ///   It is used for tree-sitter ranges, where both line+col and byte offsets are needed.
    /// - The `at` position is _relative_ to the start of the range.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::PositionOutOfRange`] if `at` lies beyond the
    /// end of the range, or (for tree-sitter ranges) does not land on a
    /// character boundary of the text.
    fn split_at(self, text: &str, at: Self::Position) -> Result<(Self, Self), RangeError>;

    /// Splits the given range into two parts at the specified position,
    /// and returns the left part.
    ///
    /// # Errors
    ///
    /// Returns the error of [`RangeExt::split_at`].
    fn split_off_left(self, text: &str, at: Self::Position) -> Result<Self, RangeError> {
        Ok(self.split_at(text, at)?.0)
    }

    /// Splits the given range into two parts at the specified position,
    /// and returns the right part.
    ///
    /// # Errors
    ///
    /// Returns the error of [`RangeExt::split_at`].
    fn split_off_right(self, text: &str, at: Self::Position) -> Result<Self, RangeError> {
        Ok(self.split_at(text, at)?.1)
    }

    /// Shrinks the same-line range by the given character count, on both the left and right.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::NotSingleLine`] if the range spans multiple lines.
    fn shrink(self, amount_left: usize, amount_right: usize) -> Result<Self, RangeError>;

    /// Returns a subrange of the range, starting at `from` and ending at `to`.
    ///
    /// Both positions are _relative_ to the start of the range, and the range
    /// itself must be an absolute range.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::PositionOutOfRange`] if `from` or `to` lie beyond
    /// the end of the range or (for tree-sitter ranges) do not land on a
    /// character boundary of the text, or [`RangeError::StartAfterEnd`] if
    /// `from > to`.
    fn sub(self, text: &str, from: Self::Position, to: Self::Position) -> Result<Self, RangeError>;

    /// Splits the given range into two optional subranges, using the given delimiter.
    ///
    /// The range should be the exact range for the given text.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_language_server::text_utils::RangeExt;
    ///
    /// const D: char = '/';
    ///
    /// assert_eq!((0..7).sub_delimited("one/two", D).expect("valid range"), (Some(0..3), Some(4..7)));
    /// assert_eq!((0..4).sub_delimited("/two", D).expect("valid range"), (None, Some(1..4)));
    /// assert_eq!((0..4).sub_delimited("one/", D).expect("valid range"), (Some(0..3), None));
    /// assert_eq!((0..3).sub_delimited("one", D).expect("valid range"), (Some(0..3), None));
    /// assert_eq!((0..0).sub_delimited("", D).expect("valid range"), (None, None));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::DelimiterNotSingleByte`] for a multi-byte
    /// delimiter, and (for the byte and tree-sitter ranges)
    /// [`RangeError::TextRangeMismatch`] when the text is not the exact
    /// text of the range.
    fn sub_delimited(
        self,
        text: &str,
        delimiter: char,
    ) -> Result<(Option<Self>, Option<Self>), RangeError>;

    /// Splits the given range into _three_ optional subranges,
    /// using the two given delimiters, consecutively.
    ///
    /// The range should be the exact range corresponding to the given text.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_language_server::text_utils::RangeExt;
    ///
    /// const D0: char = '/';
    /// const D1: char = '@';
    ///
    /// assert_eq!(
    ///     (0..13).sub_delimited_tri("one/two@three", D0, D1).expect("valid range"),
    ///     (Some(0..3), Some(4..7), Some(8..13)),
    /// );
    /// assert_eq!(
    ///     (0..7).sub_delimited_tri("one/two", D0, D1).expect("valid range"),
    ///     (Some(0..3), Some(4..7), None),
    /// );
    /// assert_eq!(
    ///     (0..3).sub_delimited_tri("one", D0, D1).expect("valid range"),
    ///     (Some(0..3), None, None),
    /// );
    /// assert_eq!((0..0).sub_delimited_tri("", D0, D1).expect("valid range"), (None, None, None));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::DelimiterNotSingleByte`] for a multi-byte
    /// delimiter, and (for the byte and tree-sitter ranges)
    /// [`RangeError::TextRangeMismatch`] when the text is not the exact
    /// text of the range.
    fn sub_delimited_tri(
        self,
        text: &str,
        delim0: char,
        delim1: char,
    ) -> Result<TriSubranges<Self>, RangeError>;
}
