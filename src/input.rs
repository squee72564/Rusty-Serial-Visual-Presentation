use std::path::Path;

use crate::error::{OrpError, Result};
use crate::extract::{extract_epub, extract_markdown, extract_pdf, extract_txt};

pub fn extract_path(path: &Path) -> Result<String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt") => extract_txt(path),
        Some("md") | Some("markdown") => extract_markdown(path),
        Some("pdf") => extract_pdf(path),
        Some("epub") => extract_epub(path),
        _ => Err(OrpError::UnsupportedExtension {
            path: path.to_path_buf(),
        }),
    }
}
