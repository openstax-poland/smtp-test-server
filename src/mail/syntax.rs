use std::{str, marker::PhantomData, borrow::Cow};
use time::{Weekday, Month, UtcOffset, Time, Date, OffsetDateTime, PrimitiveDateTime};

use crate::syntax::*;

/// List of parseable items separated by commas
pub struct ListOf<'a, T> {
    items: &'a [u8],
    separator: &'a [u8],
    _type: PhantomData<&'a [T]>,
}

impl<'a, T: Parse<'a>> ListOf<'a, T> {
    fn new(separator: &'a [u8], items: &'a [u8]) -> Self {
        ListOf { items, separator, _type: PhantomData }
    }

    fn iter<'c>(&'c self) -> impl Iterator<Item = T> + 'c
    where
        'c: 'a,
    {
        let mut items = self.items;
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

/// Folding white space
fn fws(buf: &mut &[u8]) -> Result<()> {
    // FWS = ([*WSP CRLF] 1*WSP) / obs-FWS

    let before = buf.take_while(|c, _| is_wsp(c));
    let after = buf.atomic(|buf| {
        buf.expect(b"\r\n")?;
        wsp(buf)?;
        Ok(())
    }).is_ok();

    if before.is_empty() && !after {
        Err(SyntaxError)
    } else {
        Ok(())
    }
}

fn comment(buf: &mut &[u8]) -> Result<()> {
    // comment = "(" *([FWS] ccontent) [FWS] ")"
    buf.atomic(|buf| {
        buf.expect(b"(")?;
        buf.maybe(fws);

        while !buf.is_empty() && !buf.starts_with(b")") {
            // ccontent = ctext / quoted-pair / comment
            match buf[0] {
                // ctext = %d33-39 / %d42-91 / %d93-126 / obs-ctext
                33..=39 | 42..=91 | 93..=126 => buf.advance(1),
                // quoted-pair = ("\" (VCHAR / WSP)) / obs-qp
                b'\\' if buf.len() >= 2 => match buf[1] {
                    c if is_vchar(c) || is_wsp(c) => buf.advance(2),
                    _ => return Err(SyntaxError),
                },
                _ => return Err(SyntaxError),
            }

            buf.maybe(fws);
        }

        buf.expect(b")")?;
        Ok(())
    })
}

/// Comment or folding white space
fn cfws(buf: &mut &[u8]) -> Result<()> {
    // CFWS = (1*([FWS] comment) [FWS]) / FWS
    buf.atomic(|buf| {
        let before = buf.len();
        buf.maybe(fws);

        while comment(buf).is_ok() {
            buf.maybe(fws);
        }

        if before > buf.len() {
            Ok(())
        } else {
            Err(SyntaxError)
        }
    })
}

fn atom<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    // atom = [CFWS] 1*atext [CFWS]
    buf.atomic(|buf| {
        buf.maybe(cfws);
        let atom = crate::syntax::atom(buf)?;
        buf.maybe(cfws);
        Ok(atom)
    })
}

fn dot_atom<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    // dot-atom = [CFWS] dot-atom-text [CFWS]
    buf.atomic(|buf| {
        buf.maybe(cfws);
        let atom = crate::syntax::dot_atom(buf)?;
        buf.maybe(cfws);
        Ok(atom)
    })
}

pub struct Quoted<'a>(&'a str);

impl<'a> Quoted<'a> {
    pub fn unquote(&self) -> Cow<'a, str> {
        let mut result = String::new();
        let mut rest = self.0;

        while let Some(inx) = rest.find(&['\r', '\\']) {
            if inx > 0 {
                result.push_str(&rest[..inx]);
            }

            if rest[inx..].starts_with("\r\n") {
                rest = &rest[2..];
            } else /* starts with \ */ {
                rest = &rest[1..];
            }
        }

        if result.is_empty() {
            Cow::from(self.0)
        } else {
            Cow::from(result)
        }
    }
}

