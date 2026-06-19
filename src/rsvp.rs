use std::fmt;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::tokenize::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrpMode {
    Spritz,
    Center,
}

impl OrpMode {
    pub fn next(self) -> Self {
        match self {
            Self::Spritz => Self::Center,
            Self::Center => Self::Spritz,
        }
    }

    fn pivot_index(self, graphemes: &[&str]) -> usize {
        match self {
            Self::Spritz => match graphemes.len() {
                0..=1 => 0,
                2..=5 => 1,
                6..=9 => 2,
                10..=13 => 3,
                _ => 4,
            }
            .min(graphemes.len().saturating_sub(1)),
            Self::Center => graphemes.len().saturating_sub(1) / 2,
        }
    }
}

impl fmt::Display for OrpMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spritz => f.write_str("Spritz"),
            Self::Center => f.write_str("Center"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsvpWord {
    pub text: String,
    pub pivot_index: usize,
    pub prefix_width: usize,
    pub pivot_width: usize,
    pub suffix_width: usize,
}

impl RsvpWord {
    pub fn from_token(token: &Token, mode: OrpMode) -> Self {
        let graphemes = token.text.graphemes(true).collect::<Vec<_>>();
        let pivot_index = mode.pivot_index(&graphemes);
        let prefix_width = graphemes[..pivot_index].iter().map(|g| g.width()).sum();
        let pivot_width = graphemes
            .get(pivot_index)
            .map(|grapheme| grapheme.width())
            .unwrap_or(0);
        let suffix_width = graphemes[pivot_index.saturating_add(1)..]
            .iter()
            .map(|g| g.width())
            .sum();

        Self {
            text: token.text.clone(),
            pivot_index,
            prefix_width,
            pivot_width,
            suffix_width,
        }
    }

    pub fn pivot_parts(&self) -> (&str, &str, &str) {
        let mut start = 0;
        let mut end = self.text.len();

        for (idx, (byte_idx, grapheme)) in self.text.grapheme_indices(true).enumerate() {
            if idx == self.pivot_index {
                start = byte_idx;
                end = byte_idx + grapheme.len();
                break;
            }
        }

        (
            &self.text[..start],
            &self.text[start..end],
            &self.text[end..],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spritz_and_center_return_stable_grapheme_indexes() {
        let graphemes = "reading".graphemes(true).collect::<Vec<_>>();

        assert_eq!(OrpMode::Spritz.pivot_index(&graphemes), 2);
        assert_eq!(OrpMode::Center.pivot_index(&graphemes), 3);
    }

    #[test]
    fn rsvp_word_splits_pivot_without_byte_indexing() {
        let token = Token {
            text: "cafe\u{301}".into(),
            paragraph_index: 0,
        };
        let word = RsvpWord::from_token(&token, OrpMode::Center);

        let (_, pivot, _) = word.pivot_parts();
        assert!(!pivot.is_empty());
    }
}
