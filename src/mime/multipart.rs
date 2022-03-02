// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use memchr::memmem;
use serde::Serialize;
use thiserror::Error;

use crate::{mail::syntax as mail, syntax::*, util::{SetOnce, self}};
use super::{Unparsed, Entity, syntax::Header, EntityData};

#[derive(Debug)]
pub struct Multipart {
    pub kind: MultipartKind,
    pub parts: Vec<Entity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MultipartKind {
    Mixed,
    Alternative,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("missing required parameter `boundary`")]
    NoBoundary,
    #[error("no parts")]
    NoParts,
    #[error("missing terminator")]
    Unterminated,
    #[error("invalid header - {0}")]
    ParseHeader(#[from] SyntaxError),
    #[error("nested Content-Transfer-Encoding not allowed")]
    NestedTransferEncoding,
}

pub fn parse(from: Unparsed) -> Result<Entity, super::Error> {
    let mut boundary = None;

    for param in from.content_type.parameters() {
        #[allow(clippy::single_match)]
        match param.attribute {
            "boundary" => boundary = Some(param.value.unquote()),
            _ => {}
        }
    }

    let boundary = boundary.ok_or(Error::NoBoundary)?;
    let parts = split(from.data, boundary.as_bytes())?
        .map(|part| parse_part(&from, part?, from.transfer_encoding.is_some())?.parse())
        .collect::<Result<Vec<_>, _>>()?;

    let kind = match_ignore_ascii_case! { from.content_type.subtype;
        "alternative" => MultipartKind::Alternative,
        _ => MultipartKind::Mixed,
    };

    Ok(Entity {
        data: EntityData::Multipart(Multipart { kind, parts }),
        content_type: from.content_type.into(),
    })
}

fn split<'a: 'b, 'b>(data: &'a [u8], boundary: &'b [u8])
-> Result<impl Iterator<Item = Result<&'a [u8], Error>> + 'b, Error> {
    let except_last_line = match data.strip_suffix(b"\r\n") {
        Some(except_last_line) => except_last_line,
        None => return Err(Error::Unterminated),
    };

    let mut boundaries = memmem::find_iter(except_last_line, b"\r\n")
        .filter(|&start| start + 4 < data.len())
        .map(|start| start + 2)
        .enumerate()
        .filter(|&(_, start)|
            data[start..].starts_with(b"--") && data[start + 2..].starts_with(boundary));

    let (mut line, mut start) = if data.starts_with(b"--") && data[2..].starts_with(boundary) {
        (1, 0)
    } else {
        boundaries.next().ok_or(Error::NoParts)?
    };

    let mut finished = false;

    Ok(std::iter::from_fn(move || {
        if finished {
            return None;
        }

        let next = match boundaries.next() {
            Some(next) => next,
            None => return Some(Err(Error::Unterminated)),
        };

        let data_start = match memmem::find(&data[start..], b"\r\n") {
            Some(data_start) => start + data_start + 2,
            None => return Some(Err(ParseError::Unterminated)),
        };

        let start_line = line;

        (line, start) = next;
        finished = data[start + 2 + boundary.len()..].starts_with(b"--\r\n");

        Some(Ok((start_line, &data[data_start..start])))
    }))
}

fn parse_part<'a>(from: &Unparsed, part: &'a [u8], has_transfer_encoding: bool)
-> Result<Unparsed<'a>, Error> {
    let (header, body) = separate_entity(part);
    let mut header = Buffer::new(header);

    let mut version = None;
    let mut content_type = None;
    let mut transfer_encoding = None;
    let mut id = None;
    let mut description = None;

    while !header.is_empty() {
        let offset = header.offset();
        let field = match mail::field(&mut header)? {
            mail::Header::Mime(field) => field,
            _ => continue,
        };

        match field {
            Header::Version(value) =>
                version.set_once(offset, "MIME-Version", value)?,
            Header::ContentType(value) =>
                content_type.set_once(offset, "Content-Type", value)?,
            Header::ContentTransferEncoding(value) =>
                transfer_encoding.set_once(offset, "Content-Transfer-Encoding", value)?,
            Header::ContentId(value) =>
                id.set_once(offset, "Content-ID", value)?,
            Header::ContentDescription(value) =>
                description.set_once(offset, "Content-Description", value)?,
        }
    }

    if transfer_encoding.is_some() && has_transfer_encoding {
        return Err(Error::NestedTransferEncoding);
    }

    Ok(super::Unparsed {
        data: body,
        version: version.unwrap_or(from.version),
        content_type: content_type.unwrap_or_default(),
        transfer_encoding,
    })
}

/// Separate entity into its header and body sections
fn separate_entity(entity: &[u8]) -> (&[u8], &[u8]) {
    match memmem::find(entity, b"\r\n\r\n") {
        Some(cr) => (&entity[..cr + 2], &entity[cr + 4..]),
        None => (entity, b""),
    }
}
