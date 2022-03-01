// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use axum::http::HeaderValue;
use std::{fmt, borrow::Cow};
use thiserror::Error;

mod multipart;

pub mod encoding;
pub mod syntax;

use crate::{mime::encoding::Charset, util};

pub use self::{
    multipart::{Multipart, MultipartKind},
    syntax::{MimeVersion, TransferEncoding, Header},
};

#[derive(Debug)]
pub struct Entity {
    pub data: EntityData,
    pub content_type: ContentType,
}

pub enum EntityData {
    /// text/plain
    Text(String),
    /// Any binary data, such as application/octet-stream, or image/*
    Binary(Vec<u8>),
    Multipart(Multipart),
}

pub struct Unparsed<'a> {
    pub data: &'a [u8],
    pub version: MimeVersion,
    pub content_type: syntax::ContentType<'a>,
    pub transfer_encoding: Option<TransferEncoding>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("unsupported Content-Type")]
    UnsupportedContentType,
    #[error("missing required parameter {0}")]
    MissingRequiredParameter(&'static str),
    #[error(transparent)]
    TransferEncoding(#[from] self::encoding::DecodeError),
    #[error(transparent)]
    Charset(#[from] self::encoding::CharsetError),
    #[error("Content-Type: multipart - {0}")]
    Multipart(#[from] multipart::Error),
}

impl<'a> Unparsed<'a> {
    pub fn parse(self) -> Result<Entity, Error> {
        let data = match self.transfer_encoding {
            Some(encoding) => Cow::from(encoding.decode(self.data)?),
            None => Cow::from(self.data),
        };

        match_ignore_ascii_case! { self.content_type.type_;
            "text" => {
                let mut charset = Charset::UsAscii;

                for param in self.content_type.parameters() {
                    #[allow(clippy::single_match)]
                    match param.attribute {
                        "charset" => charset = match Charset::by_name(&param.value.unquote()) {
                            Some(charset) => charset,
                            None => return Ok(Entity {
                                data: EntityData::Binary(data.into_owned()),
                                content_type: ContentType::APPLICATION_OCTET_STREAM,
                            }),
                        },
                        _ => {}
                    }
                }

                match_ignore_ascii_case! { self.content_type.subtype;
                    "html" => Ok(Entity {
                        data: EntityData::Text(charset.decode(&data)?.into_owned()),
                        content_type: self.content_type.into(),
                    }),
                    _ => Ok(Entity {
                        data: EntityData::Text(charset.decode(&data)?.into_owned()),
                        content_type: ContentType::from(self.content_type).with_subtype("plain"),
                    }),
                }
            }

            "audio" | "image" | "video" => Ok(Entity {
                data: EntityData::Binary(data.into_owned()),
                content_type: self.content_type.into(),
            }),

            "application" => match_ignore_ascii_case! { self.content_type.subtype;
                _ => Ok(Entity {
                    data: EntityData::Binary(data.into_owned()),
                    content_type: ContentType::APPLICATION_OCTET_STREAM,
                }),
            },

            "multipart" => multipart::parse(self),

            _ => Err(Error::UnsupportedContentType),
        }
    }
}

impl fmt::Debug for EntityData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EntityData::Text(ref text) =>
                f.debug_tuple("Text").field(text).finish(),
            EntityData::Binary(ref data) =>
                f.debug_tuple("Binary").field(&util::maybe_ascii(data)).finish(),
            EntityData::Multipart(ref mp) =>
                f.debug_tuple("Multipart").field(mp).finish(),
        }
    }
}

#[derive(Clone)]
pub struct ContentType {
    type_: Cow<'static, str>,
    subtype: Cow<'static, str>,
    parameters: Cow<'static, [(Cow<'static, str>, Cow<'static, str>)]>,
}

impl ContentType {
    pub const TEXT_PLAIN: ContentType = ContentType {
        type_: Cow::Borrowed("text"),
        subtype: Cow::Borrowed("plain"),
        parameters: Cow::Borrowed(&[
            (Cow::Borrowed("charset"), Cow::Borrowed("us-ascii")),
        ]),
    };

    pub const APPLICATION_OCTET_STREAM: ContentType = ContentType {
        type_: Cow::Borrowed("application"),
        subtype: Cow::Borrowed("octet-stream"),
        parameters: Cow::Borrowed(&[]),
    };

    pub const APPLICATION_JSON: ContentType = ContentType {
        type_: Cow::Borrowed("application"),
        subtype: Cow::Borrowed("json"),
        parameters: Cow::Borrowed(&[]),
    };

    pub fn with_subtype(self, subtype: impl Into<Cow<'static, str>>) -> Self {
        ContentType { subtype: subtype.into(), ..self }
    }
}

impl From<syntax::ContentType<'_>> for ContentType {
    fn from(ct: syntax::ContentType<'_>) -> Self {
        ContentType {
            type_: Cow::Owned(ct.type_.into()),
            subtype: Cow::Owned(ct.subtype.into()),
            parameters: ct.parameters()
                .map(|param| (
                    Cow::Owned(param.attribute.into()),
                    Cow::Owned(param.value.unquote().into_owned()),
                ))
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

impl fmt::Debug for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ContentType {{ {self} }}")
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.type_, self.subtype)?;

        for (attribute, value) in self.parameters.iter() {
            write!(f, "; {attribute}={value:?}")?;
        }

        Ok(())
    }
}

impl From<ContentType> for HeaderValue {
    fn from(ct: ContentType) -> Self {
        ct.to_string().try_into().unwrap()
    }
}


impl From<&'_ ContentType> for HeaderValue {
    fn from(ct: &'_ ContentType) -> Self {
        ct.to_string().try_into().unwrap()
    }
}
