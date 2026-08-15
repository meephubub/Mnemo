Mnemo — Detailed Development Plan

1. Project Overview

Mnemo is a local-first, high-performance personal memory and knowledge engine for AI agents, written entirely in Rust.

Mnemo combines:

* Personal user profiles
* Long-term agent memory
* Conversations
* Documents
* Emails
* Notes
* Semantic search
* Full-text/BM25 search
* Hybrid retrieval
* Reranking
* Context packing
* Entity extraction
* Relationship/knowledge graphs
* Temporal memory
* Confidence scoring
* Contradiction detection
* Memory lifecycle management
* Source provenance and citations
* Incremental indexing
* Background ingestion
* Local embeddings
* Optional local LLM processing
* MCP/API access
* Evaluation and benchmarking

Mnemo is designed to sit underneath an AI agent and provide the agent with the smallest, highest-quality set of relevant information required to complete a task.

The primary target architecture is:

User
 |
 v
Agent
 |
 +------------------+
 |                  |
 v                  v
Needle 2          Mnemo
Router            Memory / Knowledge
 |                  |
 |        +---------+---------+
 |        |         |         |
 |     Profile   Documents  Conversations
 |                  |
 |          +-------+-------+
 |          |       |       |
 |       Semantic  FTS    Graph
 |          |       |       |
 |          +-------+-------+
 |                  |
 |              Reranking
 |                  |
 |           Context Packing
 |                  |
 +---------> Gemma 4
                 |
                 v
             Tool Calls

⸻

2. Goals

Primary Goals

G1 — Local-first

All core functionality must work without a cloud service.

Mnemo should be capable of running entirely on a user’s machine.

G2 — Rust-native

The core system must be implemented entirely in Rust.

Avoid Python dependencies in the core runtime.

G3 — High-performance retrieval

Retrieval should be fast enough to be used interactively by an AI agent.

Target:

* <50 ms for local lexical retrieval
* <100 ms for typical hybrid retrieval where practical
* Minimal unnecessary model inference
* Incremental indexing
* Background processing

Exact targets should be established through benchmarks rather than assumed.

G4 — High-quality retrieval

Mnemo should outperform simple vector RAG by combining:

* Semantic search
* Full-text search
* Metadata filtering
* Entity search
* Relationship expansion
* Temporal filtering
* Reranking
* Context packing

G5 — Persistent personal context

Mnemo should allow an agent to remember:

* User identity
* Preferences
* Interests
* Current projects
* Long-term facts
* Historical facts
* Previous conversations

G6 — Personal knowledge base

Mnemo should ingest and search:

* PDFs
* Markdown
* Plain text
* DOCX
* HTML
* Emails
* Conversations
* Web pages
* Images eventually
* Structured data eventually

G7 — Source-grounded answers

Every retrieved piece of information should retain provenance.

The agent should be able to determine:

* Where information came from
* What document it came from
* Which page/section/message
* When it was created
* When it was indexed
* Whether it was inferred or explicitly stated

G8 — Autonomous memory management

The agent should be able to:

* Add memories
* Update memories
* Supersede memories
* Delete memories
* Update the user profile
* Detect contradictions
* Assign confidence
* Determine whether information should become long-term memory

G9 — Agent integration

Mnemo should expose a clean Rust API and eventually:

* REST API
* OpenAI-compatible retrieval interface where useful
* MCP server
* CLI

⸻

3. Non-Goals

Mnemo should NOT initially attempt to become:

* A general-purpose vector database
* A full AI agent framework
* A cloud-scale distributed database
* A general-purpose document editor
* A replacement for an LLM
* A replacement for Needle 2
* A replacement for an embedding model
* A replacement for an email client
* A full enterprise knowledge-management platform

Mnemo is the persistent context layer for agents.

⸻

4. High-Level Architecture

+----------------------------------------------------------+
|                        AI AGENT                          |
+----------------------------------------------------------+
|                                                          |
|  +------------+             +-------------------------+  |
|  | Needle 2   |             | Gemma / Reasoning LLM |  |
|  | Tool Router|             +-------------------------+  |
|  +------+-----+                         ^                |
|         |                               |                |
|         +-------------------------------+                |
|                         |                                |
|                         v                                |
|                  +-------------+                         |
|                  |    Mnemo    |                         |
|                  +-------------+                         |
|                         |                                |
+-------------------------+--------------------------------+
                          |
              +-----------+-----------+
              |           |           |
              v           v           v
           Profile    Knowledge    Conversations
                         Base
              |           |           |
              +-----------+-----------+
                          |
                 +--------+--------+
                 |                 |
                 v                 v
              SQLite        Vector Storage
                 |                 |
                 +--------+--------+
                          |
                   Retrieval Engine
                          |
              +-----------+-----------+
              |           |           |
              v           v           v
           Semantic     Full Text   Knowledge
            Search       Search      Graph
              |           |           |
              +-----------+-----------+
                          |
                       Reranker
                          |
                   Context Packer
                          |
                          v
                       Results

