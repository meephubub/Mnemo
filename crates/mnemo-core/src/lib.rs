//! `mnemo-core` — shared data model, identifiers, and error type for
//! the Mnemo workspace (see plan.md, section 63 "Crate Structure" and
//! section 64 "Core Data Model").
//!
//! This crate intentionally has no I/O and no external dependencies
//! beyond serialization/id/time libraries, so every other crate in
//! the workspace (storage, ingest, search, ...) can depend on it
//! without pulling in SQLite, embedding models, etc.

pub mod error;
pub mod ids;
pub mod math;
pub mod models;

pub use error::{MnemoError, Result};
pub use models::*;
