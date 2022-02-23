// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use std::fmt;

pub fn maybe_ascii(ascii: &[u8]) -> MaybeAscii {
    MaybeAscii(ascii)
}

pub struct MaybeAscii<'a>(&'a [u8]);

impl fmt::Display for MaybeAscii<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &byte in self.0 {
            if byte.is_ascii() {
                write!(f, "{}", byte as char)?;
            } else {
                write!(f, "\\x{:02x}", byte)?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for MaybeAscii<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("\"")?;
        for &byte in self.0 {
            if byte.is_ascii_graphic() || byte == b' ' {
                write!(f, "{}", byte as char)?;
            } else {
                write!(f, "\\x{:02x}", byte)?;
            }
        }
        f.write_str("\"")?;
        Ok(())
    }
}
