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
/// let (left, right) = (0..7).split_at("one/two", 3);
/// assert_eq!(left, 0..3);
/// assert_eq!(right, 3..7);
///
/// assert_eq!((0..7).shrink(1, 2), 1..5);
/// assert_eq!((0..7).sub("one/two", 1, 5), 1..5);
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
    /// # Panics
    ///
    /// Panics if `at` lies beyond the end of the range.
    #[must_use]
    fn split_at(self, text: &str, at: Self::Position) -> (Self, Self);

    /// Splits the given range into two parts at the specified position,
    /// and returns the left part.
    ///
    /// - The `text` parameter must be the exact text corresponding to this range.
    ///   It is used for tree-sitter ranges, where both line+col and byte offsets are needed.
    /// - The `at` position is _relative_ to the start of the range.
    #[must_use]
    fn split_off_left(self, text: &str, at: Self::Position) -> Self {
        let (left, _) = self.split_at(text, at);
        left
    }

    /// Splits the given range into two parts at the specified position,
    /// and returns the right part.
    ///
    /// - The `text` parameter must be the exact text corresponding to this range.
    ///   It is used for tree-sitter ranges, where both line+col and byte offsets are needed.
    /// - The `at` position is _relative_ to the start of the range.
    #[must_use]
    fn split_off_right(self, text: &str, at: Self::Position) -> Self {
        let (_, right) = self.split_at(text, at);
        right
    }

    /// Shrinks the same-line range by the given character count, on both the left and right.
    ///
    /// # Panics
    ///
    /// Panics if the range spans multiple lines.
    #[must_use]
    fn shrink(self, amount_left: usize, amount_right: usize) -> Self;

    /// Returns a subrange of the range, starting at `from` and ending at `to`.
    ///
    /// - The `text` parameter must be the exact text corresponding to this range.
    ///   It is used for tree-sitter ranges, where both line+col and byte offsets are needed.
    /// - Both positions are _relative_ to the start of the range, and that the range itself
    ///   must be an absolute range.
    ///
    /// # Panics
    ///
    /// Panics if `from` or `to` lie beyond the end of the range, or if `from > to`.
    #[must_use]
    fn sub(self, text: &str, from: Self::Position, to: Self::Position) -> Self;

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
    /// assert_eq!((0..7).sub_delimited("one/two", D), (Some(0..3), Some(4..7)));
    /// assert_eq!((0..4).sub_delimited("/two", D), (None, Some(1..4)));
    /// assert_eq!((0..4).sub_delimited("one/", D), (Some(0..3), None));
    /// assert_eq!((0..3).sub_delimited("one", D), (Some(0..3), None));
    /// assert_eq!((0..0).sub_delimited("", D), (None, None));
    /// ```
    ///
    /// # Panics
    ///
    /// - Panics if the text and range are not the exact same length.
    /// - Panics if the delimiter is not a single-byte UTF8 character.
    #[must_use]
    fn sub_delimited(self, text: &str, delimiter: char) -> (Option<Self>, Option<Self>);

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
    ///     (0..13).sub_delimited_tri("one/two@three", D0, D1),
    ///     (Some(0..3), Some(4..7), Some(8..13)),
    /// );
    /// assert_eq!(
    ///     (0..7).sub_delimited_tri("one/two", D0, D1),
    ///     (Some(0..3), Some(4..7), None),
    /// );
    /// assert_eq!(
    ///     (0..3).sub_delimited_tri("one", D0, D1),
    ///     (Some(0..3), None, None),
    /// );
    /// assert_eq!((0..0).sub_delimited_tri("", D0, D1), (None, None, None));
    /// ```
    ///
    /// # Panics
    ///
    /// - Panics if the text and range are not the exact same length.
    /// - Panics if any delimiter is not a single-byte UTF8 character.
    #[must_use]
    fn sub_delimited_tri(
        self,
        text: &str,
        delim0: char,
        delim1: char,
    ) -> (Option<Self>, Option<Self>, Option<Self>);
}
