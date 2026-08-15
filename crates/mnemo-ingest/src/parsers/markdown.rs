use crate::error::Result;

/// A Markdown "section": a heading and the raw text that follows it,
/// up to (but not including) the next heading of equal-or-higher
/// level. Used to drive section-aware chunking (plan.md section 13
/// "Chunking" — "Markdown section chunks").
#[derive(Debug, Clone)]
pub struct MarkdownSection {
    pub heading: Option<String>,
    pub text: String,
    pub start_offset: usize,
}

/// Extract the document title (first level-1 heading, if any) and
/// split the body into heading-delimited sections.
pub fn parse(raw: &str) -> Result<(Option<String>, Vec<MarkdownSection>)> {
    let mut title = None;
    let mut sections = Vec::new();

    let mut current_heading: Option<String> = None;
    let mut current_text = String::new();
    let mut current_start = 0usize;
    let mut offset = 0usize;

    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        if let Some(heading_text) = trimmed.trim_start().strip_prefix("# ") {
            if title.is_none() {
                title = Some(heading_text.trim().to_string());
            }
        }

        let is_heading = trimmed.trim_start().starts_with('#');
        if is_heading {
            if !current_text.trim().is_empty() || current_heading.is_some() {
                sections.push(MarkdownSection {
                    heading: current_heading.take(),
                    text: std::mem::take(&mut current_text),
                    start_offset: current_start,
                });
            }
            current_heading = Some(trimmed.trim_start_matches('#').trim().to_string());
            current_start = offset;
        } else {
            current_text.push_str(line);
        }

        offset += line.len();
    }

    if !current_text.trim().is_empty() || current_heading.is_some() {
        sections.push(MarkdownSection {
            heading: current_heading,
            text: current_text,
            start_offset: current_start,
        });
    }

    if sections.is_empty() {
        sections.push(MarkdownSection {
            heading: None,
            text: raw.to_string(),
            start_offset: 0,
        });
    }

    Ok((title, sections))
}
