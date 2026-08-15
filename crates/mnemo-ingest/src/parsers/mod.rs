pub mod html;
pub mod markdown;
pub mod txt;

use std::path::Path;

use crate::error::{IngestError, Result};

/// File formats this crate currently knows how to parse.
///
/// PDF, DOCX, and email parsing are planned (plan.md Phase 2) but not
/// yet implemented here — see ROADMAP.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Text,
    Markdown,
    Html,
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
        other => Err(IngestError::UnsupportedType(other.to_string())),
    }
}

pub fn mime_type(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Text => "text/plain",
        FileKind::Markdown => "text/markdown",
        FileKind::Html => "text/html",
    }
}

/// The result of parsing a raw file into plain, indexable text plus
/// any section structure the format provided.
pub struct ParsedFile {
    pub kind: FileKind,
    pub title: Option<String>,
    pub text: String,
    /// (heading, section_text, start_offset) triples, when the source
    /// format has natural sections (currently: Markdown headings).
    /// Plain text/HTML yield a single unnamed section.
    pub sections: Vec<(Option<String>, String, usize)>,
}

pub fn parse(kind: FileKind, raw: &str) -> Result<ParsedFile> {
    match kind {
        FileKind::Text => {
            let text = txt::parse(raw)?;
            Ok(ParsedFile {
                kind,
                title: None,
                sections: vec![(None, text.clone(), 0)],
                text,
            })
        }
        FileKind::Markdown => {
            let (title, sections) = markdown::parse(raw)?;
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
                    .map(|s| (s.heading, s.text, s.start_offset))
                    .collect(),
            })
        }
        FileKind::Html => {
            let text = html::parse(raw)?;
            Ok(ParsedFile {
                kind,
                title: None,
                sections: vec![(None, text.clone(), 0)],
                text,
            })
        }
    }
}
