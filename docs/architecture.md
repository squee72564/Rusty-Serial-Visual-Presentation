# ORP Reader MVP Architecture

## Summary

ORP Reader is a terminal-first RSVP reader for prose documents. The MVP turns `.txt`, `.md`, best-effort `.pdf`, and best-effort `.epub` files into a clean stream of words, then displays those words one at a time with a highlighted Optimal Recognition Point (ORP).

The project is library-first: document ingestion, text cleanup, tokenization, RSVP word modeling, timing, and playback state live in reusable core modules. The terminal UI is the first frontend and should remain a thin layer over that core.

MVP non-goals: clipboard input, docx/URL ingestion, OCR, saved progress, async loading, GUI/web frontends, and aggressive PDF/EPUB cleanup.

## Architecture

The crate uses conventional Rust naming:

- Package: `orp-reader`
- Library crate: `orp_reader`
- Binary: `orp-reader`

Main modules:

- `input`: source detection and file loading entry points.
- `extract`: format-specific text extraction functions.
- `normalize`: whitespace and line-break cleanup.
- `tokenize`: conversion from normalized text into display tokens.
- `rsvp`: ORP strategies and RSVP display word construction.
- `playback`: timing and reader state machine.
- `tui`: Ratatui/crossterm terminal frontend.
- `error`: typed domain errors.

The pipeline is synchronous for MVP:

```text
file path
  -> extractor
  -> extracted document
  -> normalizer
  -> normalized document
  -> tokenizer
  -> RSVP words
  -> playback state
  -> TUI renderer
```

## Data Model And Behavior

Extraction:

- `.txt` files are read as UTF-8 text.
- `.md` files are parsed into readable prose with Markdown syntax removed.
- Fenced and indented Markdown code blocks are skipped.
- `.pdf` files use best-effort text extraction.
- `.epub` files use best-effort text extraction from the EPUB spine and ignore images, styling, and navigation.
- Scanned PDFs and PDFs that extract to no readable tokens are unsupported in MVP.

Normalization:

- Collapse repeated spaces and tabs.
- Preserve paragraph boundaries as metadata.
- Join obvious hyphenated line breaks.
- Defer page number, header, footer, table, and figure cleanup.

Tokenization:

- One token is one word-like display unit.
- Punctuation remains attached to the word, for example `word,`, `sentence.`, `don't`, and `well-known`.
- Unicode grapheme segmentation is required for safe indexing and highlighting.

RSVP modeling:

- Each `RsvpWord` contains display text, grapheme-aware pivot index, and display duration.
- The default ORP mode is Spritz-like length-table pivoting.
- The second MVP mode is center pivoting.
- The TUI cycles ORP modes at runtime with `o`.

Timing:

- Default speed: `300` WPM.
- Allowed range: `100..=1000` WPM.
- Keyboard adjustments change speed by `25` WPM.
- Base timing is `60_000 / wpm` milliseconds.
- Tokens ending in `,`, `;`, or `:` use a `1.5x` pause.
- Tokens ending in `.`, `?`, or `!` use a `2x` pause.
- Tokens before a paragraph boundary use a `2.5x` pause.
- Word-length pauses are a future extension and should be isolated behind timing code.

Playback:

- Core playback state owns play/pause, current token index, WPM, ORP mode, restart, previous/next, and end-of-document behavior.
- Reaching the end pauses on the final token.
- Progress is not persisted between runs.

Terminal controls:

- `Space`: pause/resume
- `Left`/`Right`: previous/next token
- `Up`/`Down`: increase/decrease WPM
- `o`: cycle ORP mode
- `r`: restart
- `q`: quit

## Error Handling

Errors are typed and should produce clear CLI/TUI messages. Important cases include:

- Unsupported file extension.
- File read failure.
- UTF-8 decoding failure.
- PDF extraction failure.
- Extraction succeeded but produced no readable tokens.
- Terminal initialization or rendering failure.

## Test Plan

Core behavior should be covered by focused unit tests:

- Txt extraction reads UTF-8.
- Markdown extraction strips markup and skips code blocks.
- PDF extraction works against one tiny text-PDF fixture.
- Normalization collapses whitespace and joins hyphenated line breaks.
- Tokenization preserves punctuation attached to words.
- ORP strategies return stable grapheme indexes.
- Timing returns base WPM duration plus punctuation and paragraph-boundary pauses.
- Playback handles pause/resume, stepping, WPM bounds, ORP cycling, restart, and end pause.

Acceptance scenario:

```text
orp-reader sample.md --wpm 300
```

The app loads readable prose, opens a focused terminal reader, anchors the highlighted ORP consistently, supports the MVP controls, pauses at the end, and exits cleanly.