fn quoted_string<'a>(buf: &mut &'a [u8]) -> Result<Quoted<'a>> {
    // quoted-string = [CFWS]
    //                 DQUOTE *([FWS] qcontent) [FWS] DQUOTE
    //                 [CFWS]
    buf.atomic(|buf| {
        buf.maybe(cfws);
        buf.expect(b"\"")?;

        let mut cursor = *buf;
        cursor.maybe(fws);

        // qcontent    = qtext / quoted-pair
        // qtext       = %d33 / %d35-91 / %d93-126 / obs-qtext
        // quoted-pair = ("\" (VCHAR / WSP)) / obs-qp
        while !cursor.is_empty() && !cursor.starts_with(b"\"") {
            match cursor[0] {
                33 | 35..=91 | 93..=126 => buf.advance(1),
                b'\\' if cursor.len() >= 2 => match buf[1] {
                    0x21..=0x7e | b' ' | b'\t' => buf.advance(2),
                    _ => return Err(SyntaxError),
                },
                _ => return Err(SyntaxError),
            }
        }

        let length = buf.len() - cursor.len();
        let content = str::from_utf8(buf.take(length)).unwrap();

        buf.expect(b"\"")?;
        buf.maybe(cfws);

        Ok(Quoted(content))
    })
}

fn word<'a>(buf: &mut &'a [u8]) -> Result<Quoted<'a>> {
    // word = atom / quoted-string
    atom(buf).map(Quoted).or_else(|_| quoted_string(buf))
}

pub struct Phrase<'a>(&'a str);

impl Phrase<'_> {
    pub fn unquote(&self) -> String {
        let mut result = String::new();
        let mut rest = self.0.as_bytes();

        while !rest.is_empty() {
            let word = word(&mut rest).expect("invalid pre-parsed string");
            result.push_str(&word.unquote());
        }

        result
    }
}

impl<'a> Parse<'a> for Phrase<'a> {
    fn parse(from: &mut &'a [u8]) -> Result<Self> {
        phrase(from)
    }
}

fn phrase<'a>(buf: &mut &'a [u8]) -> Result<Phrase<'a>> {
    // phrase = 1*word / obs-phrase

    let mut cursor = *buf;
    word(&mut cursor)?;

    loop {
        if word(&mut cursor).is_err() {
            break;
        }
    }

    let length = buf.len() - cursor.len();
    let value = str::from_utf8(buf.take(length)).unwrap();

    Ok(Phrase(value))
}

fn unstructured<'a>(buf: &mut &'a [u8]) -> Result<&'a [u8]> {
    // unstructured = (*([FWS] VCHAR) *WSP) / obs-unstruct

    let mut cursor = *buf;

    while !cursor.is_empty() {
        if !is_vchar(cursor[0]) {
            if fws(&mut cursor).is_err() {
                break;
            }
        }
    }

    let length = buf.len() - cursor.len();
    let value = buf.take(length);

    while wsp(buf).is_ok() {}

    Ok(value)
}

// ------------------------------------------------------ 3.3. Date and Time ---

pub enum AnyDateTime {
    Local(PrimitiveDateTime),
    Offset(OffsetDateTime)
}

pub fn date_time(buf: &mut &[u8]) -> Result<AnyDateTime> {
    // date-time = [ day-of-week "," ] date time [CFWS]
    buf.atomic(|buf| {
        let day_of_week = buf.maybe(|buf| {
            let day = day_of_week(buf)?;
            buf.expect(b",")?;
            Ok(day)
        });

        let date = date(buf)?;
        let time = time(buf)?;

        if let Some(day_of_week) = day_of_week {
            if day_of_week != date.weekday() {
                return Err(SyntaxError);
            }
        }

        buf.maybe(cfws);

        Ok(match time {
            AnyTime::Local(time) => AnyDateTime::Local(PrimitiveDateTime::new(date, time)),
            AnyTime::Offset(time) => AnyDateTime::Offset(PrimitiveDateTime::new(date, time.time)
                .assume_offset(time.offset)),
        })
    })
}

