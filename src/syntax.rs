// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

//! Utilities for parsing

use std::{str, ops, borrow::Cow, marker::PhantomData, fmt};
use thiserror::Error;

use crate::util;
use self::SyntaxErrorKind::*;

pub type Result<T, E = SyntaxError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
#[error("Syntax error - at byte {byte} - {kind}")]
pub struct SyntaxError {
    byte: usize,
    kind: SyntaxErrorKind,
}

#[derive(Debug, Error)]
pub enum SyntaxErrorKind {
    #[error("expected {:?}", util::maybe_ascii(.0))]
    Expected(&'static [u8]),
    #[error("unexpected characters")]
    ExpectedEnd,
    #[error("{0}")]
    Custom(Cow<'static, str>),
}

impl From<&'static str> for SyntaxErrorKind {
    fn from(error: &'static str) -> Self {
        SyntaxErrorKind::Custom(error.into())
    }
}

impl From<String> for SyntaxErrorKind {
    fn from(error: String) -> Self {
        SyntaxErrorKind::Custom(error.into())
    }
}

impl SyntaxErrorKind {
    pub fn custom(error: impl Into<Cow<'static, str>>) -> Self {
        Custom(error.into())
    }

    pub fn at(self, byte: usize) -> SyntaxError {
        SyntaxError { byte, kind: self }
    }
}

pub trait Parse<'a>: Sized {
    fn parse(from: &mut Buffer<'a>) -> Result<Self>;
}

#[derive(Clone, Copy)]
pub struct Buffer<'a> {
    offset: usize,
    data: &'a [u8],
}

impl<'a> Buffer<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Buffer { offset: 0, data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn error<T>(&self, kind: impl Into<SyntaxErrorKind>) -> Result<T> {
        Err(kind.into().at(self.offset))
    }

    /// Advance this slice by `number` positions
    pub fn advance(&mut self, number: usize) {
        self.data = &self.data[number..];
        self.offset += number;
    }

    pub fn take(&mut self, number: usize) -> &'a [u8] {
        let value = &self.data[..number];
        self.advance(number);
        value
    }

    /// Execute `f`, advancing `self` only if it succeeds
    pub fn atomic<T: 'a>(&mut self, f: impl FnOnce(&mut Buffer<'a>) -> Result<T>) -> Result<T> {
        let mut cursor = *self;
        let value = f(&mut cursor)?;
        *self = cursor;
        Ok(value)
    }

    /// Return `Ok(())` and advance this slice if it begins with `needle`
    pub fn expect(&mut self, needle: &'static [u8]) -> Result<()> {
        if self.data.starts_with(needle) {
            self.advance(needle.len());
            Ok(())
        } else {
            self.error(Expected(needle))
        }
    }

    /// Return `Ok(())` and advance this slice if it begins (case insensitive)
    /// with `needle`
    pub fn expect_caseless(&mut self, needle: &'static [u8]) -> Result<()> {
        if needle.len() <= self.len() && self.data[..needle.len()].eq_ignore_ascii_case(needle) {
            self.advance(needle.len());
            Ok(())
        } else {
            self.error(Expected(needle))
        }
    }

    /// Return `Ok(())` if this slice is empty
    pub fn expect_empty(&self) -> Result<()> {
        if self.is_empty() {
            Ok(())
        } else {
            self.error(ExpectedEnd)
        }
    }

    /// Return longest prefix whose characters match `test`, advancing this
    /// slice by its length
    pub fn take_while(&mut self, mut test: impl FnMut(u8, usize) -> bool) -> &'a [u8] {
        let mut offset = 0;

        while offset < self.len() && test(self.data[offset], offset) {
            offset += 1;
        }

        let result = &self.data[..offset];
        self.advance(offset);
        result
    }

    pub fn take_matching(&mut self, f: impl FnOnce(&mut Self) -> Result<()>) -> Result<&'a [u8]> {
        let mut cursor = *self;
        f(&mut cursor)?;
        let length = self.len() - cursor.len();
        Ok(self.take(length))
    }

    /// Execute `f`, advancing `self` only if it succeeds
    pub fn maybe<T: 'a>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Option<T> {
        self.atomic(f).ok()
    }

    pub fn list_of<T: Parse<'a>>(&mut self, min: usize, max: usize, separator: &'static [u8])
    -> Result<ListOf<'a, T>> {
        let items = self.take_matching(|slf| {
            let mut count = 0;

            while slf.maybe(T::parse).is_some() {
                count += 1;
            }

            if count < min {
                slf.error(format!("expected at least {min} elements"))
            } else if count > max {
                slf.error(format!("expected at most {max} elements"))
            } else {
                Ok(())
            }
        })?;
        Ok(ListOf::new(separator, items))
    }
}

