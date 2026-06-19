use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{OrpError, Result};
use crate::extract::ExtractedDocument;
use crate::normalize::normalize_text;
use crate::tokenize::{Token, tokenize};

const CHANNEL_CAPACITY: usize = 2;
const TARGET_CHUNK_TOKENS: usize = 1_500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamChunk {
    pub id: usize,
    pub start_word_index: usize,
    pub tokens: Vec<Token>,
    pub eof: bool,
}

#[derive(Debug)]
pub enum StreamEvent {
    Chunk(StreamChunk),
    Failed(OrpError),
}

#[derive(Debug)]
pub struct StreamHandle {
    path: PathBuf,
    timings_enabled: bool,
    receiver: Receiver<StreamEvent>,
    timings: Arc<Mutex<StreamTimings>>,
}

impl StreamHandle {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn timings_enabled(&self) -> bool {
        self.timings_enabled
    }

    pub fn receiver(self) -> Receiver<StreamEvent> {
        self.receiver
    }

    pub fn timings(&self) -> Option<Arc<Mutex<StreamTimings>>> {
        self.timings_enabled.then(|| Arc::clone(&self.timings))
    }

    pub fn raw_timings(&self) -> Arc<Mutex<StreamTimings>> {
        Arc::clone(&self.timings)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StreamTimings {
    pub source_format: &'static str,
    pub document_load: Duration,
    pub page_extraction: Duration,
    pub normalize_tokenize: Duration,
    pub time_to_first_chunk: Option<Duration>,
    pub worker_elapsed: Duration,
    pub send_wait: Duration,
    pub pages_extracted: usize,
    pub chunks_emitted: usize,
}

pub fn spawn_stream(path: PathBuf, timings_enabled: bool) -> StreamHandle {
    let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
    let timings = Arc::new(Mutex::new(StreamTimings::default()));
    let worker_timings = Arc::clone(&timings);
    let worker_path = path.clone();

    thread::spawn(move || {
        let started = Instant::now();
        let result = run_worker(&worker_path, &sender, worker_timings.clone(), started);
        if let Err(error) = result {
            let _ = sender.send(StreamEvent::Failed(error));
        }
        worker_timings
            .lock()
            .expect("stream timing lock poisoned")
            .worker_elapsed = started.elapsed();
    });

    StreamHandle {
        path,
        timings_enabled,
        receiver,
        timings,
    }
}

pub fn format_timings(timings: &StreamTimings) -> String {
    let processing = timings.worker_elapsed.saturating_sub(timings.send_wait);
    format!(
        "timings: format {}, document load/extract {:?}, page extraction {:?} over {} page(s), normalize/tokenize {:?}, first chunk {:?}, worker elapsed {:?}, active processing {:?}, send wait {:?}, chunks {}",
        timings.source_format,
        timings.document_load,
        timings.page_extraction,
        timings.pages_extracted,
        timings.normalize_tokenize,
        timings.time_to_first_chunk,
        timings.worker_elapsed,
        processing,
        timings.send_wait,
        timings.chunks_emitted
    )
}

fn run_worker(
    path: &Path,
    sender: &SyncSender<StreamEvent>,
    timings: Arc<Mutex<StreamTimings>>,
    started: Instant,
) -> Result<()> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pdf") => stream_pdf(path, sender, timings, started),
        Some("txt") => stream_text(path, sender, timings, started),
        Some("md") | Some("markdown") => {
            stream_whole_document(path, "markdown", sender, timings, started)
        }
        Some("epub") => stream_whole_document(path, "epub", sender, timings, started),
        _ => Err(OrpError::UnsupportedExtension {
            path: path.to_path_buf(),
        }),
    }
}

