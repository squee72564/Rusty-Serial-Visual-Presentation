use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use crate::error::{OrpError, Result};
use crate::rsvp::{OrpMode, RsvpWord};
use crate::stream::{StreamChunk, StreamEvent, StreamHandle, spawn_stream};
use crate::tokenize::Token;

pub const DEFAULT_WPM: u16 = 300;
pub const MIN_WPM: u16 = 100;
pub const MAX_WPM: u16 = 1000;
pub const WPM_STEP: u16 = 25;

#[derive(Debug)]
pub struct Playback {
    prev: Option<StreamChunk>,
    current: StreamChunk,
    next: Option<StreamChunk>,
    receiver: Receiver<StreamEvent>,
    local_index: usize,
    playing: bool,
    wpm: u16,
    orp_mode: OrpMode,
    stream_done: bool,
    loading: bool,
    error: Option<String>,
    path: std::path::PathBuf,
    timings_enabled: bool,
}

impl Playback {
    pub fn from_stream(stream: StreamHandle, wpm: u16, orp_mode: OrpMode) -> Result<Self> {
        validate_wpm(wpm)?;

        let source_path = stream.path().to_path_buf();
        let timings_enabled = stream.timings_enabled();
        let receiver = stream.receiver();
        let current = first_chunk(&receiver)?;
        let stream_done = current.eof;

        let mut playback = Self {
            prev: None,
            current,
            next: None,
            receiver,
            local_index: 0,
            playing: false,
            wpm,
            orp_mode,
            stream_done,
            loading: false,
            error: None,
            path: source_path,
            timings_enabled,
        };
        playback.fill_next_nonblocking();
        Ok(playback)
    }

    pub fn current(&self) -> Option<RsvpWord> {
        self.current_token()
            .map(|token| RsvpWord::from_token(token, self.orp_mode))
    }

    pub fn current_duration(&self) -> Duration {
        self.current_token()
            .map(|token| duration_for_token(token, self.next_token(), self.wpm))
            .unwrap_or(Duration::from_millis(200))
    }

    pub fn index(&self) -> usize {
        self.current.start_word_index + self.local_index
    }

