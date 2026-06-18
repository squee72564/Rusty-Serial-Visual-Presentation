use std::path::PathBuf;

use clap::Parser;
use orp_reader::error::OrpError;
use orp_reader::input::extract_path;
use orp_reader::normalize::normalize;
use orp_reader::playback::{DEFAULT_WPM, Playback, validate_wpm};
use orp_reader::rsvp::OrpMode;
use orp_reader::tokenize::tokenize;

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

fn run() -> orp_reader::Result<()> {
    let cli = Cli::parse();
    validate_wpm(cli.wpm)?;

    let extracted = extract_path(&cli.path)?;
    let normalized = normalize(&extracted);
    let tokens = tokenize(&normalized);
    if tokens.is_empty() {
        return Err(OrpError::NoReadableText { path: cli.path });
    }

    let playback = Playback::new(tokens, cli.wpm, OrpMode::Spritz)?;
    orp_reader::tui::run(playback)
}
