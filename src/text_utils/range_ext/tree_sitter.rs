use tree_sitter::{Point as TsPosition, Range as TsRange};

use crate::error::RangeError;

use super::{check_delimiter, check_text_length};

impl super::RangeExt for TsRange {
    type Position = TsPosition;

    fn split_at(self, text: &str, at: Self::Position) -> Result<(Self, Self), RangeError> {
        check_text_length(text.len(), self.end_byte - self.start_byte)?;

        let at_absolute = TsPosition {
            row: self.start_point.row + at.row,
            column: if at.row == 0 {
                self.start_point.column + at.column
            } else {
                at.column
            },
        };

        // Find byte offset for the relative position
        let mut current_row = 0;
        let mut current_col = 0;
        let mut at_byte = self.start_byte;
        let mut found = false;

        for (i, ch) in text.char_indices() {
            if current_row == at.row && current_col == at.column {
                at_byte = self.start_byte + i;
                found = true;
                break;
            }
            if ch == '\n' {
                current_row += 1;
                current_col = 0;
            } else {
                current_col += ch.len_utf8();
            }
        }

        // Handle end-of-text case if position wasn't found in loop;
        // a position that is nowhere in the text is out of range.
        if !found {
            if current_row == at.row && current_col == at.column {
                at_byte = self.end_byte;
            } else {
                return Err(RangeError::PositionOutOfRange);
            }
        }

        let left = TsRange {
            start_byte: self.start_byte,
            end_byte: at_byte,
            start_point: self.start_point,
            end_point: at_absolute,
        };
        let right = TsRange {
            start_byte: at_byte,
            end_byte: self.end_byte,
            start_point: at_absolute,
            end_point: self.end_point,
        };

        Ok((left, right))
    }

    fn shrink(self, amount_left: usize, amount_right: usize) -> Result<Self, RangeError> {
        if self.start_point.row != self.end_point.row {
            return Err(RangeError::NotSingleLine);
        }

        let start_col = self
            .start_point
            .column
            .saturating_add(amount_left)
            .min(self.end_point.column);
        let end_col = self
            .end_point
            .column
            .saturating_sub(amount_right)
            .max(self.start_point.column);
        let start_byte = self
            .start_byte
            .saturating_add(amount_left)
            .min(self.end_byte);
        let end_byte = self
            .end_byte
            .saturating_sub(amount_right)
            .max(self.start_byte);

        Ok(TsRange {
            start_byte,
            end_byte,
            start_point: TsPosition {
                row: self.start_point.row,
                column: start_col,
            },
            end_point: TsPosition {
                row: self.end_point.row,
                column: end_col,
            },
        })
    }

    fn sub(self, text: &str, from: Self::Position, to: Self::Position) -> Result<Self, RangeError> {
        if from > to {
            return Err(RangeError::StartAfterEnd);
        }

        check_text_length(text.len(), self.end_byte - self.start_byte)?;

        let from_absolute = TsPosition {
            row: self.start_point.row + from.row,
            column: if from.row == 0 {
                self.start_point.column + from.column
            } else {
                from.column
            },
        };

        let to_absolute = TsPosition {
            row: self.start_point.row + to.row,
            column: if to.row == 0 {
                self.start_point.column + to.column
            } else {
                to.column
            },
        };

        // Find byte offsets for both positions
        let mut current_row = 0;
        let mut current_col = 0;
        let mut from_byte = self.start_byte;
        let mut to_byte = self.start_byte;
        let mut found_from = false;
        let mut found_to = false;

        for (i, ch) in text.char_indices() {
            if !found_from && current_row == from.row && current_col == from.column {
                from_byte = self.start_byte + i;
                found_from = true;
            }
            if !found_to && current_row == to.row && current_col == to.column {
                to_byte = self.start_byte + i;
                found_to = true;
            }
            if found_from && found_to {
                break;
            }
            if ch == '\n' {
                current_row += 1;
                current_col = 0;
            } else {
                current_col += ch.len_utf8();
            }
        }

        // Handle end-of-text cases for positions not found in loop;
        // a position that is nowhere in the text is out of range.
        if !found_from {
            if current_row == from.row && current_col == from.column {
                from_byte = self.end_byte;
            } else {
                return Err(RangeError::PositionOutOfRange);
            }
        }
        if !found_to {
            if current_row == to.row && current_col == to.column {
                to_byte = self.end_byte;
            } else {
                return Err(RangeError::PositionOutOfRange);
            }
        }

        Ok(TsRange {
            start_byte: from_byte,
            end_byte: to_byte,
            start_point: from_absolute,
            end_point: to_absolute,
        })
    }

    fn sub_delimited(
        self,
        text: &str,
        delim: char,
    ) -> Result<(Option<Self>, Option<Self>), RangeError> {
        check_text_length(text.len(), self.end_byte - self.start_byte)?;
        check_delimiter(delim)?;

        if let Some(offset) = text.find(delim) {
            // Find point position of delimiter
            let mut row_offset = 0;
            let mut current_line_start = 0;

            for (i, ch) in text.char_indices() {
                if i >= offset {
                    break;
                }
                if ch == '\n' {
                    row_offset += 1;
                    current_line_start = i + 1;
                }
            }

            let col_offset = offset - current_line_start;
            let delim_point = TsPosition {
                row: self.start_point.row + row_offset,
                column: if row_offset == 0 {
                    self.start_point.column + col_offset
                } else {
                    col_offset
                },
            };

            let delim_byte = self.start_byte + offset;

            let left = if offset == 0 {
                None // delimiter is the first character
            } else {
                Some(TsRange {
                    start_byte: self.start_byte,
                    end_byte: delim_byte,
                    start_point: self.start_point,
                    end_point: delim_point,
                })
            };

            let right = if offset + 1 >= text.len() {
                None // delimiter is the last character
            } else {
                let after_delim_point = if text[offset..].starts_with('\n') {
                    TsPosition {
                        row: delim_point.row + 1,
                        column: 0,
                    }
                } else {
                    TsPosition {
                        row: delim_point.row,
                        column: delim_point.column + 1,
                    }
                };

                Some(TsRange {
                    start_byte: delim_byte + 1,
                    end_byte: self.end_byte,
                    start_point: after_delim_point,
                    end_point: self.end_point,
                })
            };

            Ok((left, right))
        } else if !text.is_empty() {
            Ok((Some(self), None))
        } else {
            Ok((None, None))
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

        check_text_length(text.len(), self.end_byte - self.start_byte)?;

        let (first, remainder) = self.sub_delimited(text, delim0)?;

        if let Some(remainder) = remainder {
            let remainder_start = remainder.start_byte - self.start_byte;
            let remainder_text = &text[remainder_start..];

            let (second, third) = remainder.sub_delimited(remainder_text, delim1)?;
            Ok((first, second, third))
        } else {
            Ok((first, None, None))
        }
    }
}
