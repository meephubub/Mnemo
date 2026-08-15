//! Conversation storage API surface (plan.md section 18
//! "Conversation Storage").

use mnemo_core::ids::ConversationId;
use mnemo_core::models::{Conversation, Message, MessageRole};
use mnemo_core::Result;
use mnemo_storage::{repositories::conversations as repo, Db};

use crate::blocking;

/// Handle for recording and browsing conversation history.
///
/// Obtained via [`crate::Mnemo::conversations`]; cheap to create (it
/// just holds a clone of the shared DB handle).
#[derive(Clone)]
pub struct ConversationStore {
    db: Db,
}

impl ConversationStore {
    pub(crate) fn new(db: Db) -> Self {
        Self { db }
    }

    /// Start a new conversation, optionally titled.
    pub async fn create(&self, title: Option<String>) -> Result<Conversation> {
        let db = self.db.clone();
        blocking::run(move || {
            let conversation = Conversation::new(title);
            repo::insert_conversation(&db.conn(), &conversation)?;
            Ok(conversation)
        })
        .await
    }

    pub async fn get(&self, id: ConversationId) -> Result<Conversation> {
        let db = self.db.clone();
        blocking::run(move || Ok(repo::get_conversation(&db.conn(), id)?)).await
    }

    /// List every conversation, most recently updated first.
    pub async fn list(&self) -> Result<Vec<Conversation>> {
        let db = self.db.clone();
        blocking::run(move || Ok(repo::list_conversations(&db.conn())?)).await
    }

    /// Append a message to a conversation, bumping its `updated_at`.
    pub async fn add_message(
        &self,
        conversation_id: ConversationId,
        role: MessageRole,
        content: impl Into<String> + Send + 'static,
    ) -> Result<Message> {
        let db = self.db.clone();
        blocking::run(move || {
            let message = Message::new(conversation_id, role, content.into());
            repo::insert_message(&db.conn(), &message)?;
            Ok(message)
        })
        .await
    }

    /// List every message in a conversation, oldest first.
    pub async fn messages(&self, conversation_id: ConversationId) -> Result<Vec<Message>> {
        let db = self.db.clone();
        blocking::run(move || Ok(repo::list_messages(&db.conn(), conversation_id)?)).await
    }
}