pub fn day_of_week(buf: &mut &[u8]) -> Result<Weekday> {
    // day-of-week = ([FWS] day-name) / obs-day-of-week
    buf.atomic(|buf| {
        buf.maybe(fws);
        day_name(buf)
    })
}

pub fn day_name(buf: &mut &[u8]) -> Result<Weekday> {
    // day-name = "Mon" / "Tue" / "Wed" / "Thu" / "Fri" / "Sat" / "Sun"
    if buf.expect_caseless(b"Mon").is_ok() {
        Ok(Weekday::Monday)
    } else if buf.expect_caseless(b"Tue").is_ok() {
        Ok(Weekday::Tuesday)
    } else if buf.expect_caseless(b"Wed").is_ok() {
        Ok(Weekday::Wednesday)
    } else if buf.expect_caseless(b"Thu").is_ok() {
        Ok(Weekday::Thursday)
    } else if buf.expect_caseless(b"Fri").is_ok() {
        Ok(Weekday::Friday)
    } else if buf.expect_caseless(b"Sat").is_ok() {
        Ok(Weekday::Saturday)
    } else if buf.expect_caseless(b"Sun").is_ok() {
        Ok(Weekday::Sunday)
    } else {
        Err(SyntaxError)
    }
}

pub fn date(buf: &mut &[u8]) -> Result<Date> {
    // date = day month year
    buf.atomic(|buf| {
        let day = day(buf)?;
        let month = month(buf)?;
        let year = year(buf)?;

        Date::from_calendar_date(year, month, day).map_err(|_| SyntaxError)
    })
}

pub fn day(buf: &mut &[u8]) -> Result<u8> {
    // day = ([FWS] 1*2DIGIT FWS) / obs-day
    buf.atomic(|buf| {
        fws(buf)?;
        let day = read_number(buf, 10, 1, 2)?;
        fws(buf)?;
        Ok(day)
    })
}

pub fn month(buf: &mut &[u8]) -> Result<Month> {
    // month = "Jan" / "Feb" / "Mar" / "Apr" /
    //         "May" / "Jun" / "Jul" / "Aug" /
    //         "Sep" / "Oct" / "Nov" / "Dec"
    if buf.expect_caseless(b"Jan").is_ok() {
        Ok(Month::January)
    } else if buf.expect_caseless(b"Feb").is_ok() {
        Ok(Month::February)
    } else if buf.expect_caseless(b"Mar").is_ok() {
        Ok(Month::March)
    } else if buf.expect_caseless(b"Apr").is_ok() {
        Ok(Month::April)
    } else if buf.expect_caseless(b"May").is_ok() {
        Ok(Month::May)
    } else if buf.expect_caseless(b"Jun").is_ok() {
        Ok(Month::June)
    } else if buf.expect_caseless(b"Jul").is_ok() {
        Ok(Month::July)
    } else if buf.expect_caseless(b"Aug").is_ok() {
        Ok(Month::August)
    } else if buf.expect_caseless(b"Sep").is_ok() {
        Ok(Month::September)
    } else if buf.expect_caseless(b"Oct").is_ok() {
        Ok(Month::October)
    } else if buf.expect_caseless(b"Nov").is_ok() {
        Ok(Month::November)
    } else if buf.expect_caseless(b"Dec").is_ok() {
        Ok(Month::December)
    } else {
        Err(SyntaxError)
    }
}

pub fn year(buf: &mut &[u8]) -> Result<i32> {
    // year = (FWS 4*DIGIT FWS) / obs-year
    buf.atomic(|buf| {
        fws(buf)?;

        let year = read_number(buf, 10, 4, 4)?;

        if year < 1900 {
            return Err(SyntaxError);
        }

        fws(buf)?;
        Ok(year)
    })
}

pub enum AnyTime {
    Local(Time),
    Offset(OffsetTime),
}

pub struct OffsetTime {
    pub time: Time,
    pub offset: UtcOffset,
}