⸻

5. Core Components

5.1 Storage Layer

Use SQLite as the primary relational store.

Responsibilities:

* Metadata
* Documents
* Chunks
* Conversations
* Messages
* Profile
* Memories
* Entities
* Relationships
* Sources
* Ingestion state
* Embedding metadata
* Retrieval metadata
* Evaluation data

Potential Rust libraries:

* rusqlite
* sqlx

Choose one and standardize on it.

SQLite should be the source of truth for metadata.

⸻

6. Vector Storage

Use an embedded/local vector database or index.

Initial candidate:

* LanceDB

Alternative:

* usearch
* hnswlib bindings if appropriate
* Custom HNSW implementation later

Requirements:

* Local execution
* Persistent indexes
* Fast nearest-neighbour search
* Incremental insertion
* Deletion
* Metadata association
* Multiple embedding models/versioning

Vector records should reference SQLite IDs rather than duplicate large metadata structures.

⸻

7. Full-Text Search

Implement lexical search independently of vector search.

Initial implementation:

* SQLite FTS5

Store searchable:

* Document text
* Chunk text
* Conversation messages
* Email subject
* Email body
* Entity names
* Tags

FTS should support:

* Exact words
* Phrase matching
* Prefix matching where useful
* Ranking
* Metadata filtering

⸻

8. Hybrid Retrieval

Hybrid retrieval is a core feature.

A query should be able to search:

Semantic similarity
+
Full-text relevance
+
Metadata
+
Entities
+
Temporal information
+
Relationships

Example:

Query
 |
 +----> Vector Search
 |
 +----> FTS5
 |
 +----> Entity Search
 |
 +----> Metadata Filters
 |
 +----> Temporal Filters
 |
 v
Candidate Pool
 |
 v
Score Fusion
 |
 v
Reranker
 |
 v
Final Results

Possible initial score:

final_score =
    semantic_weight * semantic_score
  + lexical_weight * lexical_score
  + entity_weight * entity_score
  + recency_weight * recency_score
  + importance_weight * importance_score

Weights must be configurable.

Eventually support:

* Reciprocal Rank Fusion
* Weighted score fusion
* Learned ranking

⸻

9. Query Understanding

Before retrieval, classify the query.

Potential query types:

FACTUAL
DOCUMENT
CONVERSATION
PROFILE
TEMPORAL
RELATIONAL
NAVIGATIONAL
GENERAL

Examples:

"What is the definition of GDP?"
→ DOCUMENT
"What did we decide yesterday?"
→ CONVERSATION + TEMPORAL
"What is my name?"
→ PROFILE
"What tools does my agent use?"
→ RELATIONAL
"What did my teacher email me?"
→ EMAIL + DOCUMENT

Query classification should be cheap.

Prefer:

1. Rules/heuristics
2. Needle 2
3. Small local classifier
4. LLM only when necessary

⸻

10. Reranking

Retrieval should use a two-stage architecture.

Stage 1:
Cheap candidate generation
Vector + FTS + Graph
        |
        v
~50 candidates
Stage 2:
Expensive reranking
Reranker
        |
        v
~5-15 candidates

Support local reranking models.

Potential models:

* BGE reranker family
* Jina rerank models
* Other ONNX-compatible rerankers

The reranker should be optional.

⸻

11. Context Packing

Context packing is a first-class feature.

Input:

ContextRequest {
    query: String,
    token_budget: usize,
    max_sources: usize,
}

Output:

Context {
    chunks: Vec<ContextChunk>,
    estimated_tokens: usize,
    sources: Vec<Source>,
}

The packer should:

* Rank candidates
* Remove duplicates
* Avoid redundant chunks
* Respect token budget
* Prefer diverse sources
* Preserve surrounding context where needed
* Preserve citations

Example:

Token budget: 2500
Chunk A = 800 tokens
Chunk B = 500 tokens
Chunk C = 600 tokens
Chunk D = 900 tokens
A + B + C = 1900
A + B + D = 2200
Select A + B + D

⸻

12. Document Model

Define a canonical document structure.

Document {
    id: DocumentId,
    source_id: SourceId,
    title: Option<String>,
    mime_type: String,
    created_at: Option<DateTime>,
    modified_at: Option<DateTime>,
    indexed_at: DateTime,
    content_hash: String,
    parser_version: String,
    embedding_version: String,
}

⸻

13. Chunking

Implement configurable chunking.

Support:

* Fixed token chunks
* Sentence-aware chunks
* Paragraph-aware chunks
* Markdown section chunks
* PDF section chunks
* Email-aware chunks
* Conversation-aware chunks

Chunk metadata:

Chunk {
    id: ChunkId,
    document_id: DocumentId,
    text: String,
    start_offset: usize,
    end_offset: usize,
    page: Option<u32>,
    section: Option<String>,
}

