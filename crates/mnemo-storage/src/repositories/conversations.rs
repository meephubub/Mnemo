use mnemo_core::ids::{ConversationId, MessageId};
use mnemo_core::models::{Conversation, Message};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Result, StorageError};
use crate::util::*;

fn conversation_from_row(row: &Row) -> rusqlite::Result<Conversation> {
    let id: String = row.get("id")?;
    let created_at: String = row.get("created_at")?;
    let updated_at: String = row.get("updated_at")?;
    Ok(Conversation {
        id: id
            .parse::<uuid::Uuid>()
            .map(ConversationId::from_uuid)
            .unwrap_or_default(),
        title: row.get("title")?,
        created_at: str_to_dt(&created_at).unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: str_to_dt(&updated_at).unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub(crate) fn message_from_row(row: &Row) -> rusqlite::Result<Message> {
    let id: String = row.get("id")?;
    let conversation_id: String = row.get("conversation_id")?;
    let role: String = row.get("role")?;
    let timestamp: String = row.get("timestamp")?;
    let tool_calls: Option<String> = row.get("tool_calls")?;
    let tool_results: Option<String> = row.get("tool_results")?;
    let metadata: Option<String> = row.get("metadata")?;

    Ok(Message {
        id: id.parse::<uuid::Uuid>().map(MessageId::from_uuid).unwrap_or_default(),
        conversation_id: conversation_id
            .parse::<uuid::Uuid>()
            .map(ConversationId::from_uuid)
            .unwrap_or_default(),
        role: str_to_role(&role).unwrap_or(mnemo_core::models::MessageRole::User),
        content: row.get("content")?,
        timestamp: str_to_dt(&timestamp).unwrap_or_else(|_| chrono::Utc::now()),
        tool_calls: tool_calls.and_then(|s| serde_json::from_str(&s).ok()),
        tool_results: tool_results.and_then(|s| serde_json::from_str(&s).ok()),
        metadata: metadata.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

pub fn insert_conversation(conn: &Connection, conversation: &Conversation) -> Result<()> {
    conn.execute(
        "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            conversation.id.to_string(),
            conversation.title,
            dt_to_str(conversation.created_at),
            dt_to_str(conversation.updated_at),
        ],
    )?;
    Ok(())
}

pub fn get_conversation(conn: &Connection, id: ConversationId) -> Result<Conversation> {
    conn.query_row(
        "SELECT * FROM conversations WHERE id = ?1",
        params![id.to_string()],
        conversation_from_row,
    )
    .optional()?
    .ok_or_else(|| StorageError::NotFound(format!("conversation {id}")))
}

pub fn list_conversations(conn: &Connection) -> Result<Vec<Conversation>> {
    let mut stmt = conn.prepare("SELECT * FROM conversations ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([], conversation_from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn insert_message(conn: &Connection, message: &Message) -> Result<()> {
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, timestamp, tool_calls, tool_results, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            message.id.to_string(),
            message.conversation_id.to_string(),
            role_to_str(message.role),
            message.content,
            dt_to_str(message.timestamp),
            message.tool_calls.as_ref().map(|v| v.to_string()),
            message.tool_results.as_ref().map(|v| v.to_string()),
            message.metadata.as_ref().map(|v| v.to_string()),
        ],
    )?;
    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        params![dt_to_str(message.timestamp), message.conversation_id.to_string()],
    )?;
    Ok(())
}

pub fn list_messages(conn: &Connection, conversation_id: ConversationId) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare("SELECT * FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC")?;
    let rows = stmt.query_map(params![conversation_id.to_string()], message_from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
