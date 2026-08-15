# Building Mnemo

This is a pure Rust workspace. No `cargo add`, `cargo new`, or other
scaffolding commands were run to produce it — every `Cargo.toml` and
source file was written by hand, so the commands below are the
**first** commands you should run against this tree.

This file is a reference, not a log — every command below is
documented for you (or CI) to run manually; none have been executed
in this environment.

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
  mnemo-storage/              # SQLite schema, migrations, repositories (incl. embeddings), FTS5
  mnemo-ingest/                # txt/md/html parsing + chunking (pure, no I/O beyond reading files)
  mnemo-embeddings/             # Embedder trait + default HashingEmbedder (Phase 4)
  mnemo-search/                  # lexical (BM25), vector (cosine), hybrid fusion retrieval
  mnemo-cli/                      # `mnemo` binary — not covered by this file; see ROADMAP.md scope note
```

`mnemo-embeddings` is now a workspace member and a dependency of both
`mnemo-search` and the top-level `mnemo` facade (it previously existed
as source but wasn't wired into the workspace at all — see
`ROADMAP.md`).

## Build everything

```sh
cargo build --workspace
```

Release build:

```sh
cargo build --workspace --release
```

## Run the CLI

The CLI binary is named `mnemo` (crate `mnemo-cli`). Per project
direction, `mnemo-cli` is out of scope for the work tracked in
`ROADMAP.md` — it has **not** been extended alongside the `mnemo`
facade, so it only exposes what was already wired into
`crates/mnemo-cli/src/main.rs` before this pass: `init`, `ingest`,
`search` (lexical only, no `--mode`/vector/hybrid flags), `profile`,
`memory`, `stats`. There is **no** `embed` or `context` subcommand —
those facade features (`Mnemo::embed()`, `Mnemo::context()`) are
currently only reachable from Rust code, not the CLI.

Every command opens/creates a database file given via `--db`
(defaults to `mnemo.db` in the current directory), so there's no
separate "init" step required — but an explicit `init` command exists
too:

```sh
cargo run -p mnemo-cli -- --db mnemo.db init

# Ingest documents (.txt, .md, .html)
cargo run -p mnemo-cli -- --db mnemo.db ingest ./notes/*.md

# Search everything that's been ingested (lexical/BM25 only)
cargo run -p mnemo-cli -- --db mnemo.db search "project deadline"
cargo run -p mnemo-cli -- --db mnemo.db search --scope documents --limit 5 "quarterly report"

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
`cargo run`. See "Using `mnemo` as a library" below for
embed/vector/hybrid/context usage, which for now requires writing a
few lines of Rust rather than a CLI flag.

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

// Lexical (BM25) search — always available, no embedding step needed.
let hits = db.search().search("project deadline").await?;

// Vector / hybrid search (Phase 4/5): embed pending chunks once, then
// reuse the same embedder handle for query embedding so model/version
// match what's stored.
let embed = db.embed(); // default HashingEmbedder; use db.embed_with(..) for a real model
embed.embed_pending().await?;
let embedder = embed.embedder();

let vector_hits = db.search().search_vector(embedder.clone(), "project deadline", 10).await?;
let hybrid_hits = db
    .search()
    .search_hybrid(
        embedder.clone(),
        "project deadline",
        mnemo::SearchOptions::default(),
        mnemo::HybridWeights::default(),
    )
    .await?;

// Context packing (Phase 7): fuse + dedupe + greedily pack into a
// token budget, capped at a number of distinct sources, with full
// `Source` records attached for citation rendering.
let packed = db.context_with(embedder).pack("project deadline", 2000).await?;
for chunk in &packed.chunks {
    println!("[{} tok] {}", chunk.estimated_tokens, chunk.hit.text);
}
println!("used {} tokens across {} sources", packed.estimated_tokens, packed.sources.len());
```

`db.context()` (no embedder argument) uses the same default
`HashingEmbedder` as `db.embed()`; use `db.context_with(embedder)` to
reuse a real model's embedder instance so query and stored vectors
are comparable. Reranking (Phase 6) is not implemented yet — see
`ROADMAP.md`'s "Not started" / "Suggested next steps" sections.

## Checks

```sh
cargo check --workspace
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
cargo test --workspace
```

No tests have been written yet for `mnemo-storage` or `mnemo-ingest`
repositories (see `ROADMAP.md`), but `mnemo-embeddings` and
`mnemo-search` (lexical/vector/hybrid search plus context packing)
both include unit tests — `cargo test` will run those.

To verify just the Phase 4 (embeddings)/Phase 5 (hybrid retrieval)/
Phase 7 (context packing) work without running the whole workspace
suite:

```sh
cargo check -p mnemo-storage -p mnemo-embeddings -p mnemo-search -p mnemo
cargo test -p mnemo-search
cargo test -p mnemo-embeddings
```

None of the commands in this file have been run in this environment —
they're recorded here so the exact build/test/lint steps are known,
per project direction to document commands rather than execute them.

## Why no crates were added via `cargo add`

Per project constraints, every dependency was declared directly in
each `Cargo.toml`'s `[dependencies]` table (backed by shared version
pins in the workspace root's `[workspace.dependencies]`), rather than
via `cargo add`. If you need to add a new dependency going forward,
edit the relevant `Cargo.toml` by hand and add a matching entry under
`[workspace.dependencies]` if it should be shared, then run
`cargo build` to update `Cargo.lock` — do not run `cargo add`.
