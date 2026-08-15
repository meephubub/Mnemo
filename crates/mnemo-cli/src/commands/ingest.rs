use std::path::PathBuf;

use mnemo::{IngestOutcome, Mnemo};

pub async fn run(db: &Mnemo, paths: Vec<PathBuf>) -> anyhow::Result<()> {
    if paths.is_empty() {
        anyhow::bail!("no paths given; usage: mnemo ingest <path> [<path> ...]");
    }

    let ingest = db.ingest();
    for path in paths {
        match ingest.ingest_file(path.clone()).await {
            Ok(IngestOutcome::Indexed(doc)) => {
                println!(
                    "indexed  {}  ({})",
                    path.display(),
                    doc.title.as_deref().unwrap_or("untitled")
                );
            }
            Ok(IngestOutcome::Unchanged(_)) => {
                println!("unchanged {}", path.display());
            }
            Err(e) => {
                eprintln!("failed to ingest {}: {e}", path.display());
            }
        }
    }

    Ok(())
}
