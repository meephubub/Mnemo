//! `mnemo` CLI — a thin command-line wrapper over the `mnemo` facade
//! crate (plan.md section 62 "Core API").
//!
//! Every command opens (or creates) a database file, so this doubles
//! as `init`: `mnemo --db mnemo.db profile list` works even if
//! `mnemo.db` doesn't exist yet.

mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Local-first, high-performance personal memory and knowledge engine.
#[derive(Parser)]
#[command(name = "mnemo", version, about)]
struct Cli {
    /// Path to the Mnemo SQLite database file.
    #[arg(long, global = true, default_value = "mnemo.db")]
    db: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create the database file (and its schema) if it doesn't exist yet.
    Init,

    /// Ingest one or more files into the searchable knowledge base.
    Ingest {
        /// Paths to files to ingest (.txt, .md, .html).
        paths: Vec<PathBuf>,
    },

    /// Run a lexical (BM25) search over ingested documents and conversations.
    Search {
        query: Vec<String>,

        /// Maximum number of results to return.
        #[arg(long, default_value_t = 10)]
        limit: usize,

        /// Restrict the search to documents, conversations, or both (default).
        #[arg(long, value_enum, default_value = "all")]
        scope: ScopeArg,
    },

    /// Read or update the small, stable user profile.
    #[command(subcommand)]
    Profile(ProfileCommand),

    /// Create and list durable memories.
    #[command(subcommand)]
    Memory(MemoryCommand),

    /// Print counts of everything currently stored.
    Stats,
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// Set (or update) a profile key.
    Set {
        key: String,
        value: String,
        #[arg(long, default_value_t = 1.0)]
        confidence: f32,
    },
    /// Get a single profile key.
    Get { key: String },
    /// List every profile entry.
    List,
    /// Remove a profile key.
    Remove { key: String },
}

#[derive(Subcommand)]
enum MemoryCommand {
    /// Record a new memory.
    Add {
        content: Vec<String>,
        #[arg(long, value_enum, default_value = "fact")]
        r#type: MemoryTypeArg,
    },
    /// List memories, optionally filtered by lifecycle status.
    List {
        #[arg(long, value_enum)]
        status: Option<MemoryStatusArg>,
    },
    /// Permanently delete a memory by id.
    Remove { id: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ScopeArg {
    All,
    Documents,
    Conversations,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum MemoryTypeArg {
    Fact,
    Preference,
    Interest,
    Goal,
    Project,
    Person,
    Location,
    Routine,
    Decision,
    Event,
    Temporary,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum MemoryStatusArg {
    Candidate,
    Active,
    Temporary,
    Superseded,
    Archived,
    Expired,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db = mnemo::Mnemo::open(&cli.db)
        .map_err(|e| anyhow::anyhow!("failed to open database at {}: {e}", cli.db.display()))?;

    match cli.command {
        Command::Init => {
            println!("Initialized Mnemo database at {}", cli.db.display());
        }
        Command::Ingest { paths } => commands::ingest::run(&db, paths).await?,
        Command::Search { query, limit, scope } => {
            commands::search::run(&db, query.join(" "), limit, scope).await?
        }
        Command::Profile(cmd) => commands::profile::run(&db, cmd).await?,
        Command::Memory(cmd) => commands::memory::run(&db, cmd).await?,
        Command::Stats => commands::stats::run(&db).await?,
    }

    Ok(())
}
