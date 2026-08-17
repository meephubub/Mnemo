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
  mnemo-storage/              # SQLite schema, migrations, repositories (incl. chunk + message embeddings), FTS5
  mnemo-ingest/                # txt/md/html/pdf/docx parsing + chunking (pure, no I/O beyond reading files)
  mnemo-embeddings/             # Embedder trait + default HashingEmbedder (Phase 4)
  mnemo-search/                  # lexical (BM25), vector (cosine), hybrid fusion, reranking, context packing
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

# Ingest documents (.txt, .md, .html, .pdf, .docx — extension picks the parser)
cargo run -p mnemo-cli -- --db mnemo.db ingest ./notes/*.md
cargo run -p mnemo-cli -- --db mnemo.db ingest ./reports/quarterly.pdf ./memos/notice.docx

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
let packed = db.context_with(embedder.clone()).pack("project deadline", 2000).await?;
for chunk in &packed.chunks {
    println!("[{} tok] {}", chunk.estimated_tokens, chunk.hit.text);
}
println!("used {} tokens across {} sources", packed.estimated_tokens, packed.sources.len());

// Reranking (Phase 6): an optional Stage 2 over the fused candidate
// pool, run by `pack_context` before dedup/packing when a `reranker`
// is set. `mnemo::HeuristicReranker` is the default, dependency-free
// implementation (blends the Stage 1 score with query/body exact-
// phrase overlap and a title/section match boost); anything
// implementing `mnemo::Reranker` can be swapped in instead (e.g. a
// real cross-encoder model).
let reranker = std::sync::Arc::new(mnemo::HeuristicReranker::default());
let reranked_packed = db
    .context_with(embedder.clone())
    .pack_with_reranker("project deadline", 2000, reranker)
    .await?;

// Neighbor-chunk expansion (Phase 7 follow-up): after the main pack,
// pull in each selected chunk's immediate previous/next sibling from
// the same document when the token budget allows, so packed context
// isn't cut off mid-sentence at either edge.
let expanded_packed = db
    .context_with(embedder)
    .pack_with_neighbor_expansion("project deadline", 2000)
    .await?;
```

`db.context()` (no embedder argument) uses the same default
`HashingEmbedder` as `db.embed()`; use `db.context_with(embedder)` to
reuse a real model's embedder instance so query and stored vectors
are comparable. Reranking and neighbor-chunk expansion are both
opt-in per request — via `ContextHandle::pack_with_reranker` /
`pack_with_neighbor_expansion`, or `ContextRequest::with_reranker` /
`with_neighbor_expansion` (or the `ContextRequest.reranker` /
`.neighbor_expansion` fields directly when building a
`pack_with_request` call that also needs other custom options, e.g. a
non-default `token_budget`/`max_sources`/`scope`). `pack`/
`pack_with_request` without either set behave exactly as before
Phase 6/the neighbor-expansion follow-up.

## Checks

```sh
cargo check --workspace
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
cargo test --workspace
```

No dedicated tests have been written yet for `mnemo-storage` (see
`ROADMAP.md`) — its `chunks::get_by_document_and_index` (added to
support neighbor-chunk expansion) is exercised only indirectly, via
`mnemo-search`'s neighbor-expansion tests, not a `mnemo-storage`-local
test. But `mnemo-embeddings`, `mnemo-search` (lexical/vector/hybrid
search, reranking, and context packing, including neighbor expansion),
and `mnemo-ingest` (parsing for every supported format — txt/md/html/
PDF/DOCX — plus chunking and the crate-level `ingest_*` entry points)
all include unit tests — `cargo test` will run those.

To verify just the Phase 4 (embeddings)/Phase 5 (hybrid retrieval)/
Phase 6 (reranking)/Phase 7 (context packing) work without running the
whole workspace suite:

```sh
cargo check -p mnemo-storage -p mnemo-embeddings -p mnemo-search -p mnemo
cargo test -p mnemo-search
cargo test -p mnemo-embeddings
```

To verify just the PDF/DOCX ingestion work (plan.md Phase 2 follow-up):

```sh
cargo check -p mnemo-ingest -p mnemo --all-targets
cargo test -p mnemo-ingest
```

None of the commands in this file have been run in this environment —
they're recorded here so the exact build/test/lint steps are known,
per project direction to document commands rather than execute them.

## PDF/DOCX ingestion dependencies

`mnemo-ingest` added two new dependencies to parse PDF and DOCX files
(plan.md Phase 2's PDF/DOCX ingestion follow-up):

- **`pdf-extract`** — pure-Rust PDF text extraction (via `lopdf`
  underneath); no system libraries (no `poppler`, no `mupdf`)
  required. `parsers::pdf` uses its per-page extraction so each
  resulting chunk keeps an accurate 1-based `page` number
  (`Chunk::page`).
- **`zip`**, with `default-features = false, features =
  ["deflate-flate2-zlib-rs"]` — `.docx` files are ZIP containers; this
  feature set is the minimal one that can still *read* the standard
  Deflate-compressed entries real Word produces (via the pure-Rust
  `zlib-rs` backend, no system zlib), without pulling in the
  zopfli/bzip2/lzma/ppmd/AES encoders bundled in `zip`'s `default`
  features (none of which ingestion needs). `parsers::docx` reads
  `word/document.xml` (body text, paragraph-level heading styles) and
  `docProps/core.xml` (document title) directly out of the archive
  with a small dependency-free XML scan — no XML/DOM crate was added.

Both parsers are exercised by unit tests that build minimal, valid
PDF/DOCX byte fixtures entirely in Rust (`parsers::pdf::test_support`,
`parsers::docx::test_support`) — no binary fixture files are checked
into the repo, and no external tool (e.g. a real copy of Word, or a
Python PDF library) was used to generate them.

## Why no crates were added via `cargo add`

Per project constraints, every dependency was declared directly in
each `Cargo.toml`'s `[dependencies]` table (backed by shared version
pins in the workspace root's `[workspace.dependencies]`), rather than
via `cargo add`. If you need to add a new dependency going forward,
edit the relevant `Cargo.toml` by hand and add a matching entry under
`[workspace.dependencies]` if it should be shared, then run
`cargo build` to update `Cargo.lock` — do not run `cargo add`.
