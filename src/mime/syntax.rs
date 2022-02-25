// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use std::str;

use crate::{syntax::*, mail::syntax as mail};

// -------------- RFC 2045: MIME Part One: Format of Internet Message Bodies ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MimeVersion {
    /// MIME 1.0
    Mime10,
}

pub fn version(buf: &mut Buffer) -> Result<MimeVersion> {
    // version := "MIME-Version" ":" 1*DIGIT "." 1*DIGIT
    // Note that despite comments and spaces not being present in grammar,
    // examples in the same section include them.
    buf.atomic(|buf| {
        let offset = buf.offset();
        buf.maybe(mail::cfws);
        let major = read_number(buf, 10, 1, 1)?;
        buf.maybe(mail::comment);
        buf.expect(b".")?;
        buf.maybe(mail::comment);
        let minor = read_number(buf, 10, 1, 1)?;
        buf.maybe(mail::cfws);

        match (major, minor) {
            (1, 0) => Ok(MimeVersion::Mime10),
            _ => Err(SyntaxErrorKind::custom(
                format!("unsupported MIME version {major}.{minor}")).at(offset)),
        }
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ContentType<'a> {
    pub type_: &'a str,
    pub subtype: &'a str,
    pub parameters: &'a [u8],
}

impl Default for ContentType<'_> {
    fn default() -> Self {
        ContentType {
            type_: "text",
            subtype: "plain",
            parameters: b"charset=us-ascii",
        }
    }
}

pub fn content_type<'a>(buf: &mut Buffer<'a>) -> Result<ContentType<'a>> {
    // content := "Content-Type" ":" type "/" subtype *(";" parameter)
    buf.atomic(|buf| {
        buf.maybe(mail::cfws);
        let type_ = token(buf)?;
        buf.expect(b"/")?;
        let subtype = token(buf)?;

        let parameters = buf.take_matching(|buf| {
            while buf.expect(b";").is_ok() {
                parameter(buf)?;
            }

            Ok(())
        })?;

        Ok(ContentType { type_, subtype, parameters })
    })
}

#[derive(Clone, Copy, Debug)]
pub struct Parameter<'a> {
    pub attribute: &'a str,
    pub value: mail::Quoted<'a>,
}

impl<'a> Parse<'a> for Parameter<'a> {
    fn parse(from: &mut Buffer<'a>) -> Result<Self> {
        parameter(from)
    }
}

fn parameter<'a>(buf: &mut Buffer<'a>) -> Result<Parameter<'a>> {
    // parameter := attribute "=" value
    // attribute := token
    // value     := token / quoted-string
    buf.atomic(|buf| {
        buf.maybe(mail::cfws);
        let attribute = token(buf)?;
        buf.expect(b"=")?;
        let value = token(buf).map(mail::Quoted).or_else(|_| mail::quoted_string(buf))?;
        buf.maybe(mail::cfws);
        Ok(Parameter { attribute, value })
    })
}

fn token<'a>(buf: &mut Buffer<'a>) -> Result<&'a str> {
    // token := 1*<any (US-ASCII) CHAR except SPACE, CTLs, or tspecials>
    buf.atomic(|buf| {
        let value = buf.take_while(
            |b, _| b.is_ascii() && !b.is_ascii_control() && b != b' ' && !is_tspecial(b));

        if value.is_empty() {
            buf.error("expected a token")
        } else {
            Ok(str::from_utf8(value).unwrap())
        }
    })
}

fn is_tspecial(ch: u8) -> bool {
    // tspecials := "(" / ")" / "<" / ">" / "@" / "," / ";" / ":" / "\" / <"> "/" / "[" / "]"
    //            / "?" / "="
    matches!(ch, b'(' | b')' | b'<' | b'>' | b'@' | b',' | b';' | b':' | b'\\' | b'"' | b'/' | b'['
        | b']' | b'?' | b'=')
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferEncoding {
    _7Bit,
    _8Bit,
    Binary,
    QuotedPrintable,
    Base64,
}

impl Default for TransferEncoding {
    fn default() -> Self {
        TransferEncoding::_7Bit
    }
}

pub fn content_transfer_encoding(buf: &mut Buffer) -> Result<TransferEncoding> {
    // encoding := "Content-Transfer-Encoding" ":" mechanism
    // mechanism := "7bit" / "8bit" / "binary" / "quoted-printable" / "base64" /
    buf.atomic(|buf| {
        buf.maybe(mail::cfws);

        let offset = buf.offset();
        let mechanism = token(buf)?;

        if mechanism.eq_ignore_ascii_case("7bit") {
            Ok(TransferEncoding::_7Bit)
        } else if mechanism.eq_ignore_ascii_case("8bit") {
            Ok(TransferEncoding::_8Bit)
        } else if mechanism.eq_ignore_ascii_case("binary") {
            Ok(TransferEncoding::Binary)
        } else if mechanism.eq_ignore_ascii_case("quoted-printable") {
            Ok(TransferEncoding::QuotedPrintable)
        } else if mechanism.eq_ignore_ascii_case("base64") {
            Ok(TransferEncoding::Base64)
        } else {
            Err(SyntaxErrorKind::custom(
                format!("unsupported transfer encoding {mechanism}")).at(offset))
        }
    })
}

#[derive(Clone, Copy, Debug)]
pub enum Header<'a> {
    Version(MimeVersion),
    ContentType(ContentType<'a>),
    ContentTransferEncoding(TransferEncoding),
    ContentId(mail::MessageIdRef<'a>),
    ContentDescription(mail::Folded<'a>),
}

pub fn header<'a>(name: &str, buf: &mut Buffer<'a>) -> Result<Option<Header<'a>>> {
    Ok(Some(if name.eq_ignore_ascii_case("MIME-Version") {
        Header::Version(version(buf)?)
    } else if name.eq_ignore_ascii_case("Content-Type") {
        Header::ContentType(content_type(buf)?)
    } else if name.eq_ignore_ascii_case("Content-Transfer-Encoding") {
        Header::ContentTransferEncoding(content_transfer_encoding(buf)?)
    } else if name.eq_ignore_ascii_case("Content-ID") {
        Header::ContentId(mail::msg_id(buf)?)
    } else if name.eq_ignore_ascii_case("Content-Description") {
        Header::ContentDescription(mail::unstructured(buf)?)
    } else {
        return Ok(None);
    }))
}
