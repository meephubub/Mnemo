pub mod docx;
pub mod html;
pub mod markdown;
pub mod pdf;
pub mod txt;

use std::path::Path;

use crate::error::{IngestError, Result};

/// File formats this crate currently knows how to parse.
///
/// Email parsing is planned (plan.md Phase 2) but not yet implemented
/// here — see ROADMAP.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Text,
    Markdown,
    Html,
    Pdf,
    Docx,
}

/// Whether `kind` is a binary container format (parsed directly from
/// bytes) as opposed to plain UTF-8 text.
pub fn is_binary(kind: FileKind) -> bool {
    matches!(kind, FileKind::Pdf | FileKind::Docx)
}

pub fn identify(path: &Path) -> Result<FileKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();

    match ext.as_str() {
        "txt" | "text" | "log" => Ok(FileKind::Text),
        "md" | "markdown" => Ok(FileKind::Markdown),
        "html" | "htm" => Ok(FileKind::Html),
        "pdf" => Ok(FileKind::Pdf),
        "docx" => Ok(FileKind::Docx),
        other => Err(IngestError::UnsupportedType(other.to_string())),
    }
}

pub fn mime_type(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Text => "text/plain",
        FileKind::Markdown => "text/markdown",
        FileKind::Html => "text/html",
        FileKind::Pdf => "application/pdf",
        FileKind::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    }
}

/// A natural section of a parsed document: an optional heading, the
/// section's text, its character offset within the document's
/// reconstructed `ParsedFile::text`, and (for paginated formats like
/// PDF) the 1-based page it came from.
#[derive(Debug, Clone)]
pub struct ParsedSection {
    pub heading: Option<String>,
    pub text: String,
    pub start_offset: usize,
    pub page: Option<u32>,
}

/// The result of parsing a raw file into plain, indexable text plus
/// any section structure the format provided.
pub struct ParsedFile {
    pub kind: FileKind,
    pub title: Option<String>,
    pub text: String,
    /// Natural sections of the document: Markdown headings, DOCX
    /// heading-styled paragraphs, PDF pages, or (for formats without
    /// any such structure) a single unnamed section.
    pub sections: Vec<ParsedSection>,
}

/// Parse raw file bytes of the given `kind` into a [`ParsedFile`].
///
/// Text-based formats (plain text, Markdown, HTML) must be valid
/// UTF-8; binary container formats (PDF, DOCX) are parsed directly
/// from bytes.
pub fn parse(kind: FileKind, raw: &[u8]) -> Result<ParsedFile> {
    match kind {
        FileKind::Text => {
            let text = txt::parse(to_utf8(raw)?)?;
            Ok(ParsedFile {
                kind,
                title: None,
                sections: vec![single_section(text.clone())],
                text,
            })
        }
        FileKind::Markdown => {
            let (title, sections) = markdown::parse(to_utf8(raw)?)?;
            let text = sections
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(ParsedFile {
                kind,
                title,
                text,
                sections: sections
                    .into_iter()
                    .map(|s| ParsedSection {
                        heading: s.heading,
                        text: s.text,
                        start_offset: s.start_offset,
                        page: None,
                    })
                    .collect(),
            })
        }
        FileKind::Html => {
            let text = html::parse(to_utf8(raw)?)?;
            Ok(ParsedFile {
                kind,
                title: None,
                sections: vec![single_section(text.clone())],
                text,
            })
        }
        FileKind::Pdf => {
            let (title, sections) = pdf::parse(raw)?;
            let text = sections
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(ParsedFile { kind, title, text, sections })
        }
        FileKind::Docx => {
            let (title, sections) = docx::parse(raw)?;
            let text = sections
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(ParsedFile { kind, title, text, sections })
        }
    }
}

fn single_section(text: String) -> ParsedSection {
    ParsedSection {
        heading: None,
        text,
        start_offset: 0,
        page: None,
    }
}

fn to_utf8(raw: &[u8]) -> Result<&str> {
    std::str::from_utf8(raw).map_err(|e| IngestError::Parse(format!("invalid UTF-8: {e}")))
}
