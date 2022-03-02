// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

//! Implementation of [RFC 5322](
//! https://datatracker.ietf.org/doc/html/rfc5322): Internet Message Format

use memchr::memmem;

use crate::{syntax::*, mime, util::SetOnce};
use self::syntax::{Header, MailboxList, MailboxRef, PathRef, Received, AnyDateTime, AddressOrGroupList};

pub use self::syntax::{Address, AddressOrGroup, Mailbox};

pub mod syntax;

pub struct ParsedMessage<'a> {
    pub trace: Vec<Trace<'a>>,
    pub id: Option<String>,
    pub origination_date: AnyDateTime,
    pub from: MailboxList<'a>,
    pub sender: Option<MailboxRef<'a>>,
    pub to: AddressOrGroupList<'a>,
    pub subject: Option<String>,
    pub body: Body<'a>,
}

pub struct Trace<'a> {
    pub return_path: Option<PathRef<'a>>,
    pub received: ListOf<'a, Received<'a>>,
    pub resending: Vec<ResentInfo<'a>>,
}

pub struct ResentInfo<'a> {
    pub date: AnyDateTime,
    pub from: MailboxList<'a>,
    pub sender: Option<MailboxRef<'a>>,
    pub to: AddressOrGroupList<'a>,
    pub cc: AddressOrGroupList<'a>,
    pub bcc: AddressOrGroupList<'a>,
    pub id: Option<String>,
}

/// Message body
pub enum Body<'a> {
    /// Unknown format
    Unknown(&'a [u8]),
    /// MIME (RFC 2045)
    Mime(mime::Unparsed<'a>),
}

pub fn parse(message: &[u8]) -> Result<ParsedMessage> {
    let (header, body) = separate_message(message);
    let mut header = Buffer::new(header);

    let mut trace = vec![];
    while let Some(item) = parse_trace(&mut header)? {
        trace.push(item);
    }

    let mut origination_date = None;
    let mut from = None;
    let mut sender = None;
    let mut reply_to = None;
    let mut to = None;
    let mut cc = None;
    let mut bcc = None;
    let mut id = None;
    let mut in_reply_to = None;
    let mut references = None;
    let mut subject = None;
    let mut comments = vec![];
    let mut keywords = vec![];
    let mut mime_version = None;
    let mut content_type = None;
    let mut transfer_encoding = None;
    let mut content_id = None;
    let mut content_description = None;

    while !header.is_empty() {
        let offset = header.offset();
        match syntax::field(&mut header)? {
            Header::OriginationDate(value) =>
                origination_date.set_once(offset, "Origination-Date", value)?,
            Header::From(value) =>
                from.set_once(offset, "From", value)?,
            Header::Sender(value) =>
                sender.set_once(offset, "Sender", value)?,
            Header::ReplyTo(value) =>
                reply_to.set_once(offset, "Reply-To", value)?,
            Header::To(value) =>
                to.set_once(offset, "To", value)?,
            Header::CarbonCopy(value) =>
                cc.set_once(offset, "Carbon-Copy", value)?,
            Header::BlindCarbonCopy(value) =>
                bcc.set_once(offset, "Blind-Carbon-Copy", value)?,
            Header::MessageId(value) =>
                id.set_once(offset, "Message-ID", value.0.into())?,
            Header::InReplyTo(value) =>
                in_reply_to.set_once(offset, "InReply-To", value)?,
            Header::References(value) =>
                references.set_once(offset, "References", value)?,
            Header::Subject(value) =>
                subject.set_once(offset, "Subject", value.unfold().into())?,
            Header::Comments(value) => comments.push(value.unfold()),
            Header::Keywords(value) =>
                keywords.extend(value.iter().map(|keyword| keyword.unquote())),
            Header::ResentDate(_) => todo!(),
            Header::ResentFrom(_) => todo!(),
            Header::ResentSender(_) => todo!(),
            Header::ResentTo(_) => todo!(),
            Header::ResentCarbonCopy(_) => todo!(),
            Header::ResentBlindCarbonCopy(_) => todo!(),
            Header::ResentMessageId(_) => todo!(),
            Header::ReturnPath(_) => todo!(),
            Header::Received(_) => todo!(),
            Header::Mime(header) => match header {
                mime::Header::Version(value) =>
                    mime_version.set_once(offset, "MIME-Version", value)?,
                mime::Header::ContentType(value) =>
                    content_type.set_once(offset, "Content-Type", value)?,
                mime::Header::ContentTransferEncoding(value) =>
                    transfer_encoding.set_once(offset, "Content-Transfer-Encoding", value)?,
                mime::Header::ContentId(value) =>
                    content_id.set_once(offset, "Content-ID", value)?,
                mime::Header::ContentDescription(value) =>
                    content_description.set_once(offset, "Content-Description", value)?,
            },
            Header::Optional { name, body } => {
                log::trace!("unrecognized header {name}: {body:?}");
            }
        }
    }

    let origination_date = origination_date
        .ok_or_else(|| SyntaxErrorKind::custom("missing required header Origination-Date").at(0))?;
    let from = from
        .ok_or_else(|| SyntaxErrorKind::custom("missing required header From").at(0))?;

    let body = match mime_version {
        None => Body::Unknown(body),
        Some(version) => Body::Mime(mime::Unparsed {
            data: body,
            version,
            content_type: content_type.unwrap_or_default(),
            transfer_encoding,
        }),
    };

    Ok(ParsedMessage {
        trace,
        id,
        origination_date,
        from,
        sender,
        to: to.unwrap_or_default(),
        subject,
        body,
    })
}

/// Separate message into its header and body sections
fn separate_message(message: &[u8]) -> (&[u8], &[u8]) {
    match memmem::find(message, b"\r\n\r\n") {
        Some(cr) => (&message[..cr + 2], &message[cr + 4..]),
        None => (message, b""),
    }
}

fn parse_trace<'a>(header: &mut Buffer<'a>) -> Result<Option<Trace<'a>>> {
    // Trace fields
    let return_path = header.maybe(syntax::return_path);
    let received = header.list_of::<Received>(if return_path.is_some() { 1 } else { 0 }, usize::MAX, b"")?;

    if return_path.is_none() && received.is_empty() {
        return Ok(None);
    }

    // Optional fields
    let mut cursor = *header;
    while let Some(Header::Optional { .. }) = cursor.maybe(syntax::field) {
        *header = cursor;
    }

    // Resending data
    let mut resending = vec![];
    while let Some(info) = parse_resent_block(header)? {
        resending.push(info);
    }

    Ok(Some(Trace { return_path, received, resending }))
}

