//! Small helpers for converting between core model types and the
//! plain-text/integer representations SQLite stores them as.

use chrono::{DateTime, Utc};
use mnemo_core::models::{MemoryStatus, MemoryType, MessageRole, Sensitivity, SourceType};

use crate::error::{Result, StorageError};

pub fn dt_to_str(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

pub fn opt_dt_to_str(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(dt_to_str)
}

pub fn str_to_dt(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| StorageError::Decode(format!("invalid timestamp '{s}': {e}")))
}

pub fn opt_str_to_dt(s: Option<String>) -> Result<Option<DateTime<Utc>>> {
    match s {
        Some(s) => Ok(Some(str_to_dt(&s)?)),
        None => Ok(None),
    }
}

pub fn source_type_to_str(t: SourceType) -> &'static str {
    match t {
        SourceType::File => "FILE",
        SourceType::Email => "EMAIL",
        SourceType::Conversation => "CONVERSATION",
        SourceType::Webpage => "WEBPAGE",
        SourceType::Profile => "PROFILE",
        SourceType::Inference => "INFERENCE",
        SourceType::UserStatement => "USER_STATEMENT",
    }
}

pub fn str_to_source_type(s: &str) -> Result<SourceType> {
    Ok(match s {
        "FILE" => SourceType::File,
        "EMAIL" => SourceType::Email,
        "CONVERSATION" => SourceType::Conversation,
        "WEBPAGE" => SourceType::Webpage,
        "PROFILE" => SourceType::Profile,
        "INFERENCE" => SourceType::Inference,
        "USER_STATEMENT" => SourceType::UserStatement,
        other => return Err(StorageError::Decode(format!("unknown source_type '{other}'"))),
    })
}

pub fn sensitivity_to_str(s: Sensitivity) -> &'static str {
    match s {
        Sensitivity::Public => "PUBLIC",
        Sensitivity::Private => "PRIVATE",
        Sensitivity::Sensitive => "SENSITIVE",
    }
}

pub fn str_to_sensitivity(s: &str) -> Result<Sensitivity> {
    Ok(match s {
        "PUBLIC" => Sensitivity::Public,
        "PRIVATE" => Sensitivity::Private,
        "SENSITIVE" => Sensitivity::Sensitive,
        other => return Err(StorageError::Decode(format!("unknown sensitivity '{other}'"))),
    })
}

pub fn role_to_str(r: MessageRole) -> &'static str {
    match r {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

pub fn str_to_role(s: &str) -> Result<MessageRole> {
    Ok(match s {
        "system" => MessageRole::System,
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "tool" => MessageRole::Tool,
        other => return Err(StorageError::Decode(format!("unknown role '{other}'"))),
    })
}

pub fn memory_type_to_str(t: MemoryType) -> &'static str {
    match t {
        MemoryType::Fact => "FACT",
        MemoryType::Preference => "PREFERENCE",
        MemoryType::Interest => "INTEREST",
        MemoryType::Goal => "GOAL",
        MemoryType::Project => "PROJECT",
        MemoryType::Person => "PERSON",
        MemoryType::Location => "LOCATION",
        MemoryType::Routine => "ROUTINE",
        MemoryType::Decision => "DECISION",
        MemoryType::Event => "EVENT",
        MemoryType::Temporary => "TEMPORARY",
    }
}

pub fn str_to_memory_type(s: &str) -> Result<MemoryType> {
    Ok(match s {
        "FACT" => MemoryType::Fact,
        "PREFERENCE" => MemoryType::Preference,
        "INTEREST" => MemoryType::Interest,
        "GOAL" => MemoryType::Goal,
        "PROJECT" => MemoryType::Project,
        "PERSON" => MemoryType::Person,
        "LOCATION" => MemoryType::Location,
        "ROUTINE" => MemoryType::Routine,
        "DECISION" => MemoryType::Decision,
        "EVENT" => MemoryType::Event,
        "TEMPORARY" => MemoryType::Temporary,
        other => return Err(StorageError::Decode(format!("unknown memory_type '{other}'"))),
    })
}

pub fn memory_status_to_str(s: MemoryStatus) -> &'static str {
    match s {
        MemoryStatus::Candidate => "CANDIDATE",
        MemoryStatus::Active => "ACTIVE",
        MemoryStatus::Temporary => "TEMPORARY",
        MemoryStatus::Superseded => "SUPERSEDED",
        MemoryStatus::Archived => "ARCHIVED",
        MemoryStatus::Expired => "EXPIRED",
    }
}

pub fn str_to_memory_status(s: &str) -> Result<MemoryStatus> {
    Ok(match s {
        "CANDIDATE" => MemoryStatus::Candidate,
        "ACTIVE" => MemoryStatus::Active,
        "TEMPORARY" => MemoryStatus::Temporary,
        "SUPERSEDED" => MemoryStatus::Superseded,
        "ARCHIVED" => MemoryStatus::Archived,
        "EXPIRED" => MemoryStatus::Expired,
        other => return Err(StorageError::Decode(format!("unknown memory_status '{other}'"))),
    })
}
