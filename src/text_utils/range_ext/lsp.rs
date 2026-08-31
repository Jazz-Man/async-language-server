use async_lsp::lsp_types::{Position as LspPosition, Range as LspRange};

use crate::error::RangeError;

use super::check_delimiter;

impl super::RangeExt for LspRange {
    type Position = LspPosition;

    fn split_at(self, _text: &str, at: Self::Position) -> Result<(Self, Self), RangeError> {
        let at_absolute = LspPosition {
            line: self.start.line + at.line,
            character: if at.line == 0 {
                self.start.character + at.character
            } else {
                at.character
            },
        };

        if !(at_absolute >= self.start && at_absolute <= self.end) {
            return Err(RangeError::PositionOutOfRange);
        }

        let left = LspRange {
            start: self.start,
            end: at_absolute,
        };
        let right = LspRange {
            start: at_absolute,
            end: self.end,
        };

        Ok((left, right))
    }

    fn shrink(self, amount_left: usize, amount_right: usize) -> Result<Self, RangeError> {
        if self.start.line != self.end.line {
            return Err(RangeError::NotSingleLine);
        }

        let start_char = self
            .start
            .character
            .saturating_add(u32::try_from(amount_left).unwrap_or(u32::MAX))
            .min(self.end.character);
        let end_char = self
            .end
            .character
            .saturating_sub(u32::try_from(amount_right).unwrap_or(u32::MAX))
            .max(self.start.character);

        Ok(LspRange {
            start: LspPosition {
                line: self.start.line,
                character: start_char,
            },
            end: LspPosition {
                line: self.end.line,
                character: end_char,
            },
        })
    }

    fn sub(
        self,
        _text: &str,
        from: Self::Position,
        to: Self::Position,
    ) -> Result<Self, RangeError> {
        if from > to {
            return Err(RangeError::StartAfterEnd);
        }

        let from_absolute = LspPosition {
            line: self.start.line + from.line,
            character: if from.line == 0 {
                self.start.character + from.character
            } else {
                from.character
            },
        };

        let to_absolute = LspPosition {
            line: self.start.line + to.line,
            character: if to.line == 0 {
                self.start.character + to.character
            } else {
                to.character
            },
        };

        // sanity check
        if !(from_absolute >= self.start && from_absolute <= self.end) {
            return Err(RangeError::PositionOutOfRange);
        }
        if !(to_absolute >= self.start && to_absolute <= self.end) {
            return Err(RangeError::PositionOutOfRange);
        }

        Ok(LspRange {
            start: from_absolute,
            end: to_absolute,
        })
    }

    fn sub_delimited(
        self,
        text: &str,
        delim: char,
    ) -> Result<(Option<Self>, Option<Self>), RangeError> {
        check_delimiter(delim)?;

        if text.is_empty() {
            return Ok((None, None));
        }

        if let Some(offset) = text.find(delim) {
            // Find relative position of delimiter from start
            let mut line_num = 0u32;
            let mut line_byte = 0;
            for (i, ch) in text.char_indices() {
                if i >= offset {
                    break;
                }
                if ch == '\n' {
                    line_num += 1;
                    line_byte = i + 1;
                }
            }

            let character =
                u32::try_from(text[line_byte..offset].chars().count()).unwrap_or(u32::MAX);
            let delim_pos = LspPosition {
                line: line_num,
                character,
            };

            let left = if offset == 0 {
                None // delimiter is the first character
            } else {
                Some(self.split_off_left(text, delim_pos)?)
            };

            let right = if offset + 1 >= text.len() {
                None // delimiter is the last character
            } else {
                let after_delim_pos = if text[offset..].starts_with('\n') {
                    LspPosition {
                        line: line_num + 1,
                        character: 0,
                    }
                } else {
                    LspPosition {
                        line: line_num,
                        character: character + 1,
                    }
                };
                Some(self.split_off_right(text, after_delim_pos)?)
            };

            Ok((left, right))
        } else {
            Ok((Some(self), None))
        }
    }

    fn sub_delimited_tri(
        self,
        text: &str,
        delim0: char,
        delim1: char,
    ) -> Result<(Option<Self>, Option<Self>, Option<Self>), RangeError> {
        check_delimiter(delim0)?;
        check_delimiter(delim1)?;

        if text.is_empty() {
            return Ok((None, None, None));
        }

        let Some(delim0_offset) = text.find(delim0) else {
            return Ok((Some(self), None, None));
        };

        let (first, remainder) = self.sub_delimited(text, delim0)?;
        let Some(remainder) = remainder else {
            return Ok((first, None, None));
        };

        let remainder_text = &text[delim0_offset + 1..];
        let (second, third) = remainder.sub_delimited(remainder_text, delim1)?;
        Ok((first, second, third))
    }
}
