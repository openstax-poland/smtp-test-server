// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use std::str;

use crate::{syntax::*, mail::syntax as mail, mime::encoding::Charset};
use super::encoding::CharsetError;

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
            parameters: b";charset=us-ascii",
        }
    }
}

impl<'a> ContentType<'a> {
    pub fn parameters(&self) -> impl Iterator<Item = Parameter<'a>> {
        let mut buf = Buffer::new(self.parameters);

        std::iter::from_fn(move || {
            if buf.expect(b";").is_ok() {
                Some(parameter(&mut buf).unwrap())
            } else {
                None
            }
        })
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

// --- RFC 2047: MIME Part Three: Message Header Extensions for Non-ASCII Text -

#[derive(Clone, Copy, Debug)]
pub struct EncodedWord<'a> {
    pub charset: &'a str,
    pub encoding: WordEncoding,
    pub encoded_text: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WordEncoding {
    Base64,
    Quoted,
}

impl<'a> EncodedWord<'a> {
    pub fn decode(self) -> Result<String, CharsetError> {
        let charset = Charset::by_name(self.charset).ok_or(CharsetError)?;

        let data = match self.encoding {
            WordEncoding::Base64 => base64::decode(self.encoded_text).map_err(|_| CharsetError)?,
            WordEncoding::Quoted => {
                let mut result = Vec::with_capacity(self.encoded_text.len());
                let mut rest = self.encoded_text;

                while let Some(inx) = rest.iter().position(|b| matches!(b, b'=' | b'_')) {
                    result.extend_from_slice(&rest[..inx]);

                    match rest[inx] {
                        b'_' => {
                            result.push(32);
                            rest = &rest[inx + 1..];
                        }
                        b'=' if rest.len() > inx + 3 => {
                            let byte = std::str::from_utf8(&rest[inx + 1..inx + 3])
                                .map_err(|_| CharsetError)?;
                            let byte = u8::from_str_radix(byte, 16)
                                .map_err(|_| CharsetError)?;
                            result.push(byte);
                            rest = &rest[inx + 3..];
                        }
                        _ => return Err(CharsetError),
                    }
                }

                result.extend_from_slice(rest);

                result
            }
        };

        charset.decode(&data).map(|d| d.into_owned())
    }
}

pub fn encoded_word<'a>(buf: &mut Buffer<'a>) -> Result<EncodedWord<'a>> {
    buf.atomic(|buf| {
        let start = buf.offset();

        buf.expect(b"=?")?;
        let charset = token(buf)?;
        buf.expect(b"?")?;

        let encoding = token(buf)?;
        let encoding = match_ignore_ascii_case! { encoding;
            "B" => WordEncoding::Base64,
            "Q" => WordEncoding::Quoted,
            _ => return buf.error(format!("unknown encoding {encoding:?}")),
        };

        buf.expect(b"?")?;
        let encoded_text = buf.take_while(|b, _| b.is_ascii_graphic() && b != b' ' && b != b'?');
        buf.expect(b"?=")?;

        let len = buf.offset() - start;
        if len > 76 {
            buf.error("too long encoded-word")
        } else {
            Ok(EncodedWord { charset, encoding, encoded_text })
        }
    })
}
