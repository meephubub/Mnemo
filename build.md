# Building Mnemo

This is a pure Rust workspace. No `cargo add`, `cargo new`, or other
scaffolding commands were run to produce it — every `Cargo.toml` and
source file was written by hand, so the commands below are the
**first** commands you should run against this tree.

Nothing here needs network access beyond the crates.io downloads that
`cargo build` performs itself.

## Prerequisites

- Rust 1.75+ (edition 2021). Install via [rustup](https://rustup.rs) if
  you don't have it:
  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- SQLite is **not** a separate system dependency: `mnemo-storage`
  depends on `rusqlite` with the `bundled` feature, which compiles
  SQLite from source as part of the build. A C compiler (`cc`) must be
  on `PATH` for that (already the case on most dev machines / CI
  images; on a bare Debian/Ubuntu box: `apt-get install build-essential`).

## Workspace layout

```
Cargo.toml                 # workspace root + the `mnemo` facade package (src/lib.rs)
src/                        # the `mnemo` facade crate
crates/
  mnemo-core/                # data model, IDs, error type (no I/O)
  mnemo-storage/              # SQLite schema, migrations, repositories, FTS5
  mnemo-ingest/                # txt/md/html parsing + chunking (pure, no I/O beyond reading files)
  mnemo-search/                 # lexical (BM25), vector (cosine), hybrid retrieval + context packing
  mnemo-embeddings/              # Embedder trait + default HashingEmbedder (Phase 4)
  mnemo-cli/                     # `mnemo` binary (init/ingest/search/embed/context/profile/memory/stats)
```

## Build everything

```sh
cargo build --workspace
```

Release build:

```sh
cargo build --workspace --release
```

## Run the CLI

The CLI binary is named `mnemo` (crate `mnemo-cli`). Every command
opens/creates a database file given via `--db` (defaults to
`mnemo.db` in the current directory), so there's no separate "init"
step required — but an explicit `init` command exists too:

```sh
cargo run -p mnemo-cli -- --db mnemo.db init

# Ingest documents (.txt, .md, .html)
cargo run -p mnemo-cli -- --db mnemo.db ingest ./notes/*.md

# Search everything that's been ingested
cargo run -p mnemo-cli -- --db mnemo.db search "project deadline"
cargo run -p mnemo-cli -- --db mnemo.db search --scope documents --limit 5 "quarterly report"
cargo run -p mnemo-cli -- --db mnemo.db search --mode lexical "project deadline"
cargo run -p mnemo-cli -- --db mnemo.db search --mode vector "project deadline"
cargo run -p mnemo-cli -- --db mnemo.db search --mode hybrid --lexical-weight 0.3 --vector-weight 0.7 "project deadline"

# Generate vector embeddings for all ingested chunks
cargo run -p mnemo-cli -- --db mnemo.db embed
cargo run -p mnemo-cli -- --db mnemo.db embed --rebuild

# Pack search results into a token-budgeted context for prompt injection
cargo run -p mnemo-cli -- --db mnemo.db context "project deadline" --token-budget 2000 --max-sources 5
cargo run -p mnemo-cli -- --db mnemo.db context "project deadline" --mode lexical

# Profile (small, stable key/value facts about the user)
cargo run -p mnemo-cli -- --db mnemo.db profile set name "Ada"
cargo run -p mnemo-cli -- --db mnemo.db profile list
cargo run -p mnemo-cli -- --db mnemo.db profile get name
cargo run -p mnemo-cli -- --db mnemo.db profile remove name

# Memories (durable facts/preferences/decisions with a lifecycle)
cargo run -p mnemo-cli -- --db mnemo.db memory add --type preference "Prefers dark mode"
cargo run -p mnemo-cli -- --db mnemo.db memory list
cargo run -p mnemo-cli -- --db mnemo.db memory list --status active
cargo run -p mnemo-cli -- --db mnemo.db memory remove <memory-id>

# Counts of everything currently stored
cargo run -p mnemo-cli -- --db mnemo.db stats
```

After a release build, the binary is at
`target/release/mnemo` and can be run directly instead of via
`cargo run`.

## Using `mnemo` as a library

Add it as a path dependency from another crate in this workspace, or
via `git`/`path` from an external project:

```toml
[dependencies]
mnemo = { path = "../mnemo" } # or a git dependency once published
```

```rust
let db = mnemo::Mnemo::open("mnemo.db")?;
db.ingest().ingest_file("notes.md").await?;
db.embed().embed_pending().await?;
let hits = db.search().search("project deadline").await?;
let ctx = db.context().pack(Default::default()).await?;
```

## Checks

```sh
cargo check --workspace
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
cargo test --workspace
```

No tests have been written yet for `mnemo-storage` or `mnemo-ingest`
repositories (see `ROADMAP.md`), but `mnemo-embeddings` and
`mnemo-search` both include unit tests — `cargo test` will run those.

## Why no crates were added via `cargo add`

Per project constraints, every dependency was declared directly in
each `Cargo.toml`'s `[dependencies]` table (backed by shared version
pins in the workspace root's `[workspace.dependencies]`), rather than
via `cargo add`. If you need to add a new dependency going forward,
edit the relevant `Cargo.toml` by hand and add a matching entry under
`[workspace.dependencies]` if it should be shared, then run
`cargo build` to update `Cargo.lock` — do not run `cargo add`.
