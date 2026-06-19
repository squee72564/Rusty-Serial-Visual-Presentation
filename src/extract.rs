use std::path::Path;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::error::{OrpError, Result};

pub fn extract_txt(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|source| OrpError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    String::from_utf8(bytes).map_err(|source| OrpError::InvalidUtf8 {
        path: path.to_path_buf(),
        source,
    })
}

pub fn extract_markdown(path: &Path) -> Result<String> {
    Ok(markdown_to_text(&extract_txt(path)?))
}

pub fn extract_pdf(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path).map_err(|source| OrpError::PdfExtract {
        path: path.to_path_buf(),
        source,
    })
}

pub fn extract_epub(path: &Path) -> Result<String> {
    let epub = epub_parser::Epub::parse(path).map_err(|source| OrpError::EpubExtract {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(epub
        .pages
        .iter()
        .map(|page| page.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n"))
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
    use std::io::{Cursor, Write};

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

        let text = extract_pdf(&path).unwrap();

        assert!(text.contains("Hello PDF text"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn epub_extraction_reads_spine_order_text() {
        let path =
            std::env::temp_dir().join(format!("orp-reader-fixture-{}.epub", std::process::id()));
        std::fs::write(&path, tiny_epub()).unwrap();

        let text = extract_epub(&path).unwrap();

        assert!(text.contains("First chapter text."));
        assert!(text.contains("Second chapter text."));
        assert!(
            text.find("First chapter text.").unwrap() < text.find("Second chapter text.").unwrap()
        );
        assert!(!text.contains("console.log"));
        assert!(!text.contains("display:none"));

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

    fn tiny_epub() -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            write_zip_file(&mut archive, "mimetype", "application/epub+zip", options);
            write_zip_file(
                &mut archive,
                "META-INF/container.xml",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
                options,
            );
            write_zip_file(
                &mut archive,
                "OEBPS/content.opf",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Tiny EPUB</dc:title>
    <dc:creator>Test Author</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="chapter-1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chapter-2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chapter-1"/>
    <itemref idref="chapter-2"/>
  </spine>
</package>"#,
                options,
            );
            write_zip_file(
                &mut archive,
                "OEBPS/chapter1.xhtml",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><style>display:none</style></head>
  <body>
    <script>console.log('skip me')</script>
    <p>First chapter text.</p>
  </body>
</html>"#,
                options,
            );
            write_zip_file(
                &mut archive,
                "OEBPS/chapter2.xhtml",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body><p>Second chapter text.</p></body>
</html>"#,
                options,
            );
            archive.finish().unwrap();
        }

        cursor.into_inner()
    }

    fn write_zip_file<W: Write + std::io::Seek>(
        archive: &mut zip::ZipWriter<W>,
        name: &str,
        content: &str,
        options: zip::write::SimpleFileOptions,
    ) {
        archive.start_file(name, options).unwrap();
        archive.write_all(content.as_bytes()).unwrap();
    }
}
