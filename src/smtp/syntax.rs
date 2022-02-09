use std::{fmt, net::{IpAddr, Ipv4Addr, Ipv6Addr}, str};

pub type Result<T, E = SyntaxError> = std::result::Result<T, E>;

pub struct SyntaxError;

pub trait SliceExt<'a> {
    /// Advance this slice by `number` positions
    fn advance(&mut self, number: usize);

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
}

impl<'a> SliceExt<'a> for &'a [u8] {
    fn atomic<T: 'a>(&mut self, f: impl FnOnce(&mut &'a [u8]) -> Result<T>) -> Result<T> {
        let mut cursor = *self;
        let value = f(&mut cursor)?;
        *self = cursor;
        Ok(value)
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
}

pub enum ReversePathRef<'a> {
    Null,
    Mailbox(MailboxRef<'a>),
}

pub enum ReversePath {
    Null,
    Mailbox(Mailbox),
}

impl<'a> ReversePathRef<'a> {
    pub fn to_owned(self) -> ReversePath {
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

pub fn reverse_path<'a>(line: &mut &'a [u8]) -> Result<ReversePathRef<'a>> {
    // Reverse-path = Path / "<>"
    if line.starts_with(b"<>") {
        line.advance(2);
        return Ok(ReversePathRef::Null);
    }

    path(line).map(ReversePathRef::Mailbox)
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
    pub fn to_owned(self) -> ForwardPath {
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

pub fn forward_path<'a>(line: &mut &'a [u8]) -> Result<ForwardPathRef<'a>> {
    if line.expect_caseless(b"<postmaster>").is_ok() {
        return Ok(ForwardPathRef::Postmaster(None));
    }

    let path = path(line)?;

    if path.local.eq_ignore_ascii_case("postmaster") {
        match path.location {
            DomainRefOrAddr::Domain(domain) => Ok(ForwardPathRef::Postmaster(Some(domain))),
            DomainRefOrAddr::Addr(_) => Err(SyntaxError),
        }
    } else {
        Ok(ForwardPathRef::Mailbox(path))
    }
}

pub fn path<'a>(line: &mut &'a [u8]) -> Result<MailboxRef<'a>> {
    // Path = "<" [ A-d-l ":" ] Mailbox ">"
    line.atomic(|line| {
        line.expect(b"<")?;

        // A-d-l     = At-domain *( "," At-domain )
        // At-domain = "@" Domain
        if line.starts_with(b"@") {
            loop {
                line.expect(b"@")?;
                domain(line)?;

                if line.starts_with(b":") {
                    line.advance(1);
                    break;
                } else if line.starts_with(b",") {
                    line.advance(1);
                } else {
                    return Err(SyntaxError);
                }
            }
        }

        let mailbox = mailbox(line)?;
        line.expect(b">")?;

        Ok(mailbox)
    })
}

pub fn parameter<'a>(line: &mut &'a [u8]) -> Result<(&'a [u8], &'a [u8])> {
    // Mail-parameters = esmtp-param *(SP esmtp-param)
    // Rcpt-parameters = esmtp-param *(SP esmtp-param)
    // esmtp-param     = esmtp-keyword ["=" esmtp-value]
    // esmtp-keyword   = (ALPHA / DIGIT) *(ALPHA / DIGIT / "-")
    // esmtp-value     = 1*(%d33-60 / %d62-126)
    line.atomic(|line| {
        let keyword = line.take_while(|c, inx| c.is_ascii_alphanumeric() || c == b'-' && inx > 0);
        line.expect(b"=")?;
        let value = line.take_while(|c, _| matches!(c, 33..=60 | 62..=126));

        if keyword.is_empty() || value.is_empty() {
            Err(SyntaxError)
        } else {
            Ok((keyword, value))
        }
    })
}

// Keyword        = Ldh-str

// Argument       = Atom

pub enum DomainRefOrAddr<'a> {
    Domain(&'a str),
    Addr(IpAddr),
}

