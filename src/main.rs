use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Parser;
use rsvp::playback::{DEFAULT_WPM, Playback, validate_wpm};
use rsvp::rsvp::OrpMode;
use rsvp::stream::{StreamTimings, format_timings, spawn_stream};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Terminal RSVP reader with Optimal Recognition Point highlighting"
)]
struct Cli {
    /// File to read. Supports .txt, .md, best-effort .pdf, and best-effort .epub.
    path: PathBuf,

    /// Words per minute.
    #[arg(long, default_value_t = DEFAULT_WPM)]
    wpm: u16,

    /// Print extraction and chunking timings after exit.
    #[arg(long)]
    timings: bool,
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

    print_loading_splash(&cli.path);

    let stream = spawn_stream(cli.path.clone(), cli.timings);
    let timings = stream.timings();
    let playback = match Playback::from_stream(stream, cli.wpm, OrpMode::Spritz) {
        Ok(playback) => playback,
        Err(error) => {
            if let Some(timings) = snapshot(&timings) {
                eprintln!("{}", format_timings(&timings));
            }
            return Err(error);
        }
    };
    let result = rsvp::tui::run(playback);

    if let Some(timings) = snapshot(&timings) {
        eprintln!("{}", format_timings(&timings));
    }

    result
}

fn print_loading_splash(path: &PathBuf) {
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| "FILE".into());
    let size = std::fs::metadata(path)
        .map(|metadata| format_size(metadata.len()))
        .unwrap_or_else(|_| "unknown size".into());

    eprintln!(
        "\
+----------------------------------------------+
| Rusty-Serial-Visual-Presentation             |
| Rapid Serial Visual Presentation TUI in Rust |
+----------------------------------------------+
loading {format} ({size})..."
    );
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn snapshot(
    timings: &Option<std::sync::Arc<std::sync::Mutex<StreamTimings>>>,
) -> Option<StreamTimings> {
    let timings = timings.as_ref()?;
    let wait_until = Instant::now() + Duration::from_millis(100);

    loop {
        let snapshot = timings.lock().expect("stream timing lock poisoned").clone();
        if snapshot.worker_elapsed > Duration::ZERO || Instant::now() >= wait_until {
            return Some(snapshot);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_loading_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1_536), "1.5 KB");
        assert_eq!(format_size(2 * 1024 * 1024), "2.0 MB");
    }
}
