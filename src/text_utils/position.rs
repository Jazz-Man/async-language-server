use async_lsp::lsp_types::Position as LspPosition;

/// A zero-based line and column position.
///
/// May be cheaply copied, as well as converted
/// to / from language server positions.
///
/// # Examples
///
/// ```
/// use async_language_server::text_utils::Position;
/// use async_lsp::lsp_types::Position as LspPosition;
///
/// let position = Position { line: 3, col: 7 };
/// let lsp = position.into_lsp();
/// assert_eq!(lsp, LspPosition { line: 3, character: 7 });
/// assert_eq!(Position::from_lsp(lsp), position);
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// Zero-based line index.
    pub line: usize,
    /// Column offset within the line, in the units of the encoding in use.
    pub col: usize,
}

impl Position {
    /// Creates a position from an LSP position.
    #[must_use]
    pub const fn from_lsp(position: LspPosition) -> Self {
        Self {
            line: position.line as usize,
            col: position.character as usize,
        }
    }

    /// Converts the position into an LSP position.
    #[must_use]
    pub const fn into_lsp(self) -> LspPosition {
        #[allow(clippy::cast_possible_truncation)]
        LspPosition {
            line: self.line as u32,
            character: self.col as u32,
        }
    }
}

impl From<&Position> for Position {
    fn from(position: &Position) -> Self {
        *position
    }
}

impl From<&LspPosition> for Position {
    fn from(position: &LspPosition) -> Self {
        Self::from_lsp(*position)
    }
}

impl From<LspPosition> for Position {
    fn from(position: LspPosition) -> Self {
        Self::from_lsp(position)
    }
}

impl From<&Position> for LspPosition {
    fn from(position: &Position) -> Self {
        position.into_lsp()
    }
}

impl From<Position> for LspPosition {
    fn from(position: Position) -> Self {
        position.into_lsp()
    }
}

#[cfg(feature = "tree-sitter")]
use tree_sitter::Point as TsPoint;

#[cfg(feature = "tree-sitter")]
impl Position {
    /// Creates a position from a tree-sitter point.
    #[must_use]
    pub const fn from_ts(point: TsPoint) -> Self {
        Self {
            line: point.row,
            col: point.column,
        }
    }

    /// Converts the position into a tree-sitter point.
    #[must_use]
    pub const fn into_ts(self) -> TsPoint {
        TsPoint {
            row: self.line,
            column: self.col,
        }
    }
}

#[cfg(feature = "tree-sitter")]
impl From<TsPoint> for Position {
    fn from(point: TsPoint) -> Self {
        Self::from_ts(point)
    }
}

#[cfg(feature = "tree-sitter")]
impl From<&TsPoint> for Position {
    fn from(point: &TsPoint) -> Self {
        Self::from_ts(*point)
    }
}

#[cfg(feature = "tree-sitter")]
impl From<Position> for TsPoint {
    fn from(position: Position) -> Self {
        position.into_ts()
    }
}

#[cfg(feature = "tree-sitter")]
impl From<&Position> for TsPoint {
    fn from(position: &Position) -> Self {
        position.into_ts()
    }
}
