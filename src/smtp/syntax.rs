// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use std::{fmt, net::{IpAddr, Ipv4Addr, Ipv6Addr}, str};

use crate::syntax::*;

pub enum ReversePathRef<'a> {
    Null,
    Mailbox(MailboxRef<'a>),
}

pub enum ReversePath {
    Null,
    Mailbox(Mailbox),
}

impl<'a> ReversePathRef<'a> {
    pub fn to_owned(&self) -> ReversePath {
        match self {
            ReversePathRef::Null => ReversePath::Null,
            ReversePathRef::Mailbox(mb) => ReversePath::Mailbox(mb.to_owned()),
        }
    }
}

impl ReversePath {
    pub fn borrow(&self) -> ReversePathRef {
        match self {
            ReversePath::Null => ReversePathRef::Null,
            ReversePath::Mailbox(mb) => ReversePathRef::Mailbox(mb.borrow()),
        }
    }
}

pub fn reverse_path<'a>(buf: &mut Buffer<'a>) -> Result<ReversePathRef<'a>> {
    // Reverse-path = Path / "<>"
    if buf.starts_with(b"<>") {
        buf.advance(2);
        return Ok(ReversePathRef::Null);
    }

    path(buf).map(ReversePathRef::Mailbox)
}

pub enum ForwardPathRef<'a> {
    Postmaster(Option<&'a str>),
    Mailbox(MailboxRef<'a>),
}

pub enum ForwardPath {
    Postmaster(Option<String>),
    Mailbox(Mailbox),
}

impl<'a> ForwardPathRef<'a> {
    pub fn to_owned(&self) -> ForwardPath {
        match self {
            ForwardPathRef::Postmaster(domain) => ForwardPath::Postmaster(domain.map(String::from)),
            ForwardPathRef::Mailbox(mb) => ForwardPath::Mailbox(mb.to_owned()),
        }
    }
}

impl ForwardPath {
    pub fn borrow(&self) -> ForwardPathRef {
        match self {
            ForwardPath::Postmaster(domain) => ForwardPathRef::Postmaster(domain.as_deref()),
            ForwardPath::Mailbox(mb) => ForwardPathRef::Mailbox(mb.borrow()),
        }
    }
}

pub fn forward_path<'a>(buf: &mut Buffer<'a>) -> Result<ForwardPathRef<'a>> {
    if buf.expect_caseless(b"<postmaster>").is_ok() {
        return Ok(ForwardPathRef::Postmaster(None));
    }

    let path = path(buf)?;

    if path.local.eq_ignore_ascii_case("postmaster") {
        match path.location {
            DomainRefOrAddr::Domain(domain) => Ok(ForwardPathRef::Postmaster(Some(domain))),
            DomainRefOrAddr::Addr(_) => buf.error("expected domain name"),
        }
    } else {
        Ok(ForwardPathRef::Mailbox(path))
    }
}

pub fn path<'a>(buf: &mut Buffer<'a>) -> Result<MailboxRef<'a>> {
    // Path = "<" [ A-d-l ":" ] Mailbox ">"
    buf.atomic(|buf| {
        buf.expect(b"<")?;

        // A-d-l     = At-domain *( "," At-domain )
        // At-domain = "@" Domain
        if buf.starts_with(b"@") {
            loop {
                buf.expect(b"@")?;
                domain(buf)?;

                if buf.starts_with(b":") {
                    buf.advance(1);
                    break;
                } else if buf.starts_with(b",") {
                    buf.advance(1);
                } else {
                    return buf.error("expected one of ':' or ','");
                }
            }
        }

        let mailbox = mailbox(buf)?;
        buf.expect(b">")?;

        Ok(mailbox)
    })
}

pub fn parameter<'a>(buf: &mut Buffer<'a>) -> Result<(&'a [u8], &'a [u8])> {
    // Mail-parameters = esmtp-param *(SP esmtp-param)
    // Rcpt-parameters = esmtp-param *(SP esmtp-param)
    // esmtp-param     = esmtp-keyword ["=" esmtp-value]
    // esmtp-keyword   = (ALPHA / DIGIT) *(ALPHA / DIGIT / "-")
    // esmtp-value     = 1*(%d33-60 / %d62-126)
    buf.atomic(|buf| {
        let keyword = buf.take_while(|c, inx| c.is_ascii_alphanumeric() || c == b'-' && inx > 0);
        if keyword.is_empty() {
            return buf.error("expected a keyword");
        }

        buf.expect(b"=")?;

        let value = buf.take_while(|c, _| matches!(c, 33..=60 | 62..=126));
        if value.is_empty() {
            return buf.error("expected a value");
        }

        Ok((keyword, value))
    })
}

