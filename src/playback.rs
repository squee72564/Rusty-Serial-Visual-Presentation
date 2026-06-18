use std::time::Duration;

use crate::error::{OrpError, Result};
use crate::rsvp::{OrpMode, RsvpWord};
use crate::tokenize::Token;

pub const DEFAULT_WPM: u16 = 300;
pub const MIN_WPM: u16 = 100;
pub const MAX_WPM: u16 = 1000;
pub const WPM_STEP: u16 = 25;

#[derive(Debug, Clone)]
pub struct Playback {
    tokens: Vec<Token>,
    words: Vec<RsvpWord>,
    index: usize,
    playing: bool,
    wpm: u16,
    orp_mode: OrpMode,
}

impl Playback {
    pub fn new(tokens: Vec<Token>, wpm: u16, orp_mode: OrpMode) -> Result<Self> {
        validate_wpm(wpm)?;
        let words = crate::rsvp::build_words(&tokens, orp_mode);
        Ok(Self {
            tokens,
            words,
            index: 0,
            playing: false,
            wpm,
            orp_mode,
        })
    }

    pub fn current(&self) -> Option<&RsvpWord> {
        self.words.get(self.index)
    }

    pub fn current_duration(&self) -> Duration {
        self.tokens
            .get(self.index)
            .map(|token| duration_for_token(token, self.tokens.get(self.index + 1), self.wpm))
            .unwrap_or(Duration::from_millis(200))
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn wpm(&self) -> u16 {
        self.wpm
    }

    pub fn orp_mode(&self) -> OrpMode {
        self.orp_mode
    }

    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
    }

    pub fn restart(&mut self) {
        self.index = 0;
        self.playing = true;
    }

    pub fn previous(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    pub fn next(&mut self) {
        if self.index + 1 >= self.words.len() {
            self.playing = false;
            return;
        }
        self.index += 1;
    }

    pub fn tick(&mut self) {
        if self.playing {
            self.next();
        }
    }

    pub fn increase_wpm(&mut self) {
        self.set_wpm(self.wpm.saturating_add(WPM_STEP).min(MAX_WPM));
    }

    pub fn decrease_wpm(&mut self) {
        self.set_wpm(self.wpm.saturating_sub(WPM_STEP).max(MIN_WPM));
    }

    pub fn cycle_orp_mode(&mut self) {
        self.orp_mode = self.orp_mode.next();
        self.rebuild_words();
    }

    fn set_wpm(&mut self, wpm: u16) {
        self.wpm = wpm.clamp(MIN_WPM, MAX_WPM);
    }

    fn rebuild_words(&mut self) {
        self.words = crate::rsvp::build_words(&self.tokens, self.orp_mode);
    }
}

pub fn validate_wpm(wpm: u16) -> Result<()> {
    if (MIN_WPM..=MAX_WPM).contains(&wpm) {
        Ok(())
    } else {
        Err(OrpError::InvalidWpm {
            actual: wpm,
            min: MIN_WPM,
            max: MAX_WPM,
        })
    }
}

pub fn duration_for_wpm(wpm: u16) -> Duration {
    Duration::from_millis(60_000 / u64::from(wpm.max(1)))
}

pub fn duration_for_token(token: &Token, next_token: Option<&Token>, wpm: u16) -> Duration {
    let base_ms = 60_000 / u64::from(wpm.max(1));
    let (numerator, denominator) = timing_ratio(token, next_token);

    Duration::from_millis(base_ms * numerator / denominator)
}

fn timing_ratio(token: &Token, next_token: Option<&Token>) -> (u64, u64) {
    if next_token.is_some_and(|next| next.paragraph_index > token.paragraph_index) {
        return (5, 2);
    }

    match trailing_reading_punctuation(&token.text) {
        Some('.') | Some('?') | Some('!') => (2, 1),
        Some(',') | Some(';') | Some(':') => (3, 2),
        _ => (1, 1),
    }
}

fn trailing_reading_punctuation(text: &str) -> Option<char> {
    text.chars()
        .rev()
        .find(|ch| !matches!(ch, '"' | '\'' | ')' | ']' | '}' | '”' | '’'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> Vec<Token> {
        ["one", "two", "three"]
            .into_iter()
            .map(|text| Token {
                text: text.into(),
                paragraph_index: 0,
            })
            .collect()
    }

    #[test]
    fn fixed_timing_uses_wpm() {
        assert_eq!(duration_for_wpm(300), Duration::from_millis(200));
    }

    #[test]
    fn token_timing_adds_punctuation_pauses() {
        let plain = Token {
            text: "word".into(),
            paragraph_index: 0,
        };
        let comma = Token {
            text: "word,".into(),
            paragraph_index: 0,
        };
        let period = Token {
            text: "word.\"".into(),
            paragraph_index: 0,
        };

        assert_eq!(
            duration_for_token(&plain, None, 300),
            Duration::from_millis(200)
        );
        assert_eq!(
            duration_for_token(&comma, None, 300),
            Duration::from_millis(300)
        );
        assert_eq!(
            duration_for_token(&period, None, 300),
            Duration::from_millis(400)
        );
    }

    #[test]
    fn token_timing_adds_paragraph_pause_after_token() {
        let token = Token {
            text: "word.".into(),
            paragraph_index: 0,
        };
        let next = Token {
            text: "Next".into(),
            paragraph_index: 1,
        };

        assert_eq!(
            duration_for_token(&token, Some(&next), 300),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn playback_state_handles_controls_and_bounds() {
        let mut playback = Playback::new(tokens(), DEFAULT_WPM, OrpMode::Spritz).unwrap();

        assert!(!playback.is_playing());
        playback.toggle_playing();
        playback.tick();
        assert_eq!(playback.index(), 1);
        playback.previous();
        assert_eq!(playback.index(), 0);
        playback.decrease_wpm();
        assert_eq!(playback.wpm(), 275);
        for _ in 0..40 {
            playback.increase_wpm();
        }
        assert_eq!(playback.wpm(), MAX_WPM);
        playback.cycle_orp_mode();
        assert_eq!(playback.orp_mode(), OrpMode::Center);
        playback.restart();
        assert_eq!(playback.index(), 0);
    }

    #[test]
    fn wpm_changes_apply_immediately() {
        let mut playback = Playback::new(tokens(), DEFAULT_WPM, OrpMode::Spritz).unwrap();

        assert_eq!(playback.current_duration(), duration_for_wpm(DEFAULT_WPM));

        playback.increase_wpm();

        assert_eq!(
            playback.current_duration(),
            duration_for_wpm(DEFAULT_WPM + WPM_STEP)
        );
    }

    #[test]
    fn playback_pauses_at_end() {
        let mut playback = Playback::new(tokens(), DEFAULT_WPM, OrpMode::Spritz).unwrap();

        playback.toggle_playing();
        playback.tick();
        playback.tick();
        playback.tick();

        assert_eq!(playback.index(), 2);
        assert!(!playback.is_playing());
    }
}