Avoid excessive overlap.

Chunking should be content-type aware.

⸻

14. Document Ingestion

Create an ingestion pipeline:

Input
 |
 v
Identify type
 |
 v
Parse
 |
 v
Normalize
 |
 v
Chunk
 |
 v
Extract metadata
 |
 v
Generate embeddings
 |
 v
Extract entities/events
 |
 v
Index

Support initial formats:

1. TXT
2. Markdown
3. HTML
4. PDF
5. DOCX
6. Email
7. JSON
8. Conversation JSON

⸻

15. Incremental Indexing

Every source should have a content hash.

File
 |
 v
Hash
 |
 +---- same ----> Skip
 |
 +---- changed -> Re-index

Track:

* Content hash
* Parser version
* Chunker version
* Embedding model
* Embedding version
* Entity extraction version

Changing any processing version should allow selective reprocessing.

⸻

16. File Watching

Support automatic indexing of directories.

Example:

~/Documents
~/School
~/Projects

File watcher:

New file
   ↓
Queue
   ↓
Parse
   ↓
Index

Use a background queue.

Do not block retrieval while indexing.

⸻

17. Background Job System

Create an internal job queue.

Jobs:

INGEST_DOCUMENT
REINDEX_DOCUMENT
EMBED_CHUNK
EXTRACT_ENTITIES
EXTRACT_EVENTS
UPDATE_GRAPH
REBUILD_INDEX
DELETE_SOURCE

Jobs should have:

* Priority
* Retry count
* Status
* Error information
* Created timestamp

Interactive retrieval should receive higher priority than background ingestion.

⸻

18. Conversation Storage

Store all agent conversations.

Conversation
 |
 +-- Message
 +-- Message
 +-- Message

Messages should contain:

* Role
* Content
* Timestamp
* Conversation ID
* Tool calls
* Tool results
* Metadata

Conversation history should be independently searchable.

⸻

19. Conversation Indexing

After a conversation:

Conversation
 |
 +--> Store raw messages
 |
 +--> Chunk
 |
 +--> Embed
 |
 +--> FTS
 |
 +--> Entity extraction
 |
 +--> Event extraction
 |
 +--> Memory candidate extraction

Don’t put the entire conversation into the user’s permanent profile.

⸻

20. User Profile

Create a small structured profile.

Example:

{
  "name": "Samuel",
  "preferred_language": "English",
  "response_style": "concise",
  "units": "metric",
  "current_projects": [
    "Mnemo",
    "AI agent"
  ]
}

Profile fields should remain small.

Profile is intended for information that is:

* Stable
* Frequently useful
* User-specific
* Appropriate to inject into prompts

⸻

21. Profile Updates

Allow the agent to propose profile updates.

Tools:

get_profile
update_profile
remove_profile

Example:

User:
"I prefer concise answers."
Gemma
 |
 v
update_profile(
    key="response_style",
    value="concise"
)

Rust validates and commits the change.

⸻

22. Profile Update Rules

Profile updates should have confidence.

Explicit user statement
→ confidence 1.0
Strong inference
→ confidence ~0.7
Weak inference
→ confidence ~0.4

Suggested policy:

>= 0.85
Automatically save
0.50 - 0.84
Temporary candidate
< 0.50
Do not save

Sensitive information should have stricter rules.

⸻

23. Memory Model

Distinguish between:

Profile
Memory
Document
Conversation
Event
Entity
Relationship

A memory could contain:

Memory {
    id: MemoryId,
    content: String,
    type: MemoryType,
    confidence: f32,
    importance: f32,
    created_at: DateTime,
    last_accessed: DateTime,
    valid_from: Option<DateTime>,
    valid_until: Option<DateTime>,
    source_id: Option<SourceId>,
}

⸻

24. Memory Types

Initial types:

FACT
PREFERENCE
INTEREST
GOAL
PROJECT
PERSON
LOCATION
ROUTINE
DECISION
EVENT
TEMPORARY

⸻

25. Memory Lifecycle

Implement:

CANDIDATE
   ↓
ACTIVE
   ↓
SUPERSEDED
   ↓
ARCHIVED

Temporary memories may:

CANDIDATE
   ↓
TEMPORARY
   ↓
EXPIRE

Do not delete historical evidence when a memory becomes obsolete.

⸻

26. Memory Importance

Each memory should have an importance score.

Example:

importance = 1.0
"I prefer concise answers."
importance = 0.2
"I am going to the library today."

Importance influences:

* Retrieval
* Context packing
* Memory retention
* Profile promotion

⸻

27. Recency

Retrieval should consider recency where appropriate.

Do not universally favour newer information.

For example:

"What is my name?"

should not care about recency.

But:

"What project am I currently working on?"

should.

Implement query-dependent recency weighting.

⸻

28. Temporal Memory

Facts should support:

