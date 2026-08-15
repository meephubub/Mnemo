use crate::error::Result;

/// Plain text needs no transformation beyond reading it as UTF-8.
pub fn parse(raw: &str) -> Result<String> {
    Ok(raw.to_string())
}