// Keyword        = Ldh-str

// Argument       = Atom

#[derive(Debug)]
pub enum DomainRefOrAddr<'a> {
    Domain(&'a str),
    Addr(IpAddr),
}

pub enum DomainOrAddr {
    Domain(String),
    Addr(IpAddr),
}

impl<'a> DomainRefOrAddr<'a> {
    pub fn to_owned(&self) -> DomainOrAddr {
        match *self {
            DomainRefOrAddr::Domain(domain) => DomainOrAddr::Domain(domain.into()),
            DomainRefOrAddr::Addr(addr) => DomainOrAddr::Addr(addr),
        }
    }
}

impl DomainOrAddr {
    pub fn borrow(&self) -> DomainRefOrAddr {
        match self {
            DomainOrAddr::Domain(ref domain) => DomainRefOrAddr::Domain(domain),
            DomainOrAddr::Addr(addr) => DomainRefOrAddr::Addr(*addr),
        }
    }
}

impl fmt::Display for DomainRefOrAddr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DomainRefOrAddr::Domain(domain) => domain.fmt(f),
            DomainRefOrAddr::Addr(addr) => addr.fmt(f),
        }
    }
}

impl fmt::Display for DomainOrAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.borrow().fmt(f)
    }
}

pub fn domain_or_address<'a>(buf: &mut Buffer<'a>) -> Result<DomainRefOrAddr<'a>> {
    if buf.starts_with(b"[") {
        address_literal(buf).map(DomainRefOrAddr::Addr)
    } else {
        domain(buf).map(DomainRefOrAddr::Domain)
    }
}

pub fn domain<'a>(buf: &mut Buffer<'a>) -> Result<&'a str> {
    let value = buf.take_matching(|buf| {
        // Domain = sub-domain *("." sub-domain)
        loop {
            // sub-domain = Let-dig [Ldh-str]
            // Let-dig    = ALPHA / DIGIT
            if !buf[0].is_ascii_alphanumeric() {
                return buf.error("expected letter or digit");
            }
            buf.advance(1);

            // Ldh-str = *( ALPHA / DIGIT / "-" ) Let-dig
            let ldh = buf.take_while(|ch, _| ch.is_ascii_alphabetic() || ch == b'-');

            if ldh.ends_with(b"-") {
                return buf.error("expected letter or digit following '-'");
            }

            if buf.is_empty() || buf.expect(b".").is_err() {
                break;
            }
        }

        Ok(())
    })?;
    Ok(str::from_utf8(value).unwrap())
}

pub fn address_literal(buf: &mut Buffer) -> Result<IpAddr> {
    // address-literal = "[" ( IPv4-address-literal / IPv6-address-literal ) "]"
    buf.atomic(|buf| {
        buf.expect(b"[")?;

        // IPv6-address-literal = "IPv6:" IPv6-addr
        let addr = if buf.starts_with(b"IPv6:") {
            buf.advance(5);
            address_ipv6(buf)?.into()
        } else {
            address_ipv4(buf)?.into()
        };

        buf.expect(b"]")?;

        Ok(addr)
    })
}

pub fn address_ipv4(buf: &mut Buffer) -> Result<Ipv4Addr> {
    // IPv4-address-literal = Snum 3("."  Snum)
    // Snum                 = 1*3DIGIT
    buf.atomic(|buf| {
        let a = read_number(buf, 10, 1, 3)?;
        let b = read_number(buf, 10, 1, 3)?;
        let c = read_number(buf, 10, 1, 3)?;
        let d = read_number(buf, 10, 1, 3)?;
        Ok(Ipv4Addr::new(a, b, c, d))
    })
}