valid_from
valid_until
created_at
updated_at
last_confirmed

Example:

Python
valid_from: 2025-01
valid_until: 2026-07
Rust
valid_from: 2026-07
valid_until: NULL

Mnemo can therefore answer:

“What did I use before Rust?”

⸻

29. Contradiction Detection

When adding a new memory:

New memory
 |
 v
Search related memories
 |
 v
Potential contradiction?
 |
 +---- no ----> Store
 |
 +---- yes
       |
       v
Compare timestamps/confidence
       |
       v
Mark previous memory as superseded

Example:

"I use Python."
Later:
"I've switched to Rust."

Result:

Python
status = SUPERSEDED
Rust
status = ACTIVE

⸻

30. Entity Extraction

Extract entities from documents, conversations and memories.

Initial entity types:

PERSON
ORGANIZATION
PROJECT
PLACE
PRODUCT
SOFTWARE
CONCEPT
SUBJECT
EVENT
DATE

Each entity should have:

* ID
* Canonical name
* Type
* Aliases
* Mentions
* Confidence

⸻

31. Entity Resolution

Recognise:

"Google"
"Google LLC"
"Google AI"

as potentially related entities.

Likewise:

"Gemma"
"Gemma 4"
"Google Gemma"

should not automatically become unrelated entities.

Entity resolution should be conservative.

⸻

32. Relationships

Store relationships:

Entity A
   |
 relationship
   |
Entity B

Examples:

Samuel
  └── works_on → Mnemo
Mnemo
  └── written_in → Rust
Mnemo
  └── uses → Gemma
Gemma
  └── produced_by → Google

Relationships should contain:

* Type
* Confidence
* Source
* Timestamp
* Validity period

⸻

33. Graph Retrieval

Graph retrieval should supplement semantic search.

Example:

Query:
"What models am I using in my AI agent?"
Semantic search
+
Entity search for "AI agent"
+
Graph traversal

Return:

Mnemo
 ├── uses → Gemma 4
 └── uses → Needle 2

Do not make graph traversal the default for every query.

⸻

34. Event Extraction

Extract events such as:

User started project X
User changed model
User bought product
Teacher sent email
Assignment deadline changed

Events should contain:

event_type
timestamp
participants
entities
source
confidence

⸻

35. Source Provenance

Every piece of knowledge must retain provenance.

Sources may be:

FILE
EMAIL
CONVERSATION
WEBPAGE
PROFILE
INFERENCE
USER_STATEMENT

Example:

Source {
    id: SourceId,
    source_type: SourceType,
    name: String,
    uri: Option<String>,
    created_at: Option<DateTime>,
}

⸻

36. Citation System

Retrieval results should include enough information to generate citations.

For a PDF:

Source:
Economics Notes.pdf
Page:
42
Section:
Monetary Policy
Chunk:
...

For email:

Source:
Email
From:
teacher@example.com
Date:
2026-08-12
Subject:
Economics coursework

For conversation:

Conversation:
Agent Architecture
Date:
2026-08-14

⸻

37. Source Reliability

Assign reliability.

Example:

USER_STATEMENT = 1.0
OFFICIAL_DOCUMENT = 1.0
EMAIL = 0.95
CONVERSATION = 0.85
INFERENCE = 0.50

These values should be configurable.

⸻

38. Deduplication

Deduplicate:

* Documents
* Chunks
* Memories
* Entities
* Conversations
* Embeddings

Use:

* Content hashes
* Normalized text
* Similarity checks
* Entity resolution

Avoid storing the same information repeatedly.

⸻

39. Semantic Deduplication

Two memories:

"I prefer short answers."
"I like concise responses."

should potentially become one memory:

User prefers concise responses.

Do this asynchronously.

⸻

40. Retrieval Diversity

Avoid returning ten nearly identical chunks.

Use diversity-aware selection.

Potential approach:

MMR
(Maximal Marginal Relevance)

This should improve context quality.

⸻

41. Metadata Filtering

Support filters such as:

source_type = EMAIL
date > 2026-01-01
project = Mnemo
subject = Economics
person = Teacher

Filtering should happen as early as possible.

⸻

42. Query Scopes

Support explicit scopes:

ALL
PROFILE
MEMORY
DOCUMENTS
EMAILS
CONVERSATIONS
PROJECT
ENTITY

Example:

mnemo.search(query)
    .scope(SearchScope::Conversations)

⸻

43. Context Profiles

Different agents/tasks may require different retrieval strategies.

Examples:

STUDENT_ASSISTANT
CODING_ASSISTANT
RESEARCH
GENERAL

Each profile can define:

* Search weights
* Token budget
* Recency
* Reranking
* Graph expansion
* Source priorities

⸻

44. Agent Tools

Expose tools:

search_memory
search_documents
search_conversations
search_emails
get_source
get_profile
update_profile
save_memory
update_memory
forget_memory

