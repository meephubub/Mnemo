# Mnemo
A fast, local-first memory and personal knowledge engine for AI agents, written entirely in Rust.

Mnemo gives AI agents persistent access to conversations, documents, emails, notes, and user preferences without requiring a cloud memory service. It combines semantic and keyword search with structured user profiles, knowledge extraction, and source-aware retrieval to provide agents with the right context at the right time.

Designed for local execution, low latency, privacy, and seamless integration with Rust-based AI agents.
 (name from Mnemosyne, the Greek personification of memory)

## Status

This is an early, hand-written implementation covering Phases 0-3 of
`plan.md` (workspace layout, SQLite storage, txt/md/html ingestion,
and FTS5 lexical search). See `ROADMAP.md` for exactly what's done vs.
not started yet.

## Building and running

See `build.md` for prerequisites and the exact `cargo` commands to
build the workspace and use the `mnemo` CLI/library — no scaffolding
commands (`cargo new`, `cargo add`, etc.) were used to produce this
tree, so `build.md` covers the very first commands you should run.
