use std::{fmt, borrow::Cow};
use thiserror::Error;

mod encoding;
mod multipart;

pub mod syntax;

use crate::{mime::encoding::Charset, util};

pub use self::{
    multipart::{Multipart, MultipartKind},
    syntax::{MimeVersion, ContentType, TransferEncoding, Header},
};

#[derive(Debug)]
pub struct Entity {
    pub data: EntityData,
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
    pub content_type: ContentType<'a>,
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
                        "charset" => charset = match_ignore_ascii_case! { param.value.unquote();
                            "US-ASCII" => Charset::UsAscii,
                            "ISO-8859-2" => Charset::Iso8859_2,
                            "ISO-8859-3" => Charset::Iso8859_3,
                            "ISO-8859-4" => Charset::Iso8859_4,
                            "ISO-8859-5" => Charset::Iso8859_5,
                            "ISO-8859-6" => Charset::Iso8859_6,
                            "ISO-8859-7" => Charset::Iso8859_7,
                            "ISO-8859-8" => Charset::Iso8859_8,
                            "ISO-8859-10" => Charset::Iso8859_10,
                            "ISO-8859-13" => Charset::Iso8859_13,
                            "ISO-8859-14" => Charset::Iso8859_14,
                            "ISO-8859-15" => Charset::Iso8859_15,
                            "ISO-8859-16" => Charset::Iso8859_16,
                            "UTF-8" => Charset::Utf8,
                            _ => return Ok(Entity {
                                data: EntityData::Binary(data.into_owned()),
                            }),
                        },
                        _ => {}
                    }
                }

                match_ignore_ascii_case! { self.content_type.subtype;
                    _ => {
                        Ok(Entity {
                            data: EntityData::Text(charset.decode(&data)?.into_owned()),
                        })
                    }
                }
            }

            "audio" | "image" | "video" => Ok(Entity {
                data: EntityData::Binary(data.into_owned()),
            }),

            "application" => match_ignore_ascii_case! { self.content_type.subtype;
                _ => Ok(Entity {
                    data: EntityData::Binary(data.into_owned()),
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
