use std::io::{self, Stdout};
use std::time::Instant;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::error::Result;
use crate::playback::Playback;

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn run(playback: Playback) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = ReaderApp::new(playback).run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<TuiTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut TuiTerminal) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

struct ReaderApp {
    playback: Playback,
    last_tick: Instant,
}

impl ReaderApp {
    fn new(playback: Playback) -> Self {
        Self {
            playback,
            last_tick: Instant::now(),
        }
    }

    fn run(&mut self, terminal: &mut TuiTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            let timeout = self
                .playback
                .current_duration()
                .saturating_sub(self.last_tick.elapsed());

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char(' ') => {
                            let was_playing = self.playback.is_playing();
                            self.playback.toggle_playing();
                            if !was_playing && self.playback.is_playing() {
                                self.last_tick = Instant::now();
                            }
                        }
                        KeyCode::Left => {
                            self.playback.previous();
                            self.last_tick = Instant::now();
                        }
                        KeyCode::Right => {
                            self.playback.next();
                            self.last_tick = Instant::now();
                        }
                        KeyCode::Up => {
                            self.playback.increase_wpm();
                            self.last_tick = Instant::now();
                        }
                        KeyCode::Down => {
                            self.playback.decrease_wpm();
                            self.last_tick = Instant::now();
                        }
                        KeyCode::Char('o') => self.playback.cycle_orp_mode(),
                        KeyCode::Char('r') => {
                            self.playback.restart();
                            self.last_tick = Instant::now();
                        }
                        _ => {}
                    }
                }
            }

            if self.last_tick.elapsed() >= self.playback.current_duration() {
                self.playback.tick();
                self.last_tick = Instant::now();
            }
        }
    }

    fn render(&self, frame: &mut ratatui::Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(frame.area());

        let reader_area = centered_line_area(chunks[0]);
        let word_line = self.word_line(reader_area.width);
        let reader = Paragraph::new(word_line)
            .alignment(Alignment::Left)
            .block(Block::default().borders(Borders::NONE));
        frame.render_widget(reader, reader_area);

        let progress = if self.playback.is_empty() {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.playback.index() + 1, self.playback.len())
        };
        let state = if self.playback.is_playing() {
            "playing"
        } else {
            "paused"
        };
        let status = format!(
            "{} | {} WPM | {} | {} | Space pause  arrows nav/speed  o ORP  r restart  q quit",
            progress,
            self.playback.wpm(),
            self.playback.orp_mode(),
            state
        );
        let status = Paragraph::new(status)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(status, chunks[1]);
    }

    fn word_line(&self, viewport_width: u16) -> Line<'static> {
        let Some(word) = self.playback.current() else {
            return Line::from("No readable text");
        };
        let (prefix, pivot, suffix) = word.pivot_parts();
        let padding = anchor_padding(viewport_width, prefix.width());
        let mut spans = Vec::new();

        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(
            prefix.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            pivot.to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            suffix.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

        Line::from(spans)
    }
}

fn centered_line_area(area: Rect) -> Rect {
    Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1) / 2,
        width: area.width,
        height: 1,
    }
}

fn anchor_padding(viewport_width: u16, prefix_width: usize) -> usize {
    let anchor_column = usize::from(viewport_width) / 2;
    anchor_column.saturating_sub(prefix_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_line_area_uses_vertical_middle_row() {
        let area = Rect {
            x: 2,
            y: 3,
            width: 80,
            height: 21,
        };

        assert_eq!(
            centered_line_area(area),
            Rect {
                x: 2,
                y: 13,
                width: 80,
                height: 1
            }
        );
    }

    #[test]
    fn anchor_padding_keeps_pivot_column_fixed() {
        let viewport_width = 100;

        assert_eq!(anchor_padding(viewport_width, 0), 50);
        assert_eq!(anchor_padding(viewport_width, 7), 43);
        assert_eq!(anchor_padding(viewport_width, 50), 0);
    }
}
