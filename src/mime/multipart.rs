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
    let mut boundaries = (0..data.len())
        .filter_map(|start| {
            if data[start..].starts_with(b"\r\n--")  && data[start + 4..].starts_with(boundary) {
                Some((start, start + 4 + boundary.len()))
            } else {
                None
            }
        });

    let (mut start, mut start2) = if data.starts_with(b"--") && data[2..].starts_with(boundary) {
        (0, 2 + boundary.len())
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

        let data_start = match (start2..data.len()).find(|&off| data[off..].starts_with(b"\r\n")) {
            Some(data_start) => data_start + 2,
            None => return Some(Err(Error::Unterminated)),
        };

        (start, start2) = next;
        finished = data[start2..].starts_with(b"--");

        Some(Ok(&data[data_start..start]))
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
    for (cr, _) in entity.iter().enumerate().filter(|&(_, &c)| c == b'\r') {
        if entity[cr..].starts_with(b"\r\n\r\n") {
            return (&entity[..cr + 2], &entity[cr + 4..]);
        }
    }
    (entity, b"")
}