Keep destructive operations restricted.

⸻

45. Needle 2 Integration

Mnemo should not replace Needle 2.

Needle should decide:

User query
 |
 v
Needle 2
 |
 +--> Mnemo search
 |
 +--> Tool
 |
 +--> Direct response

Mnemo can expose a small number of high-level capabilities so Needle doesn’t need hundreds of memory-specific tools.

Preferred primary interface:

search_knowledge

with optional:

search_profile
search_conversations
get_source

⸻

46. Gemma Integration

Gemma should receive:

System prompt
+
Small user profile
+
Retrieved context
+
Relevant source metadata
+
Relevant tool definitions
+
Current conversation

Avoid injecting the entire Mnemo database.

⸻

47. Local Embedding Models

Create an embedding abstraction:

trait Embedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

Support:

* ONNX
* Candle
* LiteRT eventually
* Remote API optionally

Embedding models should be replaceable.

⸻

48. Local Reranker

Create:

trait Reranker {
    async fn rerank(
        &self,
        query: &str,
        documents: &[Document]
    ) -> Result<Vec<RankedDocument>>;
}

Allow reranking to be disabled for low-latency queries.

⸻

49. Model Versioning

Every embedding/indexed item must record:

model_name
model_version
embedding_dimension

This prevents incompatible vector indexes.

⸻

50. Encryption

Because Mnemo may contain:

* Emails
* Schoolwork
* Personal conversations
* User profile
* Private documents

support encrypted storage.

Potential approach:

SQLite
+
SQLCipher or application-level encryption

Sensitive fields may also use application-level encryption.

⸻

51. Authentication

Local mode should be secure by default.

If an API server is enabled:

* API key authentication
* Optional local-only binding
* Optional TLS
* Optional Tailscale/private-network mode

Never expose the database directly.

⸻

52. Privacy

Provide clear controls:

forget_memory
delete_source
delete_conversation
clear_profile
wipe_database

Users should be able to inspect what Mnemo knows.

⸻

53. Memory Inspection

Create CLI commands:

mnemo profile
mnemo memories list
mnemo memories search "..."
mnemo source show <id>
mnemo entities list
mnemo graph <entity>

This makes debugging dramatically easier.

⸻

54. Explainability

Every retrieval result should be inspectable.

For example:

Result score: 0.91
Semantic: 0.88
Lexical: 0.93
Entity: 0.75
Recency: 0.62
Source:
Economics coursework.pdf
Page 32

This is extremely useful during development.

⸻

55. Evaluation Framework

Build evaluation into Mnemo itself.

Metrics:

Recall@1
Recall@5
Recall@10
MRR
nDCG
Precision@K
Latency
Tokens retrieved
Context utilization

Evaluate separately:

* Vector search
* FTS
* Hybrid search
* Reranking
* Graph retrieval
* Context packing
* Memory extraction

⸻

56. Personal Evaluation Dataset

Create a private dataset representing real use.

Example:

Question
Expected sources
Expected entities
Expected answer facts

Categories:

Document retrieval
Conversation retrieval
Profile retrieval
Temporal retrieval
Relationship retrieval
Email retrieval
Multi-hop retrieval

Run it after every major change.

⸻

57. Benchmark Comparisons

Eventually compare Mnemo against:

* Simple vector RAG
* BM25
* Hybrid RAG
* SAG
* Supermemory where practical
* Other open-source systems

Focus on:

Accuracy
Latency
Memory usage
Disk usage
Token usage
Indexing speed

Do not rely exclusively on vendor-published benchmarks.

⸻

58. Performance Targets

Initial targets:

SQLite metadata lookup:       <5 ms
FTS query:                    <20 ms
Vector search:                <50 ms
Hybrid retrieval:             <100 ms
Simple profile retrieval:     <1 ms
Context packing:              <10 ms

These are targets, not guarantees.

Benchmark on realistic hardware.

⸻

59. Memory Usage

Mnemo should minimize unnecessary duplication.

Prefer:

SQLite metadata
+
vector index
+
source files

rather than copying entire source documents multiple times.

⸻

60. Async Architecture

Use asynchronous tasks for:

* Ingestion
* Embedding
* Entity extraction
* Event extraction
* Graph updates
* Deduplication
* Memory consolidation

Retrieval should remain synchronous/low-latency from the agent’s perspective where practical.

⸻

61. Caching

Cache:

* Embeddings
* Recent queries
* Profile
* Frequently accessed entities
* Frequently retrieved sources
* Reranker results where safe

Use bounded caches.

⸻

62. API Design

Core API:

let mnemo = Mnemo::open("./mnemo")?;
mnemo.ingest(document).await?;
let results = mnemo
    .search("What did my teacher say about my coursework?")
    .await?;
let profile = mnemo.profile().await?;

Memory:

mnemo.memory()
    .save(memory)
    .await?;

Profile:

