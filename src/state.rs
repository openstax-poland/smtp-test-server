// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use std::{collections::{HashMap, hash_map::Entry}, sync::Arc};
use thiserror::Error;
use time::{OffsetDateTime, UtcOffset};
use tokio::sync::{RwLock, broadcast};

use crate::{mail::{self, Mailbox, AddressOrGroup}, syntax::SyntaxError, mime};

pub struct State {
    messages: RwLock<HashMap<String, Arc<Message>>>,
    on_message: broadcast::Sender<Arc<Message>>,
}

pub type StateRef = Arc<State>;

pub struct Message {
    pub id: String,
    pub date: OffsetDateTime,
    pub from: Vec<Mailbox>,
    pub subject: Option<String>,
    pub to: Vec<AddressOrGroup>,
    pub body: MessageBody,
}

pub enum MessageBody {
    Unknown(String),
    Mime(mime::Entity),
}

impl State {
    pub fn new() -> StateRef {
        Arc::new(State {
            messages: RwLock::new(HashMap::default()),
            on_message: broadcast::channel(16).0,
        })
    }

    pub async fn messages(&self) -> impl std::ops::Deref<Target = HashMap<String, Arc<Message>>> + '_ {
        self.messages.read().await
    }

    pub async fn get_message(&self, id: &str) -> Option<Arc<Message>> {
        self.messages.read().await.get(id).cloned()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Message>> {
        self.on_message.subscribe()
    }

    pub async fn submit_message(&self, message: &[u8]) -> Result<(), SubmitMessageError> {
        let message = mail::parse(message)?;

        let message = Message {
            id: message.id.unwrap_or_else(
                || format!("{}@local", OffsetDateTime::now_utc().unix_timestamp())),
            date: message.origination_date.with_offset_when_missing(UtcOffset::UTC),
            from: message.from.iter().map(|x| x.to_owned()).collect(),
            subject: message.subject,
            to: message.to.iter().map(|x| x.to_owned()).collect(),
            body: match message.body {
                mail::Body::Unknown(body) =>
                    MessageBody::Unknown(String::from_utf8(body.to_vec())?),
                mail::Body::Mime(body) => MessageBody::Mime(body.parse()?),
            },
        };

        self.add_message(message).await
    }

    /// Add message to `self.messages` and notify listeners
    async fn add_message(&self, message: Message) -> Result<(), SubmitMessageError> {
        let message = Arc::new(message);

        match self.messages.write().await.entry(message.id.clone()) {
            Entry::Occupied(_) => return Err(SubmitMessageError::DuplicateMailId),
            Entry::Vacant(entry) => {
                entry.insert(message.clone());
            }
        }

        let _ = self.on_message.send(message);

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
    #[error("Syntax error - {0}")]
    Mime(#[from] mime::Error),
}

impl SubmitMessageError {
    pub fn code(&self) -> u16 {
        match self {
            SubmitMessageError::Syntax(_) | SubmitMessageError::Encoding(_)
            | SubmitMessageError::Mime(_) => 500,
            SubmitMessageError::DuplicateMailId => 550,
        }
    }
}
