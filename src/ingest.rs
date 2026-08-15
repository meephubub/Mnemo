//! Ingestion API surface (plan.md section 14 "Document Ingestion").
//!
//! Bridges the pure `mnemo-ingest` parsing/chunking pipeline into
//! persisted `Source` / `Document` / `Chunk` rows, with content-hash
//! based deduplication so re-ingesting an unchanged file is a no-op.

use std::path::{Path, PathBuf};

use mnemo_core::ids::SourceId;
use mnemo_core::models::{Document, Source, SourceType};
use mnemo_core::Result;
use mnemo_ingest::{parsers::FileKind, ChunkConfig, IngestedFile};
use mnemo_storage::repositories::{chunks, documents, sources};
use mnemo_storage::Db;

/// Handle for turning raw files/text into indexed, retrievable
/// documents.
///
/// Obtained via [`crate::Mnemo::ingest`]; cheap to create (it just
/// holds a clone of the shared DB handle).
#[derive(Clone)]
pub struct IngestHandle {
    db: Db,
}

/// Outcome of an ingestion call, distinguishing a freshly indexed
/// document from one that was already up to date (plan.md section 15
/// "Incremental Indexing").
#[derive(Debug, Clone)]
pub enum IngestOutcome {
    Indexed(Document),
    Unchanged(Document),
}

impl IngestOutcome {
    pub fn document(&self) -> &Document {
        match self {
            IngestOutcome::Indexed(d) | IngestOutcome::Unchanged(d) => d,
        }
    }
}

impl IngestHandle {
    pub(crate) fn new(db: Db) -> Self {
        Self { db }
    }

    /// Ingest a file from disk, identifying its parser from the file
    /// extension. Creates a `File`-typed [`Source`] named after the
    /// path if this is the first time this path has been ingested.
    pub async fn ingest_file(&self, path: impl AsRef<Path> + Send + 'static) -> Result<IngestOutcome> {
        let db = self.db.clone();
        crate::blocking::run(move || {
            let path: PathBuf = path.as_ref().to_path_buf();
            let ingested = mnemo_ingest::ingest_path(&path)?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled")
                .to_string();
            let uri = path.to_string_lossy().to_string();
            persist(&db, SourceType::File, name, Some(uri), ingested)
        })
        .await
    }

    /// Ingest raw text (e.g. a pasted note) that didn't come from a
    /// file on disk.
    pub async fn ingest_text(
        &self,
        name: impl Into<String> + Send + 'static,
        kind: FileKind,
        text: impl Into<String> + Send + 'static,
    ) -> Result<IngestOutcome> {
        let db = self.db.clone();
        crate::blocking::run(move || {
            let text = text.into();
            let ingested = mnemo_ingest::ingest_str_with_config(kind, &text, &ChunkConfig::default())?;
            persist(&db, SourceType::UserStatement, name.into(), None, ingested)
        })
        .await
    }

    /// List every ingested source, most recently indexed first.
    pub async fn list_sources(&self) -> Result<Vec<Source>> {
        let db = self.db.clone();
        crate::blocking::run(move || Ok(sources::list(&db.conn())?)).await
    }

    /// List every document belonging to `source_id`... actually every
    /// indexed document, most recently indexed first (documents don't
    /// currently support per-source filtering at the repository
    /// layer; see ROADMAP.md).
    pub async fn list_documents(&self) -> Result<Vec<Document>> {
        let db = self.db.clone();
        crate::blocking::run(move || Ok(documents::list(&db.conn())?)).await
    }

    /// Remove a source and every document/chunk that came from it.
    pub async fn remove_source(&self, source_id: SourceId) -> Result<()> {
        let db = self.db.clone();
        crate::blocking::run(move || {
            sources::delete(&db.conn(), source_id)?;
            Ok(())
        })
        .await
    }
}

/// Shared persistence path for both `ingest_file` and `ingest_text`:
/// find-or-create the owning `Source`, then insert a new `Document` +
/// its `Chunk`s only if the content actually changed.
fn persist(
    db: &Db,
    source_type: SourceType,
    name: String,
    uri: Option<String>,
    ingested: IngestedFile,
) -> Result<IngestOutcome> {
    let conn = db.conn();

    if let Some(existing_doc) = find_unchanged_document(&conn, &ingested.content_hash)? {
        return Ok(IngestOutcome::Unchanged(existing_doc));
    }

    let source = match sources::find_by_content_hash(&conn, &ingested.content_hash)? {
        Some(existing) => existing,
        None => {
            let mut source = Source::new(source_type, name);
            source.uri = uri;
            source.content_hash = Some(ingested.content_hash.clone());
            sources::insert(&conn, &source)?;
            source
        }
    };

    let mut document = Document::new(
        source.id,
        ingested.mime_type,
        ingested.content_hash,
        ingested.parser_version,
    );
    document.title = ingested.title;
    documents::insert(&conn, &document)?;

    let chunk_rows: Vec<_> = ingested
        .chunks
        .into_iter()
        .enumerate()
        .map(|(index, draft)| mnemo_core::models::Chunk {
            id: mnemo_core::ids::ChunkId::new(),
            document_id: document.id,
            text: draft.text,
            start_offset: draft.start_offset,
            end_offset: draft.end_offset,
            page: None,
            section: draft.section,
            chunk_index: index,
        })
        .collect();
    chunks::insert_many(&conn, &chunk_rows)?;

    Ok(IngestOutcome::Indexed(document))
}

/// A document is "unchanged" if a document with the same content hash
/// already exists; re-ingesting it is then a no-op.
fn find_unchanged_document(
    conn: &rusqlite::Connection,
    content_hash: &str,
) -> Result<Option<Document>> {
    let docs = documents::list(conn)?;
    Ok(docs.into_iter().find(|d| d.content_hash == content_hash))
}
