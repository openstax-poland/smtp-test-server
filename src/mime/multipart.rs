// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use memchr::memmem;
use serde::Serialize;
use thiserror::Error;

use crate::{mail::{syntax as mail, ParseFieldError, separate_message}, syntax::*, state::Errors};
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
pub enum ParseError {
    #[error("no parts")]
    NoParts,
    #[error("missing multipart terminator")]
    Unterminated,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid header - {0}")]
    ParseHeader(#[from] SyntaxError),
    #[error("nested Content-Transfer-Encoding not allowed")]
    NestedTransferEncoding,
    #[error("duplicate header {0}")]
    DuplicateHeader(&'static str),
}

pub fn parse(from: Unparsed, errors: &mut Errors)
-> Result<Entity, super::Error> {
    let mut boundary = None;

    for param in from.content_type.parameters() {
        #[allow(clippy::single_match)]
        match param.attribute {
            "boundary" => boundary = Some(param.value.unquote()),
            _ => {}
        }
    }

    let boundary = boundary.ok_or(super::Error::MissingRequiredParameter("boundary"))?;
    let parts = split(from.data.item, boundary.as_bytes())?
        .map(|part| {
            let Located { at, item: data } = part?;
            let mut errors = errors.nested(at);
            let part = parse_part(&from, &mut errors, data, from.transfer_encoding.is_some())?;
            part.parse(&mut errors)
        })
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
-> Result<impl Iterator<Item = Result<Located<&'a [u8]>, ParseError>> + 'b, ParseError> {
    let except_last_line = match data.strip_suffix(b"\r\n") {
        Some(except_last_line) => except_last_line,
        None => return Err(ParseError::Unterminated),
    };

    let mut boundaries = memmem::find_iter(except_last_line, b"\r\n")
        .enumerate()
        .map(|(line, start)| (line + 1, start + 2))
        .filter(|&(_, start)| {
            start + 2 < data.len()
                && data[start..].starts_with(b"--")
                && data[start + 2..].starts_with(boundary)
        });

    let (mut line, mut start) = if data.starts_with(b"--") && data[2..].starts_with(boundary) {
        (0, 0)
    } else {
        boundaries.next().ok_or(ParseError::NoParts)?
    };

    let mut finished = false;

    Ok(std::iter::from_fn(move || {
        if finished {
            return None;
        }

        let next = match boundaries.next() {
            Some(next) => next,
            None => return Some(Err(ParseError::Unterminated)),
        };

        let data_start = match memmem::find(&data[start..], b"\r\n") {
            Some(data_start) => start + data_start + 2,
            None => return Some(Err(ParseError::Unterminated)),
        };

        let location = Location {
            offset: data_start,
            line: line + 1,
            column: 1,
        };

        (line, start) = next;
        finished = data[start + 2 + boundary.len()..].starts_with(b"--\r\n");

        Some(Ok(Located::new(location, &data[data_start..start])))
    }))
}

fn parse_part<'a>(
    from: &Unparsed,
    errors: &mut Errors,
    part: &'a [u8],
    has_transfer_encoding: bool,
) -> Result<Unparsed<'a>, Located<SyntaxError>> {
    let (header, body) = separate_message(part);
    let mut header = Buffer::new(header);

    let mut version = None;
    let mut content_type = None;
    let mut transfer_encoding = None;
    let mut id = None;
    let mut description = None;

    while !header.is_empty() {
        let location = header.location();

        let field = match mail::field(&mut header) {
            Ok(field) => field,
            Err(error) => {
                log::trace!("error parsing field: {error}");
                let (field, _) = mail::optional_field(&mut header)?;
                errors.add(error.map(|error| ParseFieldError { field, error }));
                continue;
            }
        };

        let field = match field {
            mail::Header::Mime(field) => field,
            _ => continue,
        };

        match field {
            Header::Version(value) =>
                version.set_once(errors, location, "MIME-Version", value),
            Header::ContentType(value) =>
                content_type.set_once(errors, location, "Content-Type", value),
            Header::ContentTransferEncoding(value) => {
                if has_transfer_encoding {
                    errors.add(Located::<Error>::new(location, Error::NestedTransferEncoding));
                } else {
                    transfer_encoding.set_once(errors, location, "Content-Transfer-Encoding", value);
                }
            }
            Header::ContentId(value) =>
                id.set_once(errors, location, "Content-ID", value),
            Header::ContentDescription(value) =>
                description.set_once(errors, location, "Content-Description", value),
        }
    }

    Ok(super::Unparsed {
        data: body,
        version: version.unwrap_or(from.version),
        content_type: content_type.unwrap_or_default(),
        transfer_encoding,
    })
}

trait SetOnce<T> {
    fn set_once(
        &mut self,
        errors: &mut Errors,
        at: Location,
        header: &'static str,
        value: T,
    );
}

impl<T> SetOnce<T> for Option<T> {
    fn set_once(
        &mut self,
        errors: &mut Errors,
        at: Location,
        header: &'static str,
        value: T,
    ) {
        if self.is_some() {
            errors.add(Located::<Error>::new(at, Error::DuplicateHeader(header)));
        } else {
            *self = Some(value);
        }
    }
}
