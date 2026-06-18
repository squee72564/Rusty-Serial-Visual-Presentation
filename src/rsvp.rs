use std::fmt;
use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::playback::duration_for_token;
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

    pub fn strategy(self) -> Box<dyn OrpStrategy> {
        match self {
            Self::Spritz => Box::new(SpritzOrp),
            Self::Center => Box::new(CenterOrp),
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

pub trait OrpStrategy {
    fn pivot_index(&self, graphemes: &[&str]) -> usize;
}

#[derive(Debug)]
pub struct SpritzOrp;

impl OrpStrategy for SpritzOrp {
    fn pivot_index(&self, graphemes: &[&str]) -> usize {
        match graphemes.len() {
            0..=1 => 0,
            2..=5 => 1,
            6..=9 => 2,
            10..=13 => 3,
            _ => 4,
        }
        .min(graphemes.len().saturating_sub(1))
    }
}

#[derive(Debug)]
pub struct CenterOrp;

impl OrpStrategy for CenterOrp {
    fn pivot_index(&self, graphemes: &[&str]) -> usize {
        graphemes.len().saturating_sub(1) / 2
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsvpWord {
    pub text: String,
    pub pivot_index: usize,
    pub duration: Duration,
    pub prefix_width: usize,
    pub pivot_width: usize,
    pub suffix_width: usize,
}

impl RsvpWord {
    pub fn from_token(token: &Token, mode: OrpMode, wpm: u16) -> Self {
        Self::from_token_with_next(token, None, mode, wpm)
    }

    pub fn from_token_with_next(
        token: &Token,
        next_token: Option<&Token>,
        mode: OrpMode,
        wpm: u16,
    ) -> Self {
        let graphemes = token.text.graphemes(true).collect::<Vec<_>>();
        let strategy = mode.strategy();
        let pivot_index = strategy.pivot_index(&graphemes);
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
            duration: duration_for_token(token, next_token, wpm),
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

pub fn build_words(tokens: &[Token], mode: OrpMode, wpm: u16) -> Vec<RsvpWord> {
    tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            RsvpWord::from_token_with_next(token, tokens.get(index + 1), mode, wpm)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spritz_and_center_return_stable_grapheme_indexes() {
        let graphemes = "reading".graphemes(true).collect::<Vec<_>>();

        assert_eq!(SpritzOrp.pivot_index(&graphemes), 2);
        assert_eq!(CenterOrp.pivot_index(&graphemes), 3);
    }

    #[test]
    fn rsvp_word_splits_pivot_without_byte_indexing() {
        let token = Token {
            text: "cafe\u{301}".into(),
            paragraph_index: 0,
        };
        let word = RsvpWord::from_token(&token, OrpMode::Center, 300);

        let (_, pivot, _) = word.pivot_parts();
        assert!(!pivot.is_empty());
    }
}
