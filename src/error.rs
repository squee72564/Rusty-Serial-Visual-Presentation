use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum OrpError {
    #[error("unsupported file extension for {path}")]
    UnsupportedExtension { path: PathBuf },

    #[error("failed to read {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not valid UTF-8 text")]
    InvalidUtf8 {
        path: PathBuf,
        #[source]
        source: std::string::FromUtf8Error,
    },

    #[error("failed to extract text from PDF {path}")]
    PdfExtract {
        path: PathBuf,
        #[source]
        source: pdf_extract::OutputError,
    },

    #[error("failed to extract text from EPUB {path}")]
    EpubExtract {
        path: PathBuf,
        #[source]
        source: epub_parser::epub::Error,
    },

    #[error("no readable text found in {path}")]
    NoReadableText { path: PathBuf },

    #[error("WPM must be between {min} and {max}; got {actual}")]
    InvalidWpm { actual: u16, min: u16, max: u16 },

    #[error("terminal error")]
    Terminal(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, OrpError>;
