//! SMTP protocol state machine

use super::syntax::{self, DomainOrAddr, ForwardPath, ReversePath, SliceExt};

enum Command<'a> {
    Hello(Hello<'a>),
    Mail(Mail<'a>),
    Recipient(Recipient<'a>),
    Data,
    Reset,
    Verify(&'a [u8]),
    Expand(&'a [u8]),
    Help(Option<&'a [u8]>),
    Noop,
    Quit,
}

struct Hello<'a> {
    /// Is this an Extended HELO (EHLO)?
    extended: bool,
    client: DomainOrAddr<'a>,
}

struct Mail<'a> {
    from: ReversePath<'a>,
}

struct Recipient<'a> {
    to: ForwardPath<'a>,
}

impl<'a> Command<'a> {
    fn parse(mut line: &'a [u8]) -> syntax::Result<Self> {
        if line.ends_with(b"\r\n") {
            line = &line[..line.len() - 2];
        }

        let command = syntax::atom(&mut line)?;

        let command = if command.eq_ignore_ascii_case(b"HELO") {
            Command::parse_helo(&mut line)?
        } else if command.eq_ignore_ascii_case(b"EHLO") {
            Command::parse_ehlo(&mut line)?
        } else if command.eq_ignore_ascii_case(b"MAIL") {
            Command::parse_mail(&mut line)?
        } else if command.eq_ignore_ascii_case(b"RCPT") {
            Command::parse_rcpt(&mut line)?
        } else if command.eq_ignore_ascii_case(b"DATA") {
            Command::Data
        } else if command.eq_ignore_ascii_case(b"RSET") {
            Command::Reset
        } else if command.eq_ignore_ascii_case(b"VRFY") {
            Command::parse_vrfy(&mut line)?
        } else if command.eq_ignore_ascii_case(b"EXPN") {
            Command::parse_expn(&mut line)?
        } else if command.eq_ignore_ascii_case(b"HELP") {
            Command::parse_help(&mut line)?
        } else if command.eq_ignore_ascii_case(b"NOOP") {
            Command::parse_noop(&mut line)?
        } else if command.eq_ignore_ascii_case(b"QUIT") {
            Command::Quit
        } else {
            todo!()
        };

        line.expect_empty()?;
        Ok(command)
    }

    fn parse_helo(line: &mut &'a [u8]) -> syntax::Result<Self> {
        line.expect(b" ")?;
        Ok(Command::Hello(Hello {
            extended: false,
            client: DomainOrAddr::Domain(syntax::domain(line)?),
        }))
    }

    fn parse_ehlo(line: &mut &'a [u8]) -> syntax::Result<Self> {
        line.expect(b" ")?;
        Ok(Command::Hello(Hello {
            extended: true,
            client: syntax::domain_or_address(line)?,
        }))
    }

    fn parse_mail(line: &mut &'a [u8]) -> syntax::Result<Self> {
        line.expect_caseless(b" FROM:")?;
        let from = syntax::reverse_path(line)?;

        // TODO: extensions

        Ok(Command::Mail(Mail { from }))
    }

    fn parse_rcpt(line: &mut &'a [u8]) -> syntax::Result<Self> {
        line.expect_caseless(b" TO:")?;
        let to = syntax::forward_path(line)?;

        // TODO: extensions

        Ok(Command::Recipient(Recipient { to }))
    }

    fn parse_vrfy(line: &mut &'a [u8]) -> syntax::Result<Self> {
        line.expect(b" ")?;
        Ok(Command::Verify(syntax::string(line)?))
    }

    fn parse_expn(line: &mut &'a [u8]) -> syntax::Result<Self> {
        line.expect(b" ")?;
        Ok(Command::Expand(syntax::string(line)?))
    }

    fn parse_help(line: &mut &'a [u8]) -> syntax::Result<Self> {
        let topic = match line.expect(b" ") {
            Ok(_) => Some(syntax::string(line)?),
            Err(_) => None,
        };
        Ok(Command::Help(topic))
    }

    fn parse_noop(line: &mut &'a [u8]) -> syntax::Result<Self> {
        if line.expect(b" ").is_ok() {
            syntax::string(line)?;
        }
        Ok(Command::Noop)
    }
}
