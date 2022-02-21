use std::{collections::{HashMap, hash_map::Entry}, sync::Arc};
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::{mail, syntax::SyntaxError};

pub struct State {
    messages: RwLock<HashMap<String, Arc<Message>>>,
}

pub type StateRef = Arc<State>;

pub struct Message {
    pub id: String,
    pub subject: Option<String>,
    // TODO: parse message body
    pub body: String,
}

impl State {
    pub fn new() -> StateRef {
        Arc::new(State {
            messages: RwLock::new(HashMap::default()),
        })
    }

    pub async fn messages(&self) -> impl std::ops::Deref<Target = HashMap<String, Arc<Message>>> + '_ {
        self.messages.read().await
    }

    pub async fn get_message(&self, id: &str) -> Option<Arc<Message>> {
        self.messages.read().await.get(id).cloned()
    }

    pub async fn submit_message(&self, message: &[u8]) -> Result<(), SubmitMessageError> {
        let message = mail::parse(message)?;

        let message = Message {
            id: message.id.unwrap_or_else(
                || format!("{}@local", OffsetDateTime::now_utc().unix_timestamp())),
            subject: message.subject,
            // TODO: parse message body
            body: String::from_utf8(message.body.to_vec())?,
        };

        self.add_message(message).await
    }

    /// Add message to `self.messages` and notify listeners
    async fn add_message(&self, message: Message) -> Result<(), SubmitMessageError> {
        let message = Arc::new(message);

        match self.messages.write().await.entry(message.id.clone()) {
            Entry::Occupied(_) => return Err(SubmitMessageError::DuplicateMailId),
            Entry::Vacant(entry) => {
                entry.insert(message);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SubmitMessageError {
    #[error(transparent)]
    Syntax(#[from] SyntaxError),
    #[error("Attempted to re-use existing mail ID")]
    DuplicateMailId,
    #[error("Syntax error - invalid character - {0}")]
    Encoding(#[from] std::string::FromUtf8Error),
}

impl SubmitMessageError {
    pub fn code(&self) -> u16 {
        match self {
            SubmitMessageError::Syntax(_) | SubmitMessageError::Encoding(_) => 500,
            SubmitMessageError::DuplicateMailId => 550,
        }
    }
}
