use std::{error::Error, fmt};

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
