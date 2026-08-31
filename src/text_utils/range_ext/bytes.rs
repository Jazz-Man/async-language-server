use crate::error::RangeError;

use super::{check_delimiter, check_text_length};

type ByteRange = std::ops::Range<usize>;
type BytePosition = usize;

impl super::RangeExt for ByteRange {
    type Position = BytePosition;

    fn split_at(self, _text: &str, at: Self::Position) -> Result<(Self, Self), RangeError> {
        if at > self.end - self.start {
            return Err(RangeError::PositionOutOfRange);
        }
        Ok((self.start..(self.start + at), (self.start + at)..self.end))
    }

    fn shrink(self, amount_left: usize, amount_right: usize) -> Result<Self, RangeError> {
        // Byte ranges have no line concept, so shrinking cannot fail.
        let new_start = self.start.saturating_add(amount_left).min(self.end);
        let new_end = self.end.saturating_sub(amount_right).max(self.start);
        Ok(new_start..new_end)
    }

    fn sub(
        self,
        _text: &str,
        from: Self::Position,
        to: Self::Position,
    ) -> Result<Self, RangeError> {
        let len = self.end - self.start;
        if from > len || to > len {
            return Err(RangeError::PositionOutOfRange);
        }
        if from > to {
            return Err(RangeError::StartAfterEnd);
        }
        Ok((self.start + from)..(self.start + to))
    }

    fn sub_delimited(
        self,
        text: &str,
        delim: char,
    ) -> Result<(Option<Self>, Option<Self>), RangeError> {
        check_text_length(text.len(), self.end - self.start)?;
        check_delimiter(delim)?;

        if let Some(offset) = text.find(delim) {
            Ok((
                if offset == 0 {
                    None // delimiter is the first character
                } else {
                    Some(self.clone().split_off_left(text, offset)?)
                },
                if offset + 1 >= text.len() {
                    None // delimiter is the last character
                } else {
                    Some(self.clone().split_off_right(text, offset + 1)?)
                },
            ))
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

        check_text_length(text.len(), self.end - self.start)?;

        let Some(delim0_offset) = text.find(delim0) else {
            return Ok((Some(self), None, None));
        };

        let (first, remainder) = self.clone().sub_delimited(text, delim0)?;
        let Some(remainder) = remainder else {
            return Ok((first, None, None));
        };

        let remainder_text = &text[delim0_offset + 1..];

        let (second, third) = remainder.sub_delimited(remainder_text, delim1)?;
        Ok((first, second, third))
    }
}