    pub fn is_empty(&self) -> bool {
        self.current.tokens.is_empty()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn stream_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn is_at_end(&self) -> bool {
        self.stream_done && self.next.is_none() && self.local_index + 1 >= self.current.tokens.len()
    }

    pub fn wpm(&self) -> u16 {
        self.wpm
    }

    pub fn orp_mode(&self) -> OrpMode {
        self.orp_mode
    }

    pub fn poll_stream(&mut self) {
        if self.next.is_none() {
            self.fill_next_nonblocking();
        }
    }

    pub fn toggle_playing(&mut self) {
        if self.error.is_some() || self.is_at_end() {
            self.playing = false;
            return;
        }
        self.playing = !self.playing;
        if self.playing {
            self.loading = false;
        }
    }

    pub fn restart(&mut self) {
        let stream = spawn_stream(self.path.clone(), self.timings_enabled);
        self.receiver = stream.receiver();
        self.prev = None;
        self.next = None;
        self.local_index = 0;
        self.playing = false;
        self.stream_done = false;
        self.loading = true;
        self.error = None;

        match first_chunk(&self.receiver) {
            Ok(chunk) => {
                self.current = chunk;
                self.loading = false;
                self.playing = true;
                self.fill_next_nonblocking();
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.playing = false;
            }
        }
    }

    pub fn previous(&mut self) {
        self.loading = false;
        if self.local_index > 0 {
            self.local_index -= 1;
            return;
        }

        if let Some(prev) = self.prev.take() {
            let old_current = std::mem::replace(&mut self.current, prev);
            self.next = Some(old_current);
            self.local_index = self.current.tokens.len().saturating_sub(1);
        }
    }

    pub fn next(&mut self) {
        if self.error.is_some() {
            self.playing = false;
            return;
        }

        if self.local_index + 1 < self.current.tokens.len() {
            self.local_index += 1;
            self.loading = false;
            return;
        }

        if let Some(next) = self.next.take() {
            let old_current = std::mem::replace(&mut self.current, next);
            self.prev = Some(old_current);
            self.local_index = 0;
            self.loading = false;
            self.fill_next_nonblocking();
            return;
        }

        self.fill_next_nonblocking();
        if let Some(next) = self.next.take() {
            let old_current = std::mem::replace(&mut self.current, next);
            self.prev = Some(old_current);
            self.local_index = 0;
            self.loading = false;
            self.fill_next_nonblocking();
        } else if self.current.eof || self.stream_done {
            self.playing = false;
            self.loading = false;
        } else {
            self.playing = false;
            self.loading = true;
        }
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
    }

    fn set_wpm(&mut self, wpm: u16) {
        self.wpm = wpm.clamp(MIN_WPM, MAX_WPM);
    }

    fn current_token(&self) -> Option<&Token> {
        self.current.tokens.get(self.local_index)
    }

    fn next_token(&self) -> Option<&Token> {
        self.current
            .tokens
            .get(self.local_index + 1)
            .or_else(|| self.next.as_ref().and_then(|chunk| chunk.tokens.first()))
    }

    fn fill_next_nonblocking(&mut self) {
        if self.next.is_some() || self.stream_done || self.error.is_some() {
            return;
        }

        loop {
            match self.receiver.try_recv() {
                Ok(StreamEvent::Chunk(chunk)) => {
                    self.loading = false;
                    if chunk.eof {
                        self.stream_done = true;
                    }
                    self.next = Some(chunk);
                    return;
                }
                Ok(StreamEvent::Failed(error)) => {
                    self.error = Some(error.to_string());
                    self.playing = false;
                    self.loading = false;
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.error = Some(OrpError::StreamDisconnected.to_string());
                    self.playing = false;
                    self.loading = false;
                    return;
                }
            }
        }
    }
}

fn first_chunk(receiver: &Receiver<StreamEvent>) -> Result<StreamChunk> {
    loop {
        match receiver.recv().map_err(|_| OrpError::StreamDisconnected)? {
            StreamEvent::Chunk(chunk) => return Ok(chunk),
            StreamEvent::Failed(error) => return Err(error),
        }
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
    use std::sync::mpsc::sync_channel;

    fn tokens() -> Vec<Token> {
        ["one", "two", "three"]
            .into_iter()
            .map(|text| Token {
                text: text.into(),
                paragraph_index: 0,
            })
            .collect()
    }

    fn token(text: &str, paragraph_index: usize) -> Token {
        Token {
            text: text.into(),
            paragraph_index,
        }
    }

    fn chunk(id: usize, start_word_index: usize, tokens: Vec<Token>, eof: bool) -> StreamChunk {
        StreamChunk {
            id,
            start_word_index,
            tokens,
            eof,
        }
    }

    fn streaming_playback(current: StreamChunk, receiver: Receiver<StreamEvent>) -> Playback {
        Playback {
            prev: None,
            current,
            next: None,
            receiver,
            local_index: 0,
            playing: false,
            wpm: DEFAULT_WPM,
            orp_mode: OrpMode::Spritz,
            stream_done: false,
            loading: false,
            error: None,
            path: std::path::PathBuf::from("<test>"),
            timings_enabled: false,
        }
    }

    fn closed_playback(tokens: Vec<Token>) -> Playback {
        let (_sender, receiver) = sync_channel(2);
        let mut playback = streaming_playback(chunk(0, 0, tokens, true), receiver);
        playback.stream_done = true;
        playback
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
        let mut playback = closed_playback(tokens());

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
    }

    #[test]
    fn wpm_changes_apply_immediately() {
        let mut playback = closed_playback(tokens());

        assert_eq!(playback.current_duration(), duration_for_wpm(DEFAULT_WPM));

        playback.increase_wpm();

        assert_eq!(
            playback.current_duration(),
            duration_for_wpm(DEFAULT_WPM + WPM_STEP)
        );
    }

    #[test]
    fn playback_pauses_at_end() {
        let mut playback = closed_playback(tokens());

        playback.toggle_playing();
        playback.tick();
        playback.tick();
        playback.tick();

        assert_eq!(playback.index(), 2);
        assert!(!playback.is_playing());
    }

    #[test]
    fn streaming_playback_rotates_chunks_and_steps_back() {
        let (sender, receiver) = sync_channel(2);
        sender
            .send(StreamEvent::Chunk(chunk(1, 1, vec![token("two", 0)], true)))
            .unwrap();
        let mut playback = streaming_playback(chunk(0, 0, vec![token("one", 0)], false), receiver);

        playback.poll_stream();
        playback.next();

        assert_eq!(playback.index(), 1);

        playback.previous();

        assert_eq!(playback.index(), 0);
    }

    #[test]
    fn streaming_playback_pauses_at_unloaded_boundary() {
        let (_sender, receiver) = sync_channel(2);
        let mut playback = streaming_playback(chunk(0, 0, vec![token("one", 0)], false), receiver);
        playback.toggle_playing();

        playback.next();

        assert!(playback.is_loading());
        assert!(!playback.is_playing());
        assert_eq!(playback.index(), 0);
    }

    #[test]
    fn timing_uses_loaded_next_chunk_for_paragraph_pause() {
        let (sender, receiver) = sync_channel(2);
        sender
            .send(StreamEvent::Chunk(chunk(
                1,
                1,
                vec![token("next", 1)],
                true,
            )))
            .unwrap();
        let mut playback = streaming_playback(chunk(0, 0, vec![token("end.", 0)], false), receiver);

        playback.poll_stream();

        assert_eq!(playback.current_duration(), Duration::from_millis(500));
    }
}