pub fn time(buf: &mut &[u8]) -> Result<AnyTime> {
    // time = time-of-day zone
    buf.atomic(|buf| {
        let time = time_of_day(buf)?;
        let zone = zone(buf)?;

        Ok(match zone {
            Some(offset) => AnyTime::Offset(OffsetTime { time, offset }),
            None => AnyTime::Local(time),
        })
    })
}

pub fn time_of_day(buf: &mut &[u8]) -> Result<Time> {
    // time-of-day = hour ":" minute [ ":" second ]
    buf.atomic(|buf| {
        let hour = hour(buf)?;
        buf.expect(b":")?;
        let minute = minute(buf)?;
        let second = buf.maybe(|buf| {
            buf.expect(b":")?;
            second(buf)
        }).unwrap_or(0);

        Time::from_hms(hour, minute, second).map_err(|_| SyntaxError)
    })
}

pub fn hour(buf: &mut &[u8]) -> Result<u8> {
    // hour = 2DIGIT / obs-hour
    read_number(buf, 10, 2, 2)
}

pub fn minute(buf: &mut &[u8]) -> Result<u8> {
    // minute = 2DIGIT / obs-minute
    read_number(buf, 10, 2, 2)
}

pub fn second(buf: &mut &[u8]) -> Result<u8> {
    // second = 2DIGIT / obs-second
    read_number(buf, 10, 2, 2)
}

pub fn zone(buf: &mut &[u8]) -> Result<Option<UtcOffset>> {
    // zone = (FWS ( "+" / "-" ) 4DIGIT) / obs-zone
    buf.atomic(|buf| {
        fws(buf)?;

        if buf.is_empty() {
            return Err(SyntaxError);
        }

        let positive = match buf[0] {
            b'+' => true,
            b'-' => false,
            _ => return Err(SyntaxError),
        };
        buf.advance(1);

        let hours: i32 = read_number(buf, 10, 2, 2)?;
        let minutes: i32 = read_number(buf, 10, 4, 4)?;

        if !positive && hours == 0 && minutes == 0 {
            Ok(None)
        } else {
            let seconds = (hours * 60 + minutes) * 60;
            let seconds = if positive { seconds } else { -seconds };
            UtcOffset::from_whole_seconds(seconds).map(Some).map_err(|_| SyntaxError)
        }
    })
}

// ------------------------------------------------------------ 3.4. Address ---

pub enum AddressOrGroupRef<'a> {
    Mailbox(MailboxRef<'a>),
    Group(GroupRef<'a>),
}

impl<'a> Parse<'a> for AddressOrGroupRef<'a> {
    fn parse(from: &mut &'a [u8]) -> Result<Self> {
        address(from)
    }
}

pub fn address<'a>(buf: &mut &'a [u8]) -> Result<AddressOrGroupRef<'a>> {
    // address = mailbox / group
    mailbox(buf).map(AddressOrGroupRef::Mailbox)
        .or_else(|_| group(buf).map(AddressOrGroupRef::Group))
}

pub struct MailboxRef<'a> {
    pub name: Option<Phrase<'a>>,
    pub address: AddressRef<'a>,
}

impl<'a> Parse<'a> for MailboxRef<'a> {
    fn parse(from: &mut &'a [u8]) -> Result<Self> {
        mailbox(from)
    }
}

pub fn mailbox<'a>(buf: &mut &'a [u8]) -> Result<MailboxRef<'a>> {
    // mailbox = name-addr / addr-spec
    name_addr(buf).or_else(|_| {
        let address = addr_spec(buf)?;
        Ok(MailboxRef { name: None, address })
    })
}

pub fn name_addr<'a>(buf: &mut &'a [u8]) -> Result<MailboxRef<'a>> {
    // name-addr = [display-name] angle-addr
    buf.atomic(|buf| {
        let name = buf.maybe(display_name);
        let address = angle_addr(buf)?;
        Ok(MailboxRef { name, address })
    })
}

