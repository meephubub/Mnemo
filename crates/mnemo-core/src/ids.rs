//! Strongly-typed identifiers for every core entity in the data model
//! (plan.md, section 64 "Core Data Model").
//!
//! Each ID wraps a UUID so entities can never be accidentally mixed up
//! at compile time (e.g. passing a `ChunkId` where a `DocumentId` is
//! expected), while still storing/round-tripping cleanly as `TEXT` in
//! SQLite and as JSON strings.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Generate a new random (v4) identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wrap an existing UUID (e.g. one read back from storage).
            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }
    };
}

define_id!(SourceId);
define_id!(DocumentId);
define_id!(ChunkId);
define_id!(ConversationId);
define_id!(MessageId);
define_id!(MemoryId);
define_id!(ProfileEntryId);
define_id!(EntityId);
define_id!(RelationshipId);
define_id!(EventId);
define_id!(EmbeddingId);
define_id!(IngestionJobId);
