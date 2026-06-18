use std::path::{Path, PathBuf};

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::error::{OrpError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Text,
    Markdown,
    Pdf,
}

#[derive(Debug, Clone)]
pub struct ExtractedDocument {
    pub source_path: PathBuf,
    pub format: SourceFormat,
    pub text: String,
    pub warnings: Vec<String>,
}

pub trait Extractor {
    fn extract(&self, path: &Path) -> Result<ExtractedDocument>;
}

#[derive(Debug, Default)]
pub struct TxtExtractor;

impl Extractor for TxtExtractor {
    fn extract(&self, path: &Path) -> Result<ExtractedDocument> {
        let bytes = std::fs::read(path).map_err(|source| OrpError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        let text = String::from_utf8(bytes).map_err(|source| OrpError::InvalidUtf8 {
            path: path.to_path_buf(),
            source,
        })?;

        Ok(ExtractedDocument {
            source_path: path.to_path_buf(),
            format: SourceFormat::Text,
            text,
            warnings: Vec::new(),
        })
    }
}

#[derive(Debug, Default)]
pub struct MarkdownExtractor;

impl Extractor for MarkdownExtractor {
    fn extract(&self, path: &Path) -> Result<ExtractedDocument> {
        let bytes = std::fs::read(path).map_err(|source| OrpError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        let markdown = String::from_utf8(bytes).map_err(|source| OrpError::InvalidUtf8 {
            path: path.to_path_buf(),
            source,
        })?;

        Ok(ExtractedDocument {
            source_path: path.to_path_buf(),
            format: SourceFormat::Markdown,
            text: markdown_to_text(&markdown),
            warnings: Vec::new(),
        })
    }
}

#[derive(Debug, Default)]
pub struct PdfExtractor;

impl Extractor for PdfExtractor {
    fn extract(&self, path: &Path) -> Result<ExtractedDocument> {
        let text = pdf_extract::extract_text(path).map_err(|source| OrpError::PdfExtract {
            path: path.to_path_buf(),
            source,
        })?;

        Ok(ExtractedDocument {
            source_path: path.to_path_buf(),
            format: SourceFormat::Pdf,
            text,
            warnings: vec!["PDF extraction is best-effort and may include layout artifacts".into()],
        })
    }
}

fn markdown_to_text(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut text = String::new();
    let mut skip_code_block = false;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
            | Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                skip_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                skip_code_block = false;
                push_break(&mut text);
            }
            Event::Text(value) | Event::Code(value) if !skip_code_block => {
                text.push_str(&value);
            }
            Event::SoftBreak | Event::HardBreak if !skip_code_block => {
                text.push('\n');
            }
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Heading(_)) if !skip_code_block => {
                push_break(&mut text);
            }
            _ => {}
        }
    }

    text
}

fn push_break(text: &mut String) {
    if !text.ends_with("\n\n") {
        if text.ends_with('\n') {
            text.push('\n');
        } else {
            text.push_str("\n\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_extraction_skips_code_blocks_and_markup() {
        let text = markdown_to_text(
            "# Heading\n\nThis is **bold** text.\n\n```rust\nfn main() {}\n```\n\nAfter `inline` code.",
        );

        assert!(text.contains("Heading"));
        assert!(text.contains("This is bold text."));
        assert!(text.contains("After inline code."));
        assert!(!text.contains("fn main"));
        assert!(!text.contains("**"));
    }

    #[test]
    fn pdf_extraction_reads_tiny_text_pdf() {
        let path =
            std::env::temp_dir().join(format!("orp-reader-fixture-{}.pdf", std::process::id()));
        std::fs::write(&path, tiny_pdf()).unwrap();

        let document = PdfExtractor.extract(&path).unwrap();

        assert!(document.text.contains("Hello PDF text"));

        let _ = std::fs::remove_file(path);
    }

    fn tiny_pdf() -> Vec<u8> {
        let objects = [
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
            "4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        ];
        let stream = "BT /F1 24 Tf 100 700 Td (Hello PDF text) Tj ET\n";
        let stream_object = format!(
            "5 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
            stream.len(),
            stream
        );

        let mut pdf = "%PDF-1.4\n".to_string();
        let mut offsets = Vec::new();
        for object in objects {
            offsets.push(pdf.len());
            pdf.push_str(object);
        }
        offsets.push(pdf.len());
        pdf.push_str(&stream_object);

        let xref = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));

        pdf.into_bytes()
    }
}