pub fn angle_addr<'a>(buf: &mut &'a [u8]) -> Result<AddressRef<'a>> {
    // angle-addr = [CFWS] "<" addr-spec ">" [CFWS] / obs-angle-addr
    buf.atomic(|buf| {
        buf.maybe(cfws);
        buf.expect(b"<")?;
        let address = addr_spec(buf)?;
        buf.expect(b">")?;
        buf.maybe(cfws);
        Ok(address)
    })
}

pub struct GroupRef<'a> {
    pub name: Phrase<'a>,
    pub members: MailboxList<'a>,
}

pub fn group<'a>(buf: &mut &'a [u8]) -> Result<GroupRef<'a>> {
    // group = display-name ":" [group-list] ";" [CFWS]
    buf.atomic(|buf| {
        let name = display_name(buf)?;
        buf.expect(b":")?;
        let members = buf.maybe(group_list).unwrap_or(MailboxList::new(b"", b""));
        buf.expect(b";")?;
        buf.maybe(cfws);
        Ok(GroupRef { name, members })
    })
}

pub fn display_name<'a>(buf: &mut &'a [u8]) -> Result<Phrase<'a>> {
    // display-name = phrase
    phrase(buf)
}

pub type MailboxList<'a> = ListOf<'a, MailboxRef<'a>>;

pub fn mailbox_list<'a>(buf: &mut &'a [u8]) -> Result<MailboxList<'a>> {
    // mailbox-list = (mailbox *("," mailbox)) / obs-mbox-list

    let mut cursor = *buf;
    mailbox(&mut cursor)?;

    while cursor.expect(b",").is_ok() {
        mailbox(&mut cursor)?;
    }

    let length = buf.len() - cursor.len();
    let value = buf.take(length);

    Ok(ListOf::new(b",", value))
}

pub type AddressOrGroupList<'a> = ListOf<'a, AddressOrGroupRef<'a>>;

pub fn address_list<'a>(buf: &mut &'a [u8]) -> Result<AddressOrGroupList<'a>> {
    // address-list = (address *("," address)) / obs-addr-list

    let mut cursor = *buf;
    address(&mut cursor)?;

    while cursor.expect(b",").is_ok() {
        address(&mut cursor)?;
    }

    let length = buf.len() - cursor.len();
    let value = buf.take(length);

    Ok(ListOf::new(b",", value))
}

pub fn group_list<'a>(buf: &mut &'a [u8]) -> Result<MailboxList<'a>> {
    // group-list = mailbox-list / CFWS / obs-group-list
    match mailbox_list(buf) {
        Ok(value) => Ok(value),
        Err(_) => {
            cfws(buf)?;
            Ok(MailboxList::new(b"", b""))
        }
    }
}

// -------------------------------------------------------- 3.4.1. Addr-Spec ---

pub struct AddressRef<'a> {
    pub local: Quoted<'a>,
    pub domain: &'a str,
}

pub fn addr_spec<'a>(buf: &mut &'a [u8]) -> Result<AddressRef<'a>> {
    // addr-spec = local-part "@" domain
    buf.atomic(|buf| {
        let local = local_part(buf)?;
        buf.expect(b"@")?;
        let domain = domain(buf)?;
        Ok(AddressRef { local, domain })
    })
}

pub fn local_part<'a>(buf: &mut &'a [u8]) -> Result<Quoted<'a>> {
    // local-part = dot-atom / quoted-string / obs-local-part
    dot_atom(buf).map(Quoted).or_else(|_| quoted_string(buf))
}

pub fn domain<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    // domain = dot-atom / domain-literal / obs-domain
    dot_atom(buf).or_else(|_| domain_literal(buf))
}

#[inline]
pub fn is_dtext(c: u8) -> bool {
    // dtext = %d33-90 / %d94-126 / obs-dtext
    matches!(c, 33..=90 | 94..=126)
}

