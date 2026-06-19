# rsvp

Terminal RSVP reader with Optimal Recognition Point highlighting.

`rsvp` reads prose from a file, cleans it into a stream of words, and displays
one word at a time in the terminal. It is built for quick focused reading, not
document conversion.

## Supported files

- `.txt`
- `.md` / `.markdown`
- `.pdf` best effort
- `.epub` best effort

Scanned PDFs, OCR, saved progress, and DOCX/URL input are not supported.

## Run

```sh
cargo run -- path/to/book.md
```

Set reading speed:

```sh
cargo run -- path/to/book.pdf --wpm 350
```

Show extraction and chunking timings after exit:

```sh
cargo run -- path/to/book.epub --timings
```

## Controls

| Key | Action |
| --- | --- |
| `Space` | pause / resume |
| `Left` / `Right` | previous / next word |
| `Up` / `Down` | increase / decrease WPM |
| `o` | switch ORP mode |
| `r` | restart |
| `q` | quit |

## Development

```sh
cargo test
```

The core pipeline lives in `src/`: extraction, normalization, tokenization,
RSVP word modeling, playback state, and the Ratatui terminal UI.
