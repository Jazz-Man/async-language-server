use async_lsp::lsp_types::PositionEncodingKind as LspPositionEncoding;

/// A position encoding supported by this crate.
///
/// Easy to copy and match against, unlike `PositionEncodingKind`, and
/// contains several similar utilities, that are additionally `const`.
///
/// # Examples
///
/// ```
/// use async_language_server::text_utils::Encoding;
///
/// // The LSP default when the client does not negotiate an encoding.
/// assert_eq!(Encoding::default(), Encoding::UTF16);
/// assert_eq!(Encoding::UTF8.as_str(), "utf-8");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// Character offsets count UTF-8 code units.
    UTF8,
    /// Character offsets count UTF-16 code units.
    ///
    /// This is the default for the Language Server Protocol - if a client
    /// does not specify which position encoding they prefer and / or support,
    /// this encoding must always be used.
    #[default]
    UTF16,
    /// Character offsets count UTF-32 code units.
    ///
    /// This encoding is equivalent to Unicode code points, so it may also
    /// be used for an encoding-agnostic representation of character offsets.
    UTF32,
}

impl Encoding {
    /// Converts the encoding into its `lsp_types` counterpart.
    #[must_use]
    pub const fn into_lsp(self) -> LspPositionEncoding {
        match self {
            Self::UTF8 => LspPositionEncoding::UTF8,
            Self::UTF16 => LspPositionEncoding::UTF16,
            Self::UTF32 => LspPositionEncoding::UTF32,
        }
    }

    /// Returns the wire representation of the encoding (`utf-8`, `utf-16`, or `utf-32`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::UTF8 => "utf-8",
            Self::UTF16 => "utf-16",
            Self::UTF32 => "utf-32",
        }
    }

    /// Creates an encoding from its `lsp_types` counterpart.
    ///
    /// # Panics
    ///
    /// Panics if the encoding kind is not one of UTF-8, UTF-16, or UTF-32.
    #[must_use]
    pub fn from_lsp(encoding: &LspPositionEncoding) -> Self {
        if encoding == &LspPositionEncoding::UTF8 {
            Self::UTF8
        } else if encoding == &LspPositionEncoding::UTF16 {
            Self::UTF16
        } else if encoding == &LspPositionEncoding::UTF32 {
            Self::UTF32
        } else {
            panic!("unsupported position encoding kind: {encoding:?}")
        }
    }

    /// Creates an encoding from its `lsp_types` counterpart, if it is one of
    /// the supported kinds (UTF-8, UTF-16, UTF-32).
    ///
    /// Returns `None` for any other kind: client capabilities can carry
    /// values this crate does not know, and negotiation ignores them
    /// instead of failing.
    #[must_use]
    pub fn try_from_lsp(encoding: &LspPositionEncoding) -> Option<Self> {
        if encoding == &LspPositionEncoding::UTF8 {
            Some(Self::UTF8)
        } else if encoding == &LspPositionEncoding::UTF16 {
            Some(Self::UTF16)
        } else if encoding == &LspPositionEncoding::UTF32 {
            Some(Self::UTF32)
        } else {
            None
        }
    }
}

impl From<&Encoding> for Encoding {
    fn from(encoding: &Encoding) -> Self {
        *encoding
    }
}

impl From<&LspPositionEncoding> for Encoding {
    fn from(encoding: &LspPositionEncoding) -> Self {
        Self::from_lsp(encoding)
    }
}

impl From<LspPositionEncoding> for Encoding {
    fn from(value: LspPositionEncoding) -> Self {
        Self::from_lsp(&value)
    }
}

impl From<&Encoding> for LspPositionEncoding {
    fn from(value: &Encoding) -> Self {
        value.into_lsp()
    }
}

impl From<Encoding> for LspPositionEncoding {
    fn from(value: Encoding) -> Self {
        value.into_lsp()
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::PositionEncodingKind;

    use super::{Encoding, LspPositionEncoding};

    #[test]
    fn try_from_lsp_returns_none_for_unknown_kinds() {
        assert_eq!(
            Encoding::try_from_lsp(&PositionEncodingKind::new("utf-7")),
            None
        );
        assert_eq!(
            Encoding::try_from_lsp(&LspPositionEncoding::UTF8),
            Some(Encoding::UTF8)
        );
        assert_eq!(
            Encoding::try_from_lsp(&LspPositionEncoding::UTF16),
            Some(Encoding::UTF16)
        );
        assert_eq!(
            Encoding::try_from_lsp(&LspPositionEncoding::UTF32),
            Some(Encoding::UTF32)
        );
    }
}
