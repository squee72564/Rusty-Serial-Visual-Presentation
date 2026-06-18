use std::path::Path;

use crate::error::{OrpError, Result};
use crate::extract::{ExtractedDocument, Extractor, MarkdownExtractor, PdfExtractor, TxtExtractor};

pub fn extract_path(path: &Path) -> Result<ExtractedDocument> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt") => TxtExtractor.extract(path),
        Some("md") | Some("markdown") => MarkdownExtractor.extract(path),
        Some("pdf") => PdfExtractor.extract(path),
        _ => Err(OrpError::UnsupportedExtension {
            path: path.to_path_buf(),
        }),
    }
}