mnemo.profile()
    .set("response_style", "concise")
    .await?;

⸻

63. Crate Structure

Suggested workspace:

mnemo/
├── Cargo.toml
├── crates/
│   ├── mnemo-core/
│   ├── mnemo-storage/
│   ├── mnemo-search/
│   ├── mnemo-ingest/
│   ├── mnemo-embeddings/
│   ├── mnemo-rerank/
│   ├── mnemo-memory/
│   ├── mnemo-graph/
│   ├── mnemo-profile/
│   ├── mnemo-api/
│   ├── mnemo-mcp/
│   └── mnemo-cli/
├── models/
├── tests/
├── benchmarks/
└── docs/

⸻

64. Core Data Model

Main entities:

Source
Document
Chunk
Conversation
Message
Memory
ProfileEntry
Entity
Relationship
Event
Embedding
IngestionJob

Relationships:

Source
 └── Document
      └── Chunk
           ├── Embedding
           ├── EntityMention
           └── Event
Conversation
 └── Message
      ├── Chunk
      ├── EntityMention
      ├── MemoryCandidate
      └── Event
Entity
 └── Relationship
      └── Entity

⸻

65. CLI

Initial commands:

mnemo init
mnemo ingest ./Documents
mnemo ingest-file document.pdf
mnemo search "coursework deadline"
mnemo profile
mnemo profile set name Samuel
mnemo memories list
mnemo memories search "Rust"
mnemo sources list
mnemo sources show <id>
mnemo entities search "Gemma"
mnemo graph "Mnemo"
mnemo stats
mnemo reindex
mnemo doctor

⸻

66. API Server

Eventually provide:

POST /search
POST /ingest
GET  /profile
PATCH /profile
GET  /memories
POST /memories
PATCH /memories/:id
DELETE /memories/:id
GET  /sources/:id
GET  /entities/:id
GET  /health
GET  /stats

⸻

67. MCP Server

Provide MCP tools:

mnemo_search
mnemo_search_documents
mnemo_search_conversations
mnemo_get_source
mnemo_get_profile
mnemo_update_profile
mnemo_save_memory

This allows external agents to use Mnemo.

⸻

68. Web UI

Optional later.

The UI should show:

Profile
Memories
Sources
Documents
Conversations
Entities
Relationships
Search

Especially useful:

"What does Mnemo know about me?"

with every fact linked to its source.

⸻

69. Observability

Record:

* Query latency
* Retrieval method
* Candidate count
* Reranking latency
* Context token count
* Cache hits
* Embedding latency
* Indexing throughput
* Errors

Expose:

mnemo stats

and optionally structured logs.

⸻

70. Error Handling

All ingestion should be fault tolerant.

One corrupt PDF must not stop the entire ingestion queue.

Jobs should record:

status
attempts
error
timestamp

Support retry.

⸻

71. Security Boundaries

Profile updates and destructive memory operations should be treated as privileged operations.

Potential levels:

READ
WRITE
DELETE
ADMIN

The agent should not automatically be able to:

wipe database
delete all memories
export all private data

without explicit user approval.

⸻

72. Sensitive Information

Mnemo should allow source-level sensitivity:

PUBLIC
PRIVATE
SENSITIVE

Sensitive sources should optionally be excluded from:

* automatic profile extraction
* cloud APIs
* external model processing

⸻

73. Offline Mode

Mnemo must support:

Offline:
- Search
- Profile
- Memory
- Local embeddings
- Local reranking
- Local ingestion

No network dependency should exist for core functionality.

⸻

74. Cloud Optionality

Cloud services should be optional adapters:

EmbeddingProvider
├── Local
├── OpenAI
├── Gemini
└── Other
LLMProvider
├── Local
├── Gemini
└── Other

The core database/retrieval engine must not depend on them.

⸻

75. Recommended Technology Stack

Initial:

Language:
Rust
Database:
SQLite
Vector:
LanceDB or equivalent embedded vector index
Full text:
SQLite FTS5
Async:
Tokio
Serialization:
Serde
HTTP:
Axum
HTTP client:
Reqwest
CLI:
Clap
File watching:
Notify
IDs:
UUID
Dates:
Chrono / time
Logging:
Tracing
Testing:
Cargo test
Benchmarking:
Criterion

⸻

76. Development Phases

Phase 0 — Architecture

* Create Cargo workspace
* Define core traits
* Define IDs
* Define database schema
* Define source model
* Define retrieval result model
* Define profile model
* Define memory model

Deliverable:

Compiles
+
Database opens
+
Basic CRUD works

⸻

77. Phase 1 — Basic Storage

* SQLite integration
* Sources
* Documents
* Chunks
* Conversations
* Messages
* Profile
* Memories
* Metadata
* Migrations

Deliverable:

mnemo ingest
mnemo profile
mnemo memories

⸻

78. Phase 2 — Document Ingestion

