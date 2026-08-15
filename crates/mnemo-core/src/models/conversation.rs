//! Conversation storage model (plan.md section 18 "Conversation Storage").

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{ConversationId, MessageId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    pub fn new(title: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: ConversationId::new(),
            title,
            created_at: now,
            updated_at: now,
        }
    }
}

/// The role a [`Message`] author played in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    /// Raw tool call payloads, if this message issued any.
    pub tool_calls: Option<Value>,
    /// Raw tool result payloads, if this message is a tool response.
    pub tool_results: Option<Value>,
    pub metadata: Option<Value>,
}

impl Message {
    pub fn new(conversation_id: ConversationId, role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            conversation_id,
            role,
            content: content.into(),
            timestamp: Utc::now(),
            tool_calls: None,
            tool_results: None,
            metadata: None,
        }
    }
}