fn stream_pdf(
    path: &Path,
    sender: &SyncSender<StreamEvent>,
    timings: Arc<Mutex<StreamTimings>>,
    started: Instant,
) -> Result<()> {
    set_source_format(&timings, "pdf");
    let load_started = Instant::now();
    let mut doc = pdf_extract::Document::load(path).map_err(|source| OrpError::PdfExtract {
        path: path.to_path_buf(),
        source: source.into(),
    })?;
    if doc.is_encrypted() {
        doc.decrypt("").map_err(|source| OrpError::PdfExtract {
            path: path.to_path_buf(),
            source: source.into(),
        })?;
    }
    timings
        .lock()
        .expect("stream timing lock poisoned")
        .document_load += load_started.elapsed();

    let page_numbers = doc.get_pages().keys().copied().collect::<Vec<_>>();
    let mut builder = ChunkBuilder::new();

    for page_num in page_numbers {
        let extraction_started = Instant::now();
        let mut text = String::new();
        {
            let mut output = pdf_extract::PlainTextOutput::new(&mut text);
            pdf_extract::output_doc_page(&doc, &mut output, page_num).map_err(|source| {
                OrpError::PdfExtract {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
        }
        {
            let mut guard = timings.lock().expect("stream timing lock poisoned");
            guard.page_extraction += extraction_started.elapsed();
            guard.pages_extracted += 1;
        }

        let nt_started = Instant::now();
        let tokens = normalize_and_tokenize(&text, &mut builder.next_paragraph_index);
        timings
            .lock()
            .expect("stream timing lock poisoned")
            .normalize_tokenize += nt_started.elapsed();

        if tokens.is_empty() {
            continue;
        }
        builder.push_tokens(tokens, sender, &timings, started)?;
    }

    builder.finish(sender, &timings, started, path)?;
    Ok(())
}

fn stream_text(
    path: &Path,
    sender: &SyncSender<StreamEvent>,
    timings: Arc<Mutex<StreamTimings>>,
    started: Instant,
) -> Result<()> {
    set_source_format(&timings, "text");
    let file = std::fs::File::open(path).map_err(|source| OrpError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut builder = ChunkBuilder::new();
    let mut buffer = String::new();
    let mut group = String::new();

    loop {
        buffer.clear();
        let bytes = reader
            .read_line(&mut buffer)
            .map_err(|source| OrpError::ReadFile {
                path: path.to_path_buf(),
                source,
            })?;
        if bytes == 0 {
            break;
        }
        group.push_str(&buffer);

        if buffer.trim().is_empty() || builder.pending_len() >= TARGET_CHUNK_TOKENS {
            flush_text_group(&mut group, &mut builder, sender, &timings, started)?;
        }
    }

    flush_text_group(&mut group, &mut builder, sender, &timings, started)?;
    builder.finish(sender, &timings, started, path)
}

fn stream_whole_document(
    path: &Path,
    source_format: &'static str,
    sender: &SyncSender<StreamEvent>,
    timings: Arc<Mutex<StreamTimings>>,
    started: Instant,
) -> Result<()> {
    set_source_format(&timings, source_format);
    let load_started = Instant::now();
    let document = crate::input::extract_path(path)?;
    timings
        .lock()
        .expect("stream timing lock poisoned")
        .document_load += load_started.elapsed();
    let ExtractedDocument { text, .. } = document;
    let mut builder = ChunkBuilder::new();
    let nt_started = Instant::now();
    let tokens = normalize_and_tokenize(&text, &mut builder.next_paragraph_index);
    timings
        .lock()
        .expect("stream timing lock poisoned")
        .normalize_tokenize += nt_started.elapsed();
    builder.push_tokens(tokens, sender, &timings, started)?;
    builder.finish(sender, &timings, started, path)
}

fn flush_text_group(
    group: &mut String,
    builder: &mut ChunkBuilder,
    sender: &SyncSender<StreamEvent>,
    timings: &Arc<Mutex<StreamTimings>>,
    started: Instant,
) -> Result<()> {
    if group.trim().is_empty() {
        group.clear();
        return Ok(());
    }

    let nt_started = Instant::now();
    let tokens = normalize_and_tokenize(group, &mut builder.next_paragraph_index);
    timings
        .lock()
        .expect("stream timing lock poisoned")
        .normalize_tokenize += nt_started.elapsed();
    group.clear();
    builder.push_tokens(tokens, sender, timings, started)
}

fn normalize_and_tokenize(text: &str, next_paragraph_index: &mut usize) -> Vec<Token> {
    let normalized = normalize_text(text);
    let mut tokens = tokenize(&normalized);
    if tokens.is_empty() {
        return tokens;
    }

    for token in &mut tokens {
        token.paragraph_index += *next_paragraph_index;
    }
    if let Some(last) = tokens.last() {
        *next_paragraph_index = last.paragraph_index + 1;
    }
    tokens
}

#[derive(Debug)]
struct ChunkBuilder {
    pending: Vec<Token>,
    chunk_id: usize,
    start_word_index: usize,
    next_paragraph_index: usize,
}

impl ChunkBuilder {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            chunk_id: 0,
            start_word_index: 0,
            next_paragraph_index: 0,
        }
    }

    fn pending_len(&self) -> usize {
        self.pending.len()
    }

    fn push_tokens(
        &mut self,
        mut tokens: Vec<Token>,
        sender: &SyncSender<StreamEvent>,
        timings: &Arc<Mutex<StreamTimings>>,
        started: Instant,
    ) -> Result<()> {
        if tokens.is_empty() {
            return Ok(());
        }

        self.pending.append(&mut tokens);
        while self.pending.len() >= TARGET_CHUNK_TOKENS {
            let rest = self.pending.split_off(TARGET_CHUNK_TOKENS);
            let chunk_tokens = std::mem::replace(&mut self.pending, rest);
            self.emit(chunk_tokens, false, sender, timings, started)?;
        }
        Ok(())
    }

    fn finish(
        &mut self,
        sender: &SyncSender<StreamEvent>,
        timings: &Arc<Mutex<StreamTimings>>,
        started: Instant,
        path: &Path,
    ) -> Result<()> {
        if self.chunk_id == 0 && self.pending.is_empty() {
            return Err(OrpError::NoReadableText {
                path: path.to_path_buf(),
            });
        }

        if !self.pending.is_empty() {
            let tokens = std::mem::take(&mut self.pending);
            self.emit(tokens, true, sender, timings, started)?;
        }

        Ok(())
    }

    fn emit(
        &mut self,
        tokens: Vec<Token>,
        eof: bool,
        sender: &SyncSender<StreamEvent>,
        timings: &Arc<Mutex<StreamTimings>>,
        started: Instant,
    ) -> Result<()> {
        if tokens.is_empty() {
            return Ok(());
        }

        let chunk = StreamChunk {
            id: self.chunk_id,
            start_word_index: self.start_word_index,
            tokens,
            eof,
        };
        self.start_word_index += chunk.tokens.len();
        self.chunk_id += 1;

        {
            let mut guard = timings.lock().expect("stream timing lock poisoned");
            if guard.time_to_first_chunk.is_none() {
                guard.time_to_first_chunk = Some(started.elapsed());
            }
        }

        let send_started = Instant::now();
        sender
            .send(StreamEvent::Chunk(chunk))
            .map_err(|_| OrpError::StreamDisconnected)?;
        let mut guard = timings.lock().expect("stream timing lock poisoned");
        guard.send_wait += send_started.elapsed();
        guard.chunks_emitted += 1;
        Ok(())
    }
}

fn set_source_format(timings: &Arc<Mutex<StreamTimings>>, source_format: &'static str) {
    timings
        .lock()
        .expect("stream timing lock poisoned")
        .source_format = source_format;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_stream_emits_ordered_chunks_with_final_eof() {
        let path =
            std::env::temp_dir().join(format!("rsvp-stream-text-{}.txt", std::process::id()));
        let text = (0..1_601)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        std::fs::write(&path, text).unwrap();

        let stream = spawn_stream(path.clone(), true);
        let receiver = stream.receiver();
        let first = match receiver.recv().unwrap() {
            StreamEvent::Chunk(chunk) => chunk,
            other => panic!("expected first chunk, got {other:?}"),
        };
        let second = match receiver.recv().unwrap() {
            StreamEvent::Chunk(chunk) => chunk,
            other => panic!("expected second chunk, got {other:?}"),
        };

        assert_eq!(first.id, 0);
        assert_eq!(first.start_word_index, 0);
        assert_eq!(first.tokens.len(), TARGET_CHUNK_TOKENS);
        assert_eq!(second.id, 1);
        assert_eq!(second.start_word_index, TARGET_CHUNK_TOKENS);
        assert!(second.eof);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_text_stream_reports_no_readable_text() {
        let path =
            std::env::temp_dir().join(format!("rsvp-stream-empty-{}.txt", std::process::id()));
        std::fs::write(&path, "\n\n").unwrap();

        let stream = spawn_stream(path.clone(), false);
        let receiver = stream.receiver();

        match receiver.recv().unwrap() {
            StreamEvent::Failed(OrpError::NoReadableText { path: error_path }) => {
                assert_eq!(error_path, path);
            }
            other => panic!("expected no readable text error, got {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }
}