* TXT
* Markdown
* HTML
* PDF
* DOCX
* Basic email
* Content hashing
* Chunking
* Metadata extraction

Deliverable:

A directory can be indexed.

⸻

79. Phase 3 — Full-Text Search

* SQLite FTS5
* Search API
* Ranking
* Filtering
* Source metadata
* Search benchmarks

Deliverable:

mnemo search "economics coursework"

⸻

80. Phase 4 — Embeddings

* Embedding trait
* Local embedding model
* Vector index
* Persistent vectors
* Model versioning
* Embedding cache
* Vector search

Deliverable:

Semantic search works locally.

⸻

81. Phase 5 — Hybrid Retrieval

* Vector retrieval
* FTS retrieval
* Score fusion
* Metadata filtering
* Candidate deduplication
* Retrieval configuration
* Benchmarks

Deliverable:

Hybrid retrieval beats either system alone.

⸻

82. Phase 6 — Reranking

* Reranker abstraction
* Local reranker
* Candidate pipeline
* Reranking benchmarks
* Optional fast path

Deliverable:

High-quality top-K retrieval.

⸻

83. Phase 7 — Context Packing

* Token counting
* Token budgets
* Duplicate removal
* Diversity selection
* Source diversity
* Citation preservation

Deliverable:

mnemo.context(query, token_budget)

⸻

84. Phase 8 — Conversation Memory

* Conversation ingestion
* Message indexing
* Conversation chunking
* Conversation retrieval
* Conversation source citations
* Automatic indexing after conversations

Deliverable:

"What did we discuss last week?"

works reliably.

⸻

85. Phase 9 — User Profile

* Profile database
* Profile retrieval
* Profile update API
* Profile validation
* Confidence
* Importance
* Automatic update candidates
* Profile inspection

Deliverable:

Agent learns stable user preferences.

⸻

86. Phase 10 — Memory Lifecycle

* Memory types
* Importance
* Confidence
* Recency
* Expiration
* Superseding
* Archiving
* Deduplication

Deliverable:

Memories remain clean over time.

⸻

87. Phase 11 — Temporal Memory

* Validity periods
* Events
* Temporal retrieval
* Historical queries
* Current-vs-historical state
* Temporal contradiction handling

Deliverable:

"What did I use before Rust?"

works correctly.

⸻

88. Phase 12 — Entity Extraction

* Entity model
* Entity mentions
* Entity extraction
* Entity resolution
* Aliases
* Entity search

Deliverable:

"What documents mention Gemma?"

works through entities and text retrieval.

⸻

89. Phase 13 — Knowledge Graph

* Relationships
* Relationship confidence
* Graph storage
* Graph traversal
* Entity neighbourhood retrieval
* Graph + semantic hybrid retrieval

Deliverable:

"What models are connected to my AI agent?"

can be answered through relationships.

⸻

90. Phase 14 — Contradictions

* Related-memory detection
* Conflict detection
* Confidence comparison
* Temporal resolution
* Superseded memories
* Contradiction inspection

Deliverable:

Old facts remain historical.
Current facts remain current.

⸻

91. Phase 15 — Background Processing

* Job queue
* Worker pool
* Priority
* Retry
* File watcher
* Incremental indexing
* Background entity extraction
* Background deduplication

Deliverable:

Drop files into folder.
Mnemo automatically processes them.

⸻

92. Phase 16 — Provenance and Citations

* Source model
* Source hierarchy
* Page metadata
* Email metadata
* Conversation metadata
* Citation formatter
* Source inspection

Deliverable:

Every factual result can be traced to its source.

⸻

93. Phase 17 — Agent Integration

* Rust API
* Search tools
* Memory tools
* Profile tools
* Needle 2 integration
* Gemma context generation
* Tool approval integration

Deliverable:

Mnemo becomes the agent's memory layer.

⸻

94. Phase 18 — API/MCP

* Axum API
* Authentication
* REST endpoints
* MCP server
* Health endpoint
* Metrics
* Documentation

Deliverable:

Any compatible agent can use Mnemo.

⸻

95. Phase 19 — Security

* Database encryption
* API authentication
* Sensitive source handling
* Permissions
* Secure deletion
* Backup/restore

Deliverable:

Private personal data can safely live inside Mnemo.

⸻

96. Phase 20 — Evaluation

* Retrieval dataset
* Profile dataset
* Conversation dataset
* Temporal dataset
* Graph dataset
* Retrieval metrics
* Latency metrics
* Token metrics
* Regression tests

Deliverable:

Every retrieval improvement is measurable.

⸻

97. Phase 21 — Optimization

* Profiling
* Allocation reduction
* Parallel retrieval
* Cache optimization
* SIMD where useful
* Index tuning
* Database tuning
* Batch embeddings
* Background scheduling

Deliverable:

Fast enough for real-time agent usage.

⸻

98. Phase 22 — Multimodal Knowledge

Future:

