//! `mnemo-storage` — SQLite metadata store + FTS5 lexical index
//! (plan.md section 5.1 "Storage Layer" and section 7 "Full-Text
//! Search").
//!
//! This crate owns the schema and all CRUD access to it. Higher
//! layers (`mnemo-ingest`, `mnemo-search`, the top-level `mnemo`
//! facade) depend on it instead of talking to SQLite directly.

pub mod db;
pub mod error;
pub mod fts;
pub mod migrations;
pub mod repositories;
pub mod util;

pub use db::Db;
pub use error::{Result, StorageError};