pub fn domain_literal<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    // domain-literal = [CFWS] "[" *([FWS] dtext) [FWS] "]" [CFWS]
    buf.atomic(|buf| {
        buf.maybe(cfws);
        buf.expect(b"[")?;

        let mut cursor = *buf;
        cursor.maybe(fws);

        while !cursor.is_empty() && !cursor.starts_with(b"]") {
            match cursor[0] {
                c if is_dtext(c) => cursor.advance(1),
                _ => return Err(SyntaxError),
            }
        }

        let length = buf.len() - cursor.len();
        let value = str::from_utf8(buf.take(length)).unwrap();

        buf.expect(b"]")?;
        buf.maybe(cfws);

        Ok(value)
    })
}

// -------------------------------------------------- 3.6. Field Definitions ---

pub enum Header<'a> {
    OriginationDate(AnyDateTime),
    /// Author(s) of the message
    ///
    /// This field represents the mailboxes of the persons or systems
    /// responsible for the writing of the message.
    From(MailboxList<'a>),
    /// Mailbox of the agent responsible for the actual transmission of
    /// the message
    ///
    /// If the originator of the message can be indicated by a single mailbox
    /// and the author and transmitter are identical, the `Sender` field should
    /// not be used. Otherwise, both fields should appear.
    Sender(MailboxRef<'a>),
    /// Indicates the addresses to which the author of the message suggests that
    /// replies be sent
    ReplyTo(AddressOrGroupList<'a>),
    /// Addresses of the primary recipients of the message
    To(AddressOrGroupList<'a>),
    /// Addresses of others who are to receive the message, though the content
    /// of the message may not be directed at them
    CarbonCopy(AddressOrGroupList<'a>),
    /// Addresses of recipients of the message whose addresses are not to
    /// be revealed to other recipients of the message
    BlindCarbonCopy(AddressOrGroupList<'a>),
    /// Unique message identifier that refers to a particular version of
    /// a particular message
    ///
    /// The uniqueness of the message identifier is guaranteed by the host that
    /// generates it. This message identifier is intended to be machine readable
    /// and not necessarily meaningful to humans. A message identifier pertains
    /// to exactly one version of a particular message; subsequent revisions to
    /// the message each receive new message identifiers.
    MessageId(MessageIdRef<'a>),
    /// IDs of messages to which this one is a reply
    InReplyTo(MessageIdList<'a>),
    /// Contents of the parent's `References` field (if any) followed by
    /// the contents of the parent's `MessageId` field (if any)
    References(MessageIdList<'a>),
    /// Short string identifying the topic of the message
    ///
    /// When used in a reply, the field body MAY start with the string “Re: ” (
    /// an abbreviation of the Latin “in re”, meaning “in the matter of”)
    /// followed by the contents of the `Subject` field body of the original
    /// message. If this is done, only one instance of the literal string “Re: ”
    /// ought to be used since use of other strings or more than one instance
    /// can lead to undesirable consequences.
    Subject(&'a [u8]),
    /// Additional comments on the text of the body of the message
    Comments(&'a [u8]),
    /// Comma-separated list of important words and phrases that might be useful
    /// for the recipient
    Keywords(KeywordList<'a>),
    /// date and time at which the resent message is dispatched by the resender
    /// of the message
    ResentDate(AnyDateTime),
    ResentFrom(MailboxList<'a>),
    ResentSender(MailboxRef<'a>),
    ResentTo(AddressOrGroupList<'a>),
    ResentCarbonCopy(AddressOrGroupList<'a>),
    ResentBlindCarbonCopy(AddressOrGroupList<'a>),
    ResentMessageId(MessageIdRef<'a>),
    ReturnPath(PathRef<'a>),
    Received(Received<'a>),
    Optional {
        name: &'a str,
        body: &'a [u8],
    },
}

pub fn filed<'a>(buf: &mut &'a [u8]) -> Result<Header<'a>> {
    buf.atomic(|buf| {
        let name = field_name(buf)?;
        buf.expect(b":")?;

        let header = if name.eq_ignore_ascii_case("Date") {
            Header::OriginationDate(date_time(buf)?)
        } else if name.eq_ignore_ascii_case("From") {
            Header::From(mailbox_list(buf)?)
        } else if name.eq_ignore_ascii_case("Sender") {
            Header::Sender(mailbox(buf)?)
        } else if name.eq_ignore_ascii_case("Reply-To:") {
            Header::ReplyTo(address_list(buf)?)
        } else if name.eq_ignore_ascii_case("To") {
            Header::To(address_list(buf)?)
        } else if name.eq_ignore_ascii_case("Cc") {
            Header::CarbonCopy(address_list(buf)?)
        } else if name.eq_ignore_ascii_case("Bcc") {
            Header::BlindCarbonCopy(bcc(buf))
        } else if name.eq_ignore_ascii_case("Message-Id") {
            Header::MessageId(msg_id(buf)?)
        } else if name.eq_ignore_ascii_case("In-Reply-To") {
            Header::InReplyTo(msg_id_list(buf)?)
        } else if name.eq_ignore_ascii_case("References") {
            Header::References(msg_id_list(buf)?)
        } else if name.eq_ignore_ascii_case("Subject") {
            Header::Subject(unstructured(buf)?)
        } else if name.eq_ignore_ascii_case("Comments") {
            Header::Comments(unstructured(buf)?)
        } else if name.eq_ignore_ascii_case("Keywords") {
            Header::Keywords(keywords(buf)?)
        } else if name.eq_ignore_ascii_case("Resent-Date") {
            Header::ResentDate(date_time(buf)?)
        } else if name.eq_ignore_ascii_case("Resent-From") {
            Header::ResentFrom(mailbox_list(buf)?)
        } else if name.eq_ignore_ascii_case("Resent-Sender") {
            Header::ResentSender(mailbox(buf)?)
        } else if name.eq_ignore_ascii_case("Resent-To") {
            Header::ResentTo(address_list(buf)?)
        } else if name.eq_ignore_ascii_case("Resent-Cc") {
            Header::ResentCarbonCopy(address_list(buf)?)
        } else if name.eq_ignore_ascii_case("Resent-Bcc") {
            Header::ResentBlindCarbonCopy(bcc(buf))
        } else if name.eq_ignore_ascii_case("Return-Path") {
            Header::ReturnPath(path(buf)?)
        } else if name.eq_ignore_ascii_case("Received") {
            Header::Received(received_value(buf)?)
        } else {
            Header::Optional { name, body: unstructured(buf)? }
        };

        buf.expect(b"\r\n")?;

        Ok(header)
    })
}

fn field_name<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    let name = buf.take_while(|_, b| matches!(b, 33..=57 | 59..=126));
    if name.is_empty() {
        Err(SyntaxError)
    } else {
        Ok(str::from_utf8(name).unwrap())
    }
}

fn bcc<'a>(buf: &mut &'a [u8]) -> AddressOrGroupList<'a> {
    // bcc = [address-list / CFWS]
    buf.maybe(|buf| {
        address_list(buf).or_else(|_| {
            cfws(buf)?;
            Ok(AddressOrGroupList::new(b"", b""))
        })
    }).unwrap_or(AddressOrGroupList::new(b"", b""))
}

pub struct MessageIdRef<'a>(&'a str);

impl<'a> Parse<'a> for MessageIdRef<'a> {
    fn parse(from: &mut &'a [u8]) -> Result<Self> {
        msg_id(from)
    }
}

fn msg_id<'a>(buf: &mut &'a [u8]) -> Result<MessageIdRef<'a>> {
    // msg-id = [CFWS] "<" id-left "@" id-right ">" [CFWS]
    buf.atomic(|buf| {
        buf.maybe(cfws);
        buf.expect(b"<")?;
        let value = buf.take_matching(|buf| {
            // id-left = dot-atom-text / obs-id-left
            crate::syntax::dot_atom(buf)?;
            buf.expect(b"@")?;
            // id-right = dot-atom-text / no-fold-literal / obs-id-right
            crate::syntax::dot_atom(buf).or_else(|_| no_fold_literal(buf))?;
            Ok(())
        })?;
        buf.expect(b">")?;
        buf.maybe(cfws);
        Ok(MessageIdRef(str::from_utf8(value).unwrap()))
    })
}

fn no_fold_literal<'a>(buf: &mut &'a [u8]) -> Result<&'a str> {
    // no-fold-literal = "[" *dtext "]"
    buf.atomic(|buf| {
        buf.expect(b"[")?;
        let value = buf.take_while(|c, _| is_dtext(c));
        buf.expect(b"]")?;
        Ok(str::from_utf8(value).unwrap())
    })
}

pub type MessageIdList<'a> = ListOf<'a, MessageIdRef<'a>>;

fn msg_id_list<'a>(buf: &mut &'a [u8]) -> Result<MessageIdList<'a>> {
    buf.take_matching(|buf| {
        msg_id(buf)?;
        while msg_id(buf).is_ok() {}
        Ok(())
    }).map(|list| MessageIdList::new(b"", list))
}

pub type KeywordList<'a> = ListOf<'a, Phrase<'a>>;

fn keywords<'a>(buf: &mut &'a [u8]) -> Result<KeywordList<'a>> {
    // keywords = phrase *("," phrase)
    let value = buf.take_matching(|buf| {
        phrase(buf)?;

        while buf.expect(b",").is_ok() {
            phrase(buf)?;
        }

        Ok(())
    })?;
    Ok(KeywordList::new(b",", value))
}

// ----------------------------------------------------- 3.6.7. Trace Fields ---

pub enum PathRef<'a> {
    Null,
    Address(AddressRef<'a>),
}

pub fn return_path<'a>(buf: &mut &'a [u8]) -> Result<PathRef<'a>> {
    // return = "Return-Path:" path CRLF
    buf.atomic(|buf| {
        buf.expect(b"Return-Path:")?;
        let path = path(buf)?;
        buf.expect(b"\r\n")?;
        Ok(path)
    })
}

fn path<'a>(buf: &mut &'a [u8]) -> Result<PathRef<'a>> {
    angle_addr(buf).map(PathRef::Address).or_else(|_| buf.atomic(|buf| {
        buf.maybe(cfws);
        buf.expect(b"<")?;
        buf.maybe(cfws);
        buf.expect(b">")?;
        buf.maybe(cfws);
        Ok(PathRef::Null)
    }))
}