pub enum DomainOrAddr {
    Domain(String),
    Addr(IpAddr),
}

impl<'a> DomainRefOrAddr<'a> {
    pub fn to_owned(self) -> DomainOrAddr {
        match self {
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

pub fn domain_or_address<'a>(line: &mut &'a [u8]) -> Result<DomainRefOrAddr<'a>> {
    domain(line).map(DomainRefOrAddr::Domain)
        .or_else(|_| address_literal(line).map(DomainRefOrAddr::Addr))
}

pub fn domain<'a>(line: &mut &'a [u8]) -> Result<&'a str> {
    line.atomic(|line| {
        let mut offset = 0;

        // Domain = sub-domain *("." sub-domain)
        loop {
            // sub-domain = Let-dig [Ldh-str]
            // Let-dig    = ALPHA / DIGIT
            if !line[offset].is_ascii_alphanumeric() {
                return Err(SyntaxError);
            }
            offset += 1;

            // Ldh-str = *( ALPHA / DIGIT / "-" ) Let-dig
            while offset < line.len()
            && (line[offset].is_ascii_alphanumeric() || line[offset] == b'-') {
                offset += 1;
            }

            if line[offset - 1] == b'-' {
                return Err(SyntaxError);
            }

            if offset >= line.len()
            || line[offset] != b'.' {
                break;
            }
        }

        let domain = str::from_utf8(&line[..offset]).unwrap();
        line.advance(offset);

        Ok(domain)
    })
}

pub fn address_literal(line: &mut &[u8]) -> Result<IpAddr> {
    // address-literal = "[" ( IPv4-address-literal / IPv6-address-literal ) "]"
    line.atomic(|line| {
        line.expect(b"[")?;

        // IPv6-address-literal = "IPv6:" IPv6-addr
        let addr = if line.starts_with(b"IPv6:") {
            line.advance(5);
            address_ipv6(line)?.into()
        } else {
            address_ipv4(line)?.into()
        };

        line.expect(b"]")?;

        Ok(addr)
    })
}

pub fn address_ipv4(line: &mut &[u8]) -> Result<Ipv4Addr> {
    // IPv4-address-literal = Snum 3("."  Snum)
    // Snum                 = 1*3DIGIT
    line.atomic(|line| {
        let a = read_number(line, 10, 3)?;
        let b = read_number(line, 10, 3)?;
        let c = read_number(line, 10, 3)?;
        let d = read_number(line, 10, 3)?;
        Ok(Ipv4Addr::new(a, b, c, d))
    })
}

pub fn read_number<T>(line: &mut &[u8], radix: u32, max_digits: usize) -> Result<T>
where
    T: TryFrom<u32> + 'static,
{
    line.atomic(|line| {
        let mut value: u32 = 0;
        let mut count = 0;

        while !line.is_empty() && count < max_digits && char::from(line[0]).is_digit(radix) {
            value *= radix;
            value += char::from(line[0]).to_digit(radix).unwrap();
            count += 1;
            line.advance(1);
        }

        if count == 0 {
            Err(SyntaxError)
        } else {
            T::try_from(value).map_err(|_| SyntaxError)
        }
    })
}