pub fn address_ipv6(buf: &mut Buffer) -> Result<Ipv6Addr> {
    // IPv6-addr      = IPv6-full / IPv6-comp / IPv6v4-full / IPv6v4-comp
    // IPv6-full      = IPv6-hex 7(":" IPv6-hex)
    // IPv6-comp      = [IPv6-hex *5(":" IPv6-hex)] "::" [IPv6-hex *5(":" IPv6-hex)]
    // IPv6v4-full    = IPv6-hex 5(":" IPv6-hex) ":" IPv4-address-literal
    // IPv6v4-comp    = [IPv6-hex *3(":" IPv6-hex)] "::"
    //                  [IPv6-hex *3(":" IPv6-hex) ":"]
    //                  IPv4-address-literal

    // Implementation based on Rust's <Ipv6Addr as FromStr>.

    /// Read a chunk of an IPv6 address into `groups`. Returns the number of
    /// groups read, along with a bool indicating if an embedded trailing IPv4
    /// address was read. Specifically, read a series of colon-separated IPv6
    /// groups (0x0000 - 0xFFFF), with an optional trailing embedded IPv4
    /// address.
    fn read_groups(buf: &mut Buffer, groups: &mut [u16]) -> (usize, bool) {
        let limit = groups.len();

        for (i, slot) in groups.iter_mut().enumerate() {
            // Try to read a trailing embedded IPv4 address. There must be at
            // least two groups left.
            if i < limit - 1 {
                let mut cursor = *buf;
                if let Ok(addr) = address_ipv4(&mut cursor) {
                    let [one, two, three, four] = addr.octets();
                    groups[i + 0] = u16::from_be_bytes([one, two]);
                    groups[i + 1] = u16::from_be_bytes([three, four]);
                    *buf = cursor;
                    return (i + 2, true);
                }
            }

            let group = buf.atomic(|buf| {
                if i > 0 {
                    buf.expect(b":")?;
                }

                read_number(buf, 16, 1, 4)
            }).ok();

            match group {
                Some(g) => *slot = g,
                None => return (i, false),
            }
        }

        (groups.len(), false)
    }

    buf.atomic(|buf| {
        // Read the front part of the address; either the whole thing, or up
        // to the first ::
        let mut head = [0; 8];
        let (head_size, head_ipv4) = read_groups(buf, &mut head);

        if head_size == 8 {
            return Ok(head.into());
        }

        // IPv4 part is not allowed before `::`
        if head_ipv4 {
            return buf.error("IP v4 address may not be followed by '::'");
        }

        // Read `::` if previous code parsed less than 8 groups.
        // `::` indicates one or more groups of 16 bits of zeros.
        buf.expect(b"::")?;

        // Read the back part of the address. The :: must contain at least one
        // set of zeroes, so our max length is 7.
        let mut tail = [0; 7];
        let limit = 8 - (head_size + 1);
        let (tail_size, _) = read_groups(buf, &mut tail[..limit]);

        // Concat the head and tail of the IP address
        head[(8 - tail_size)..8].copy_from_slice(&tail[..tail_size]);

        Ok(head.into())
    })
}

pub struct MailboxRef<'a> {
    pub local: &'a str,
    pub location: DomainRefOrAddr<'a>,
}

pub struct Mailbox {
    pub local: String,
    pub location: DomainOrAddr,
}

impl<'a> MailboxRef<'a> {
    pub fn to_owned(&self) -> Mailbox {
        Mailbox {
            local: self.local.into(),
            location: self.location.to_owned(),
        }
    }
}

impl Mailbox {
    fn borrow(&self) -> MailboxRef {
        MailboxRef {
            local: &self.local,
            location: self.location.borrow(),
        }
    }
}

pub fn mailbox<'a>(buf: &mut Buffer<'a>) -> Result<MailboxRef<'a>> {
    // Mailbox    = Local-part "@" ( Domain / address-literal )
    // Local-part = Dot-string / Quoted-string
    buf.atomic(|buf| {
        let local = quoted_string(buf).or_else(|_| dot_string(buf))?;
        buf.expect(b"@")?;
        let location = domain_or_address(buf)?;
        Ok(MailboxRef { local, location })
    })
}

pub fn dot_string<'a>(buf: &mut Buffer<'a>) -> Result<&'a str> {
    let value = buf.take_matching(|buf| {
        atom(buf)?;
        while buf.expect(b".").is_ok() {
            atom(buf)?;
        }
        Ok(())
    })?;
    Ok(str::from_utf8(value).unwrap())
}

pub fn quoted_string<'a>(buf: &mut Buffer<'a>) -> Result<&'a str> {
    let value = buf.take_matching(|buf| {
        // Quoted-string = DQUOTE *QcontentSMTP DQUOTE

        buf.expect(b"\"")?;

        // QcontentSMTP = qtextSMTP / quoted-pairSMTP
        while !buf.is_empty() && buf[0] != b'"' {
            match buf[0] {
                // qtextSMTP = %d32-33 / %d35-91 / %d93-126
                32..=33 | 35..=91 | 93..=126 => buf.advance(1),
                // quoted-pairSMTP = %d92 %d32-126
                92 if buf.len() > 1 => match buf[1] {
                    32..=126 => buf.advance(2),
                    _ => return buf.error("invalid escape sequence"),
                },
                _ => return buf.error("invalid character in quoted string"),
            }
        }

        buf.expect(b"\"")?;

        Ok(())
    })?;
    Ok(str::from_utf8(value).unwrap())
}

pub fn string<'a>(buf: &mut Buffer<'a>) -> Result<&'a str> {
    // String = Atom / Quoted-string
    atom(buf).or_else(|_| quoted_string(buf))
}