pub struct Received<'a> {
    pub tokens: ListOf<'a, ReceivedToken<'a>>,
    pub date: AnyDateTime,
}

pub fn received<'a>(buf: &mut &'a [u8]) -> Result<Received<'a>> {
    // received = "Received:" *received-token ";" date-time CRLF
    buf.atomic(|buf| {
        buf.expect(b"Received:")?;
        let value = received_value(buf)?;
        buf.expect(b"\r\n")?;
        Ok(value)
    })
}

fn received_value<'a>(buf: &mut &'a [u8]) -> Result<Received<'a>> {
    // received       = *received-token ";" date-time
    // received-token = word / angle-addr / addr-spec / domain
    buf.atomic(|buf| {
        let mut cursor = *buf;
        while !cursor.is_empty() && !cursor.starts_with(b";") {
            received_token(&mut cursor)?;
        }
        let len = buf.len() - cursor.len();
        let tokens = ListOf::new(b"", buf.take(len));
        buf.expect(b";")?;
        let date = date_time(buf)?;
        Ok(Received { tokens, date })
    })
}

pub enum ReceivedToken<'a> {
    Word(Quoted<'a>),
    Address(AddressRef<'a>),
    Domain(&'a str),
}

impl<'a> Parse<'a> for ReceivedToken<'a> {
    fn parse(from: &mut &'a [u8]) -> Result<Self> {
        received_token(from)
    }
}

fn received_token<'a>(buf: &mut &'a [u8]) -> Result<ReceivedToken<'a>> {
    word(buf).map(ReceivedToken::Word)
        .or_else(|_| angle_addr(buf).map(ReceivedToken::Address))
        .or_else(|_| addr_spec(buf).map(ReceivedToken::Address))
        .or_else(|_| domain(buf).map(ReceivedToken::Domain))
}
