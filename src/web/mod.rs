use anyhow::Result;
use axum::{AddExtensionLayer, Json, Router, extract::{Extension, Path}, http::StatusCode, routing::get};
use serde::Serialize;
use std::sync::Arc;

use crate::state::{StateRef, Message};

pub async fn start(state: StateRef) -> Result<()> {
    let app = Router::new()
        .route("/messages", get(list_messages))
        .route("/messages/:id", get(message))
        .layer(AddExtensionLayer::new(state))
    ;

    axum::Server::bind(&"0.0.0.0:80".parse().unwrap())
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

#[derive(Serialize)]
struct MessageData {
    id: String,
    subject: Option<String>,
}

impl From<&'_ Message> for MessageData {
    fn from(Message { id, subject, .. }: &'_ Message) -> Self {
        MessageData {
            id: id.clone(),
            subject: subject.clone(),
        }
    }
}

async fn list_messages(Extension(state): Extension<StateRef>) -> Json<Vec<MessageData>> {
    Json(state.messages()
        .await
        .values()
        .map(Arc::as_ref)
        .map(MessageData::from)
        .collect())
}

async fn message(Extension(state): Extension<StateRef>, Path(id): Path<String>)
-> Result<String, StatusCode> {
    match state.get_message(&id).await {
        Some(message) => Ok(message.body.clone()),
        None => Err(StatusCode::NOT_FOUND),
    }
}
