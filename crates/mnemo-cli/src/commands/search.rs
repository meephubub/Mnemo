use mnemo::{HitKind, Mnemo, SearchOptions, SearchScope};

use crate::ScopeArg;

pub async fn run(db: &Mnemo, query: String, limit: usize, scope: ScopeArg) -> anyhow::Result<()> {
    if query.trim().is_empty() {
        anyhow::bail!("no query given; usage: mnemo search <query...>");
    }

    let options = SearchOptions {
        scope: match scope {
            ScopeArg::All => SearchScope::All,
            ScopeArg::Documents => SearchScope::Documents,
            ScopeArg::Conversations => SearchScope::Conversations,
        },
        limit,
    };

    let hits = db.search().search_with_options(query, options).await?;
    if hits.is_empty() {
        println!("no results");
        return Ok(());
    }

    for (i, hit) in hits.iter().enumerate() {
        let origin = match hit.kind {
            HitKind::Chunk => hit
                .document_title
                .clone()
                .or_else(|| hit.source_name.clone())
                .unwrap_or_else(|| "document".to_string()),
            HitKind::Message => "conversation".to_string(),
        };
        println!("{:>2}. [{:>6.2}] {origin}", i + 1, hit.score);
        let snippet: String = hit.text.chars().take(200).collect();
        println!("    {}", snippet.replace('\n', " "));
    }

    Ok(())
}
