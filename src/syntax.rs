//! Utilities for parsing

use std::str;

pub type Result<T, E = SyntaxError> = std::result::Result<T, E>;

#[derive(Debug)]
pub struct SyntaxError;

pub trait Parse<'a>: Sized {
    fn parse(from: &mut &'a [u8]) -> Result<Self>;
}

pub trait SliceExt<'a> {
    /// Advance this slice by `number` positions
    fn advance(&mut self, number: usize);

    fn take(&mut self, number: usize) -> &'a [u8];

    /// Execute `f`, advancing `self` only if it succeeds
    fn atomic<T: 'a>(&mut self, f: impl FnOnce(&mut &'a [u8]) -> Result<T>) -> Result<T>;

    /// Return `Ok(())` and advance this slice if it begins with `needle`
    fn expect(&mut self, needle: &[u8]) -> Result<()>;

    /// Return `Ok(())` and advance this slice if it begins (case insensitive)
    /// with `needle`
    fn expect_caseless(&mut self, needle: &[u8]) -> Result<()>;

    /// Return `Ok(())` if this slice is empty
    fn expect_empty(&self) -> Result<()>;

    /// Return longest prefix whose characters match `test`, advancing this
    /// slice by its length
    fn take_while(&mut self, test: impl FnMut(u8, usize) -> bool) -> &'a [u8];

    fn take_matching(&mut self, f: impl FnOnce(&mut &'a [u8]) -> Result<()>) -> Result<&'a [u8]>;

    /// Execute `f`, advancing `self` only if it succeeds
    fn maybe<T: 'a>(&mut self, f: impl FnOnce(&mut &'a [u8]) -> Result<T>) -> Option<T> {
        self.atomic(f).ok()
    }
}

impl<'a> SliceExt<'a> for &'a [u8] {
    fn atomic<T: 'a>(&mut self, f: impl FnOnce(&mut &'a [u8]) -> Result<T>) -> Result<T> {
        let mut cursor = *self;
        let value = f(&mut cursor)?;
        *self = cursor;
        Ok(value)
    }

    fn take(&mut self, number: usize) -> &'a [u8] {
        let value = &self[..number];
        self.advance(number);
        value
    }

    fn advance(&mut self, by: usize) {
        *self = &self[by..];
    }

    fn expect(&mut self, needle: &[u8]) -> Result<()> {
        if self.starts_with(needle) {
            self.advance(needle.len());
            Ok(())
        } else {
            Err(SyntaxError)
        }
    }

    fn expect_caseless(&mut self, needle: &[u8]) -> Result<()> {
        if needle.len() <= self.len() && self[..needle.len()].eq_ignore_ascii_case(needle) {
            self.advance(needle.len());
            Ok(())
        } else {
            Err(SyntaxError)
        }
    }

    fn expect_empty(&self) -> Result<()> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(SyntaxError)
        }
    }

    fn take_while(&mut self, mut test: impl FnMut(u8, usize) -> bool) -> &'a [u8] {
        let mut offset = 0;

        while offset < self.len() && test(self[offset], offset) {
            offset += 1;
        }

        let result = &self[..offset];
        self.advance(offset);
        result
    }

    fn take_matching(&mut self, f: impl FnOnce(&mut &'a [u8]) -> Result<()>) -> Result<&'a [u8]> {
        let mut cursor = *self;
        f(&mut cursor)?;
        let length = self.len() - cursor.len();
        Ok(self.take(length))
    }
}

pub fn read_number<T>(buf: &mut &[u8], radix: u32, min_digits: usize, max_digits: usize) -> Result<T>
where
    T: TryFrom<u32> + 'static,
{
    buf.atomic(|line| {
        let mut value: u32 = 0;
        let mut count = 0;

        while !line.is_empty() && count < max_digits && char::from(line[0]).is_digit(radix) {
            value *= radix;
            value += char::from(line[0]).to_digit(radix).unwrap();
            count += 1;
            line.advance(1);
        }

        if count < min_digits {
            Err(SyntaxError)
        } else {
            T::try_from(value).map_err(|_| SyntaxError)
        }
    })
}

// ---------------------------------------------------------------- RFC 5234 ---

#[inline]
pub fn is_wsp(c: u8) -> bool {
    matches!(c, b' ' | b'\t')
}

pub fn wsp(buf: &mut &[u8]) -> Result<()> {
    if !buf.is_empty() && is_wsp(buf[0]) {
        buf.advance(1);
        Ok(())
    } else {
        Err(SyntaxError)
    }
}

#[inline]
pub fn is_atext(b: u8) -> bool {
    // atext = ALPHA / DIGIT / "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "/" /
    //         "=" / "?" / "^" / "_" / "`" / "{" / "|" / "}" / "~"
    match b {
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'/' | b'=' | b'?' |
        b'^' | b'_' | b'`' | b'{' | b'|' | b'}' | b'~' => true,
        _ => b.is_ascii_alphanumeric(),
    }
}

pub fn atom<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    // atom = 1*atext
    buf.atomic(|line| {
        let text = line.take_while(|b, _| is_atext(b));

        if text.is_empty() {
            Err(SyntaxError)
        } else {
            Ok(str::from_utf8(text).unwrap())
        }
    })
}

pub fn dot_atom<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    // dot-atom = 1*atext *("." 1*atext)
    buf.atomic(|line| {
        let text = line.take_while(|b, _| b == b'.' || is_atext(b));

        if text.is_empty() {
            Err(SyntaxError)
        } else {
            Ok(str::from_utf8(text).unwrap())
        }
    })
}

#[inline]
pub fn is_vchar(b: u8) -> bool {
    matches!(b, 0x21..=0x7e)
}
