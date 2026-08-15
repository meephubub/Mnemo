use crate::error::Result;

/// A minimal, dependency-free HTML-to-text pass: strips tags,
/// `<script>`/`<style>` bodies, and collapses whitespace. Good enough
/// for indexing; not a rendering engine.
///
/// A proper HTML/DOM-aware parser (e.g. `scraper`) can replace this
/// once the crate is allowed to grow its dependency footprint.
pub fn parse(raw: &str) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut in_tag = false;
    let mut skip_until_close_tag: Option<&'static str> = None;
    let mut tag_buf = String::new();

    while let Some(c) = chars.next() {
        if let Some(skip_tag) = skip_until_close_tag {
            tag_buf.push(c);
            let lower = tag_buf.to_lowercase();
            if lower.ends_with(&format!("</{skip_tag}>")) {
                skip_until_close_tag = None;
                tag_buf.clear();
            }
            continue;
        }

        match c {
            '<' => {
                in_tag = true;
                tag_buf.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let lower = tag_buf.to_lowercase();
                if lower.starts_with("script") {
                    skip_until_close_tag = Some("script");
                } else if lower.starts_with("style") {
                    skip_until_close_tag = Some("style");
                } else {
                    out.push(' ');
                }
                tag_buf.clear();
            }
            _ if in_tag => tag_buf.push(c),
            _ => out.push(c),
        }
    }

    let normalized = out.split_whitespace().collect::<Vec<_>>().join(" ");
    Ok(html_escape_decode(&normalized))
}

fn html_escape_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}
