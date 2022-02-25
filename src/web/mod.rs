// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use anyhow::Result;
use axum::{
    AddExtensionLayer, Json, Router,
    extract::{Extension, Path, ws},
    http::StatusCode,
    routing::get,
    response::{IntoResponse, Response}, body,
};
use serde::Serialize;
use time::OffsetDateTime;
use std::{sync::Arc, net::{SocketAddr, Ipv4Addr}};

use crate::{state::{StateRef, Message}, mail::{Mailbox, AddressOrGroup}, config};

pub async fn start(config: config::Http, state: StateRef) -> Result<()> {
    let app = Router::new()
        .route("/messages", get(list_messages))
        .route("/messages/:id", get(message))
        .route("/subscribe", get(message_stream))
        .route("/", get(index))
        .route("/:file", get(page_file))
        .layer(AddExtensionLayer::new(state))
    ;

    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), config.port);

    let server = axum::Server::bind(&addr).serve(app.into_make_service());
    log::info!("Started HTTP server on {addr}");
    server.await?;

    Ok(())
}

#[derive(Debug, Serialize)]
struct MessageData {
    id: String,
    #[serde(with = "time::serde::timestamp")]
    date: OffsetDateTime,
    from: Vec<Mailbox>,
    subject: Option<String>,
    to: Vec<AddressOrGroup>,
}

impl From<&'_ Message> for MessageData {
    fn from(Message { id, date, from, subject, to, .. }: &'_ Message) -> Self {
        MessageData {
            id: id.clone(),
            date: *date,
            from: from.clone(),
            subject: subject.clone(),
            to: to.clone(),
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

async fn message_stream(
    Extension(state): Extension<StateRef>,
    ws: ws::WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(state, socket))
}

async fn handle_socket(state: StateRef, mut socket: ws::WebSocket) {
    log::debug!("listener connected");

    let mut messages = state.subscribe();

    loop {
        tokio::select! {
            msg = messages.recv() => {
                let msg = match msg {
                    Ok(msg) => MessageData::from(&*msg),
                    Err(_) => break,
                };

                log::trace!("notifying listener of {msg:?}");
                let msg = serde_json::to_string(&msg).expect("failed to convert message to JSON");

                if socket.send(ws::Message::Text(msg)).await.is_err() {
                    break;
                }
            }

            msg = socket.recv() => {
                log::trace!("received message from client: {msg:?}");
                break;
            }
        }
    }

    let _ = socket.close().await;

    log::debug!("listener disconnected");
}

async fn index() -> &'static File {
    PAGE_DATA.iter()
        .find(|file| file.name == "index.html")
        .unwrap()
}

async fn page_file(Path(name): Path<String>) -> Result<&'static File, StatusCode> {
    PAGE_DATA.iter().find(|file| file.name == name).ok_or(StatusCode::NOT_FOUND)
}

struct File {
    name: &'static str,
    data: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/page_data.rs"));

impl IntoResponse for &'_ File {
    fn into_response(self) -> Response {
        Response::builder()
            .header("Content-Type", match self.name.rsplit('.').next().unwrap() {
                "css" => "text/css",
                "html" => "text/html",
                "js" => "application/javascript",
                _ => "application/octet-stream"
            })
            .body(body::boxed(body::Full::new(body::Bytes::from_static(self.data))))
            .unwrap()
    }
}
