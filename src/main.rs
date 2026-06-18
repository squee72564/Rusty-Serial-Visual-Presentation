use std::path::PathBuf;

use clap::Parser;
use rsvp::error::OrpError;
use rsvp::input::extract_path;
use rsvp::normalize::normalize;
use rsvp::playback::{DEFAULT_WPM, Playback, validate_wpm};
use rsvp::rsvp::OrpMode;
use rsvp::tokenize::tokenize;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Terminal RSVP reader with Optimal Recognition Point highlighting"
)]
struct Cli {
    /// File to read. MVP supports .txt, .md, and best-effort .pdf.
    path: PathBuf,

    /// Words per minute.
    #[arg(long, default_value_t = DEFAULT_WPM)]
    wpm: u16,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> rsvp::Result<()> {
    let cli = Cli::parse();
    validate_wpm(cli.wpm)?;

    let extracted = extract_path(&cli.path)?;
    let normalized = normalize(&extracted);
    let tokens = tokenize(&normalized);
    if tokens.is_empty() {
        return Err(OrpError::NoReadableText { path: cli.path });
    }

    let playback = Playback::new(tokens, cli.wpm, OrpMode::Spritz)?;
    rsvp::tui::run(playback)
}
