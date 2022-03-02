// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use anyhow::Result;
use axum::{
    AddExtensionLayer, Json, Router,
    body,
    extract::{Extension, Path, ws},
    http::{StatusCode, Response, header::CONTENT_TYPE},
    response::IntoResponse,
    routing::get,
};
use serde::Serialize;
use time::OffsetDateTime;
use std::{sync::Arc, net::{SocketAddr, Ipv4Addr}};

use crate::{
    config,
    mail::{Mailbox, AddressOrGroup},
    mime::{EntityData, ContentType, Entity, MultipartKind},
    state::{StateRef, Message, MessageBody},
    syntax::Located,
    util,
};

pub async fn start(config: config::Http, state: StateRef) -> Result<()> {
    let app = Router::new()
        .route("/messages", get(list_messages))
        .route("/messages/:id", get(message))
        .route("/messages/:id/*number", get(message_part))
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
    body: BodyType,
    errors: Vec<Located<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum BodyType {
    Data,
    MimeMultipart,
}

impl From<&'_ Message> for MessageData {
    fn from(Message { id, date, from, subject, to, body, errors, .. }: &'_ Message) -> Self {
        MessageData {
            id: id.clone(),
            date: *date,
            from: from.clone(),
            subject: subject.clone(),
            to: to.clone(),
            body: match body {
                MessageBody::Unknown(_) => BodyType::Data,
                MessageBody::Mime(ref mime) => match mime.data {
                    EntityData::Multipart(_) => BodyType::MimeMultipart,
                    _ => BodyType::Data,
                },
            },
            errors: errors.clone(),
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
-> Result<impl IntoResponse, StatusCode> {
    let message = match state.get_message(&id).await {
        Some(message) => message,
        None => return Err(StatusCode::NOT_FOUND),
    };

    Ok(match message.body {
        MessageBody::Unknown(ref body) => Response::builder()
            .header(CONTENT_TYPE, ContentType::TEXT_PLAIN)
            .body(to_bytes(body.as_bytes()))
            .unwrap(),
        MessageBody::Mime(ref entity) => entity_to_response(entity),
    })
}

fn entity_to_response(entity: &Entity) -> Response<body::Full<body::Bytes>> {
    match entity.data {
        EntityData::Text(ref text) => Response::builder()
            .header(CONTENT_TYPE, &entity.content_type)
            .body(to_bytes(text.as_bytes()))
            .unwrap(),
        EntityData::Binary(ref data) => Response::builder()
            .header(CONTENT_TYPE, &entity.content_type)
            .body(to_bytes(data))
            .unwrap(),
        EntityData::Multipart(ref mp) => Response::builder()
            .header(CONTENT_TYPE, ContentType::APPLICATION_JSON)
            .body(body::Full::new(serde_json::to_vec(&MultipartDesc {
                kind: mp.kind,
                parts: mp.parts.iter().map(|entity| PartDesc {
                    content_type: &entity.content_type,
                }).collect(),
            }).unwrap().into()))
            .unwrap(),
    }
}

#[derive(Serialize)]
struct MultipartDesc<'a> {
    kind: MultipartKind,
    parts: Vec<PartDesc<'a>>,
}

#[derive(Serialize)]
struct PartDesc<'a> {
    #[serde(with = "util::as_string", rename = "contentType")]
    content_type: &'a ContentType,
}

async fn message_part(Extension(state): Extension<StateRef>, Path((id, path)): Path<(String, String)>)
-> Result<impl IntoResponse, StatusCode> {
    let message = match state.get_message(&id).await {
        Some(message) => message,
        _ => return Err(StatusCode::NOT_FOUND),
    };

    let mut entity = match message.body {
        MessageBody::Mime(ref entity) => entity,
        _ => return Err(StatusCode::NOT_FOUND),
    };

    for part in path.split('/').skip(1) {
        let part: usize = match part.parse() {
            Ok(part) => part,
            Err(_) => return Err(StatusCode::NOT_FOUND),
        };

        let mp = match entity.data {
            EntityData::Multipart(ref mp) => mp,
            _ => return Err(StatusCode::NOT_FOUND),
        };

        entity = match mp.parts.get(part) {
            Some(part) => part,
            _ => return Err(StatusCode::NOT_FOUND),
        };
    }

    Ok(entity_to_response(entity))
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
    fn into_response(self) -> axum::response::Response {
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

fn to_bytes(bytes: &[u8]) -> body::Full<body::Bytes> {
    body::Full::new(body::Bytes::copy_from_slice(bytes))
}