impl ops::Deref for Buffer<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

pub fn read_number<T>(buf: &mut Buffer, radix: u32, min_digits: usize, max_digits: usize) -> Result<T>
where
    T: TryFrom<u32> + 'static,
    T::Error: std::fmt::Display,
{
    buf.atomic(|buf| {
        let mut value: u32 = 0;
        let mut count = 0;

        while !buf.is_empty() && count < max_digits && char::from(buf[0]).is_digit(radix) {
            value *= radix;
            value += char::from(buf[0]).to_digit(radix).unwrap();
            count += 1;
            buf.advance(1);
        }

        if count < min_digits {
            buf.error(format!("expected at least {} digit{}", min_digits,
                if min_digits == 1 { "" } else { "s" }))
        } else {
            T::try_from(value).map_err(|err| Custom(err.to_string().into()).at(buf.offset))
        }
    })
}

/// List of parseable items separated by commas
pub struct ListOf<'a, T> {
    items: &'a [u8],
    separator: &'static [u8],
    _type: PhantomData<&'a [T]>,
}

impl<'a, T> ListOf<'a, T> {
    const fn new(separator: &'static [u8], items: &'a [u8]) -> Self {
        ListOf { items, separator, _type: PhantomData }
    }

    pub const fn empty() -> Self {
        ListOf::new(b"", b"")
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<'a, T: Parse<'a>> ListOf<'a, T> {
    pub fn iter<'c>(&'c self) -> impl Iterator<Item = T> + 'c
    where
        'c: 'a,
    {
        let mut items = Buffer::new(self.items);
        let mut first = true;
        std::iter::from_fn(move || {
            if items.is_empty() {
                return None;
            }

            if !first {
                items.expect(self.separator).expect("invalid pre-parsed string");
            } else {
                first = false;
            }

            Some(T::parse(&mut items).expect("invalid pre-parsed string"))
        })
    }
}

impl<T> Clone for ListOf<'_, T> {
    #[inline]
    fn clone(&self) -> Self {
        ListOf { items: self.items, separator: self.separator, _type: PhantomData }
    }
}

impl<T> Copy for ListOf<'_, T> {
}

impl<T> Default for ListOf<'_, T> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<T> fmt::Debug for ListOf<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!("List<{}>", std::any::type_name::<T>()))
            .field("separator", &util::maybe_ascii(self.separator))
            .field("items", &util::maybe_ascii(self.items))
            .finish()
    }
}

// ---------------------------------------------------------------- RFC 5234 ---

#[inline]
pub fn is_wsp(c: u8) -> bool {
    matches!(c, b' ' | b'\t')
}

pub fn wsp(buf: &mut Buffer) -> Result<()> {
    if !buf.is_empty() && is_wsp(buf[0]) {
        buf.advance(1);
        Ok(())
    } else {
        buf.error("expected one of ' ' or '\\t'")
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

pub fn atom<'a>(buf: &mut Buffer<'a>) -> Result<&'a str> {
    // atom = 1*atext
    buf.atomic(|buf| {
        let text = buf.take_while(|b, _| is_atext(b));

        if text.is_empty() {
            buf.error("expected an atom")
        } else {
            Ok(str::from_utf8(text).unwrap())
        }
    })
}

pub fn dot_atom<'a>(buf: &mut Buffer<'a>) -> Result<&'a str> {
    // dot-atom = 1*atext *("." 1*atext)
    buf.atomic(|buf| {
        let text = buf.take_while(|b, _| b == b'.' || is_atext(b));

        if text.is_empty() {
            buf.error("expected an atom")
        } else {
            Ok(str::from_utf8(text).unwrap())
        }
    })
}

#[inline]
pub fn is_vchar(b: u8) -> bool {
    matches!(b, 0x21..=0x7e)
}