* Image ingestion
* OCR
* Image embeddings
* Table extraction
* Diagram understanding
* Multimodal retrieval
* Audio transcription
* Video metadata

Potential use:

"What was that diagram in my physics notes?"

⸻

99. Phase 23 — Connectors

Future connectors:

Google Drive
OneDrive
Dropbox
Gmail
Outlook
Notion
GitHub
Local filesystem
Browser history/bookmarks
Calendar

Connectors should be plugins/adapters rather than core dependencies.

⸻

100. Recommended Initial Scope

Do not implement everything simultaneously.

The first genuinely useful version should contain:

SQLite
+
FTS5
+
Vector Search
+
Hybrid Retrieval
+
Reranking
+
Context Packing
+
PDF/Markdown/TXT ingestion
+
Conversation storage
+
User Profile
+
Memory Updates
+
Provenance

Then add:

Temporal Memory
+
Entities
+
Relationships
+
Contradiction Detection

Then:

Background ingestion
+
Connectors
+
MCP
+
Encryption
+
Multimodal

⸻

101. Ideal Final Architecture

                         +----------------+
                         |      USER      |
                         +-------+--------+
                                 |
                                 v
                         +---------------+
                         |   AI AGENT    |
                         +-------+-------+
                                 |
                +----------------+----------------+
                |                                 |
                v                                 v
          +-----------+                     +-----------+
          | Needle 2  |                     |  Gemma 4  |
          |  Router   |                     | Reasoning |
          +-----+-----+                     +-----+-----+
                |                                 ^
                |                                 |
                +----------------+----------------+
                                 |
                                 v
                       +--------------------+
                       |       Mnemo        |
                       |                    |
                       | Personal Context  |
                       | Engine             |
                       +---------+----------+
                                 |
          +----------------------+----------------------+
          |                      |                      |
          v                      v                      v
      +--------+            +----------+           +---------+
      |Profile |            | Memories |           | Sources |
      +--------+            +----------+           +---------+
                                                        |
                                  +---------------------+
                                  |
              +------------------+------------------+
              |                  |                  |
              v                  v                  v
        +-----------+      +-----------+      +-----------+
        | Documents |      |   Email   |      |Conversations|
        +-----------+      +-----------+      +-----------+
              |                  |                  |
              +------------------+------------------+
                                 |
                                 v
                      +----------------------+
                      |   Ingestion Engine   |
                      +----------+-----------+
                                 |
                +----------------+----------------+
                |                |                |
                v                v                v
            Chunking         Embeddings       Extraction
                                |                |
                                |          +-----+------+
                                |          |            |
                                |       Entities      Events
                                |          |
                                |      Relationships
                                |          |
                                +----------+
                                      |
                                      v
                         +------------------------+
                         |    Retrieval Engine    |
                         +-----------+------------+
                                     |
              +----------------------+----------------------+
              |                      |                      |
              v                      v                      v
        +-----------+          +-----------+          +-----------+
        | Semantic  |          |   FTS5    |          |   Graph   |
        |  Search   |          |  Search   |          | Retrieval |
        +-----------+          +-----------+          +-----------+
              |                      |                      |
              +----------------------+----------------------+
                                     |
                                     v
                              +-------------+
                              |   Fusion    |
                              +------+------+
                                     |
                                     v
                              +-------------+
                              |  Reranking  |
                              +------+------+
                                     |
                                     v
                            +----------------+
                            | Context Packer |
                            +-------+--------+
                                    |
                                    v
                                 Gemma 4
                                    |
                                    v
                              Agent Response

⸻

102. Success Criteria

Mnemo is successful when the agent can answer questions such as:

"What did my teacher say about my coursework?"

and retrieve the correct email.

"What did we decide about the architecture of my agent?"

and retrieve the relevant previous conversations.

"What am I currently working on?"

using the profile + current memories.

"What did I use before Rust?"

using temporal memory.

"What projects am I using Gemma in?"

using entity/relationship retrieval.

"Find the notes where I discussed monetary policy."

using hybrid document retrieval.

"What do you know about me?"

using the structured profile and long-term memory.

And the system should be able to provide:

Answer
+
Relevant context
+
Source citations
+
Confidence

without sending the entire knowledge base to the LLM.

⸻

103. Core Design Principle

Mnemo should optimize for:

Maximum useful context
----------------------
Minimum tokens

The goal is not to retrieve everything that might be relevant.

The goal is to retrieve the smallest set of high-confidence information that allows the agent to answer correctly.

Mnemo should therefore act as a context compiler:

Raw personal data
       |
       v
   Index / Graph
       |
       v
   Query understanding
       |
       v
 Hybrid retrieval
       |
       v
   Reranking
       |
       v
 Deduplication
       |
       v
 Temporal resolution
       |
       v
 Context packing
       |
       v
Small, high-quality context
       |
       v
     Gemma

This is the core purpose of Mnemo.