fn parse_resent_block<'a>(header: &mut Buffer<'a>) -> Result<Option<ResentInfo<'a>>> {
    let offset = header.offset();

    let mut date = None;
    let mut from = None;
    let mut sender = None;
    let mut to = None;
    let mut cc = None;
    let mut bcc = None;
    let mut id = None;

    let mut cursor = *header;

    while !header.is_empty() {
        match syntax::field(&mut cursor)? {
            Header::ResentDate(value) => {
                if date.is_some() {
                    break;
                }
                date = Some(value);
            }
            Header::ResentFrom(value) => {
                if from.is_some() {
                    break;
                }
                from = Some(value);
            }
            Header::ResentSender(value) => {
                if sender.is_some() {
                    break;
                }
                sender = Some(value);
            }
            Header::ResentTo(value) => {
                if to.is_some() {
                    break;
                }
                to = Some(value);
            }
            Header::ResentCarbonCopy(value) => {
                if cc.is_some() {
                    break;
                }
                cc = Some(value);
            }
            Header::ResentBlindCarbonCopy(value) => {
                if bcc.is_some() {
                    break;
                }
                bcc = Some(value);
            }
            Header::ResentMessageId(value) => {
                if id.is_some() {
                    break;
                }
                id = Some(value.0.into());
            }
            _ => break,
        }

        *header = cursor;
    }

    if date.is_none() && from.is_none() && sender.is_none() && to.is_none()
    && cc.is_none() && bcc.is_none() && id.is_none() {
        return Ok(None);
    }

    let date = date
        .ok_or_else(|| SyntaxErrorKind::custom("missing required header Resent-Date").at(offset))?;
    let from = from
        .ok_or_else(|| SyntaxErrorKind::custom("missing required header Resent-From").at(offset))?;

    Ok(Some(ResentInfo {
        date,
        from,
        sender,
        to: to.unwrap_or_default(),
        cc: cc.unwrap_or_default(),
        bcc: bcc.unwrap_or_default(),
        id,
    }))
}