pub fn address_ipv6(line: &mut &[u8]) -> Result<Ipv6Addr> {
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
    fn read_groups(line: &mut &[u8], groups: &mut [u16]) -> (usize, bool) {
        let limit = groups.len();

        for (i, slot) in groups.iter_mut().enumerate() {
            // Try to read a trailing embedded IPv4 address. There must be at
            // least two groups left.
            if i < limit - 1 {
                let mut cursor = *line;
                if let Ok(addr) = address_ipv4(&mut cursor) {
                    let [one, two, three, four] = addr.octets();
                    groups[i + 0] = u16::from_be_bytes([one, two]);
                    groups[i + 1] = u16::from_be_bytes([three, four]);
                    *line = cursor;
                    return (i + 2, true);
                }
            }

            let group = line.atomic(|line| {
                if i > 0 {
                    line.expect(b":")?;
                }

                read_number(line, 16, 4)
            }).ok();

            match group {
                Some(g) => *slot = g,
                None => return (i, false),
            }
        }

        (groups.len(), false)
    }

    line.atomic(|line| {
        // Read the front part of the address; either the whole thing, or up
        // to the first ::
        let mut head = [0; 8];
        let (head_size, head_ipv4) = read_groups(line, &mut head);

        if head_size == 8 {
            return Ok(head.into());
        }

        // IPv4 part is not allowed before `::`
        if head_ipv4 {
            return Err(SyntaxError);
        }

        // Read `::` if previous code parsed less than 8 groups.
        // `::` indicates one or more groups of 16 bits of zeros.
        line.expect(b"::")?;

        // Read the back part of the address. The :: must contain at least one
        // set of zeroes, so our max length is 7.
        let mut tail = [0; 7];
        let limit = 8 - (head_size + 1);
        let (tail_size, _) = read_groups(line, &mut tail[..limit]);

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
    pub fn to_owned(self) -> Mailbox {
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

pub fn mailbox<'a>(line: &mut &'a [u8]) -> Result<MailboxRef<'a>> {
    // Mailbox    = Local-part "@" ( Domain / address-literal )
    // Local-part = Dot-string / Quoted-string
    line.atomic(|line| {
        let local = quoted_string(line).or_else(|_| dot_string(line))?;
        line.expect(b"@")?;
        let location = domain_or_address(line)?;
        Ok(MailboxRef { local, location })
    })
}

pub fn dot_string<'a>(line: &mut &'a [u8]) -> Result<&'a str> {
    line.atomic(|line| {
        let mut cursor = *line;

        // Dot-string = Atom *("."  Atom)
        atom(&mut cursor)?;
        while cursor.starts_with(b".") {
            cursor.advance(1);
            atom(&mut cursor)?;
        }

        let len = line.len() - cursor.len();
        let string = str::from_utf8(&line[..len]).unwrap();

        *line = cursor;
        Ok(string)
    })
}

pub fn atom<'a>(line: &mut &'a [u8]) -> Result<&'a str> {
    // Atom = 1*atext
    line.atomic(|line| {
        let text = line.take_while(|b, _| match b {
            b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'/' | b'=' | b'?' |
            b'^' | b'_' | b'`' | b'{' | b'|' | b'}' | b'~' => true,
            _ => b.is_ascii_alphanumeric(),
        });

        if text.is_empty() {
            Err(SyntaxError)
        } else {
            Ok(str::from_utf8(text).unwrap())
        }
    })
}

pub fn quoted_string<'a>(line: &mut &'a [u8]) -> Result<&'a str> {
    line.atomic(|line| {
        // Quoted-string = DQUOTE *QcontentSMTP DQUOTE

        line.expect(b"\"")?;

        let mut offset = 0;

        // QcontentSMTP = qtextSMTP / quoted-pairSMTP
        while offset < line.len() && line[offset] != b'"' {
            match line[offset] {
                // qtextSMTP = %d32-33 / %d35-91 / %d93-126
                32..=33 | 35..=91 | 93..=126 => offset += 1,
                // quoted-pairSMTP = %d92 %d32-126
                92 if offset + 1 < line.len() => match line[offset + 1] {
                    32..=126 => offset += 2,
                    _ => return Err(SyntaxError),
                },
                _ => return Err(SyntaxError)
            }
        }

        let string = str::from_utf8(&line[..offset]).unwrap();
        line.advance(offset);

        line.expect(b"\"")?;

        Ok(string)
    })
}

pub fn string<'a>(line: &mut &'a [u8]) -> Result<&'a str> {
    // String = Atom / Quoted-string
    atom(line).or_else(|_| quoted_string(line))
}
