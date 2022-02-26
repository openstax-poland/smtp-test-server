// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use std::{error::Error, fmt, borrow::Cow};
use thiserror::Error;

use crate::util;

use super::syntax::TransferEncoding;

impl TransferEncoding {
    pub fn decode(self, data: &[u8]) -> Result<Cow<[u8]>, DecodeError> {
        match self {
            TransferEncoding::_7Bit => {
                if data.iter().any(|&b| b > 127) {
                    log::trace!("8-bit character in 7-bit data");
                    return Err(DecodeErrorKind::IllegalCharacter.into());
                }
                check_7_8_bit_data(data)?;
                Ok(data.into())
            }
            TransferEncoding::_8Bit => {
                check_7_8_bit_data(data)?;
                Ok(data.into())
            }
            TransferEncoding::Binary => Ok(data.into()),
            TransferEncoding::QuotedPrintable => Ok(quoted_printable::decode(data)?.into()),
            TransferEncoding::Base64 => {
                let data: Vec<u8> = data.iter()
                    .copied()
                    .filter(|b| !b.is_ascii_whitespace())
                    .collect();
                Ok(base64::decode(&data)?.into())
            }
        }
    }
}

fn check_7_8_bit_data(data: &[u8]) -> Result<(), DecodeError> {
    for line in data.split_inclusive(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r\n").unwrap_or(line);

        if line.len() > 998 {
            return Err(DecodeErrorKind::LineOverflow.into());
        }

        if line.iter().any(|&b| matches!(b, b'\0' | b'\r' | b'\n')) {
            log::trace!("illegal character on line: {:?}", util::maybe_ascii(line));
            return Err(DecodeErrorKind::IllegalCharacter.into());
        }
    }

    Ok(())
}

mod quoted_printable {
    use super::{DecodeError, DecodeErrorKind};

    pub fn decode(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
        let data = std::str::from_utf8(data).expect("TODO");
        let mut result = Vec::with_capacity(data.len());

        for mut line in data.split_inclusive("\r\n") {
            if line.len() > 80 /* 78 + \r\n */ {
                return Err(DecodeErrorKind::LineOverflow.into());
            }

            while !line.is_empty() {
                if line == "=\r\n" {
                    break;
                }

                if line.starts_with('=') {
                    let h = line.as_bytes()[1];
                    let l = line.as_bytes()[2];

                    if !matches!(h, b'0'..=b'9' | b'A'..=b'F')
                    || !matches!(l, b'0'..=b'9' | b'A'..=b'F') {
                        return Err(DecodeErrorKind::InvalidEscapeSequence.into());
                    }

                    let byte = u8::from_str_radix(&line[1..3], 16).unwrap();
                    result.push(byte);

                    line = &line[3..];
                } else {
                    let next = line.find('=').unwrap_or(line.len());
                    let fragment = &line[..next];
                    line = &line[next..];

                    if fragment.trim_end_matches("\r\n")
                        .bytes()
                        .any(|b| b.is_ascii_control() && b != b'\t' || b > 126)
                    {
                        return Err(DecodeErrorKind::IllegalCharacter.into());
                    }

                    result.extend_from_slice(fragment.as_bytes());
                }
            }
        }

        Ok(result)
    }
}

#[derive(Debug)]
pub struct DecodeError(DecodeErrorKind);

#[derive(Debug)]
enum DecodeErrorKind {
    Base64(base64::DecodeError),
    LineOverflow,
    InvalidEscapeSequence,
    IllegalCharacter,
}

impl From<DecodeErrorKind> for DecodeError {
    fn from(error: DecodeErrorKind) -> Self {
        DecodeError(error)
    }
}

impl From<base64::DecodeError> for DecodeError {
    fn from(error: base64::DecodeError) -> Self {
        DecodeErrorKind::Base64(error).into()
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            DecodeErrorKind::Base64(ref error) => error.fmt(f),
            DecodeErrorKind::LineOverflow => f.write_str("line too long"),
            DecodeErrorKind::InvalidEscapeSequence => f.write_str("invalid escape sequence"),
            DecodeErrorKind::IllegalCharacter => f.write_str("illegal character"),
        }
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.0 {
            DecodeErrorKind::Base64(ref error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Charset {
    UsAscii,
    Iso8859_2,
    Iso8859_3,
    Iso8859_4,
    Iso8859_5,
    Iso8859_6,
    Iso8859_7,
    Iso8859_8,
    Iso8859_10,
    Iso8859_13,
    Iso8859_14,
    Iso8859_15,
    Iso8859_16,
    Utf8,
}

#[derive(Debug, Error)]
#[error("malformed text data")]
pub struct CharsetError;

impl Charset {
    pub fn decode(self, data: &[u8]) -> Result<Cow<str>, CharsetError> {
        use encoding_rs::*;

        let charset = match self {
            Charset::UsAscii => {
                return if data.iter().all(u8::is_ascii) {
                    Ok(std::str::from_utf8(data).unwrap().into())
                } else {
                    Err(CharsetError)
                };
            }
            Charset::Iso8859_2 => ISO_8859_2,
            Charset::Iso8859_3 => ISO_8859_3,
            Charset::Iso8859_4 => ISO_8859_4,
            Charset::Iso8859_5 => ISO_8859_5,
            Charset::Iso8859_6 => ISO_8859_6,
            Charset::Iso8859_7 => ISO_8859_7,
            Charset::Iso8859_8 => ISO_8859_8,
            Charset::Iso8859_10 => ISO_8859_10,
            Charset::Iso8859_13 => ISO_8859_13,
            Charset::Iso8859_14 => ISO_8859_14,
            Charset::Iso8859_15 => ISO_8859_15,
            Charset::Iso8859_16 => ISO_8859_16,
            Charset::Utf8 =>
                return std::str::from_utf8(data).map(Cow::from).map_err(|_| CharsetError),
        };

        charset.decode_without_bom_handling_and_without_replacement(data).ok_or(CharsetError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_printable() {
        assert_eq!(
            quoted_printable::decode(
                b"Now's the time =\r\nfor all folk to come=\r\n to the aid of their country.",
            ).unwrap(),
            (b"Now's the time for all folk to come to the aid of their country."),
        );
    }
}
