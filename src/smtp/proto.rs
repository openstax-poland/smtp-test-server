//! SMTP protocol state machine

use std::{io::Write as _, fmt, net::SocketAddr};
use thiserror::Error;

use crate::syntax::*;
use super::syntax::{self, DomainRefOrAddr, ForwardPathRef, ReversePathRef, ReversePath, ForwardPath};

pub struct Connection {
    name: SocketAddr,
    state: State,
    reverse_path: Option<ReversePath>,
    forward_path: Vec<ForwardPath>,
    message: Vec<u8>,
    /// Response buffer
    response: Vec<u8>,
}

pub struct Response<'a> {
    /// Binary representation of this response which is to be sent to the client
    pub data: &'a [u8],
    /// Should connection be closed after sending this response?
    pub close_connection: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    /// Initial connection state, before client sent EHLO/HELO
    Handshake,
    /// Nothing is happening at the moment
    Relaxed,
    /// Client is sending list of recipients
    Recipients,
    /// Client is sending message body
    Data,
}

impl Connection {
    pub fn new(name: SocketAddr) -> Connection {
        Connection {
            name,
            state: State::Handshake,
            reverse_path: None,
            forward_path: vec![],
            message: vec![],
            response: vec![],
        }
    }

    pub fn connect(&mut self) -> Response {
        Response::new(&mut self.response, 220, format!("{} Service ready", self.name))
    }

    /// Handle single line
    pub fn line(&mut self, line: &[u8]) -> Option<Response> {
        if self.state == State::Data {
            return self.data_line(line);
        }

        if !line.iter().all(u8::is_ascii) {
            return Some(Response::INVALID_CHARACTERS);
        }

        let command = match Command::parse(line) {
            Ok(command) => command,
            Err(err) => return Some(Response::new(&mut self.response, 500, err)),
        };

        Some(match command {
            Command::Hello(hello) => self.handshake(hello),
            Command::Mail(mail) => self.mail(mail),
            Command::Recipient(recipient) => self.recipient(recipient),
            Command::Data => self.data(),
            Command::Reset => self.reset(),
            Command::Verify(_) | Command::Expand(_) => Response::NOT_IMPLEMENTED,
            Command::Help(topic) => self.help(topic),
            Command::Noop => Response::OK_250,
            Command::Quit => self.close(),
        })
    }

    pub fn close(&mut self) -> Response {
        Response::new(&mut self.response, 221,
            format!("{} Service closing transmission channel", self.name)).close()
    }

    fn handshake(&mut self, hello: Hello) -> Response {
        self.reset_buffers();

        let mut rsp = Response::new_multiline(&mut self.response, 250,
                format!("{} greets {}", self.name, hello.client));

        if hello.extended {
            // TODO: list extensions
        }

        rsp.finish()
    }

    fn mail(&mut self, mail: Mail) -> Response {
        self.reset_buffers();
        self.reverse_path = Some(mail.from.to_owned());
        self.state = State::Recipients;

        Response::new(&mut self.response, 000, "TODO")
    }

    fn recipient(&mut self, recipient: Recipient) -> Response {
        if self.state != State::Recipients {
            return Response::new(&mut self.response, 000, "TODO");
        }

        self.forward_path.push(recipient.to.to_owned());

        Response::new(&mut self.response, 000, "TODO")
    }

    fn data_line(&mut self, mut line: &[u8]) -> Option<Response> {
        if line == b".\r\n" {
            self.state = State::Relaxed;

            if !self.message.iter().all(u8::is_ascii) {
                return Some(Response::INVALID_CHARACTERS);
            }

            // TODO: process email
            return Some(Response::new(&mut self.response, 000, "TODO"));
        }

        if line.starts_with(b".") {
            line = &line[1..];
        }

        self.message.extend_from_slice(line);

        None
    }

    fn data(&mut self) -> Response {
        self.state = State::Data;
        todo!()
    }

    fn reset(&mut self) -> Response {
        self.reset_buffers();
        Response::OK_250
    }

    fn reset_buffers(&mut self) {
        self.reverse_path = None;
        self.forward_path.clear();
        self.state = State::Relaxed;
    }

    fn help(&mut self, topic: Option<&str>) -> Response {
        let topic = match topic {
            Some(topic) => topic,
            None => {
                let mut rsp = Response::new_multiline(
                    &mut self.response, 214, "Available commands:");
                rsp
                    .line("HELO")
                    .line("EHLO")
                    .line("MAIL")
                    .line("RCPT")
                    .line("DATA")
                    .line("RSET")
                    .line("HELP")
                    .line("NOOP")
                    .line("QUIT")
                ;
                return rsp.finish();
            }
        };

        Response::new(&mut self.response, 504, format!("No help found for topic {topic:?}"))
    }
}

impl<'a> Response<'a> {
    const OK_250: Response<'static> = Response {
        data: b"250 OK\r\n",
        close_connection: false,
    };

    const NOT_IMPLEMENTED: Response<'static> = Response {
        data: b"502 Command not implemented\r\n",
        close_connection: false,
    };

    const INVALID_CHARACTERS: Response<'static> = Response {
        data: b"500 Syntax error - invalid character\r\n",
        close_connection: false,
    };

    fn new(buffer: &'a mut Vec<u8>, code: u16, message: impl fmt::Display) -> Response<'a> {
        buffer.clear();
        let _ = write!(buffer, "{code:03} {message}\r\n");
        Response {
            data: buffer,
            close_connection: false,
        }
    }

    fn new_multiline(buffer: &'a mut Vec<u8>, code: u16, message: impl fmt::Display)
    -> ResponseBuilder<'a> {
        buffer.clear();
        let _ = write!(buffer, "{code:03} {message}\r\n");
        ResponseBuilder { code, offset: 3, buffer }
    }

    /// Set [`close_connection`] to `true`
    fn close(self) -> Response<'a> {
        Response { close_connection: true, ..self }
    }
}

struct ResponseBuilder<'a> {
    code: u16,
    offset: usize,
    buffer: &'a mut Vec<u8>,
}

impl<'a> ResponseBuilder<'a> {
    fn finish(self) -> Response<'a> {
        Response {
            data: self.buffer,
            close_connection: false,
        }
    }

    fn line(&mut self, line: impl fmt::Display) -> &mut Self {
        self.buffer[self.offset] = b'-';
        self.offset = self.buffer.len() + 3;
        let _ = write!(self.buffer, "{:03} {line}\r\n", self.code);
        self
    }
}

enum Command<'a> {
    Hello(Hello<'a>),
    Mail(Mail<'a>),
    Recipient(Recipient<'a>),
    Data,
    Reset,
    Verify(&'a str),
    Expand(&'a str),
    Help(Option<&'a str>),
    Noop,
    Quit,
}

struct Hello<'a> {
    /// Is this an Extended HELO (EHLO)?
    extended: bool,
    client: DomainRefOrAddr<'a>,
}

struct Mail<'a> {
    from: ReversePathRef<'a>,
}

struct Recipient<'a> {
    to: ForwardPathRef<'a>,
}

#[derive(Debug, Error)]
enum CommandParseError {
    #[error(transparent)]
    Syntax(SyntaxError),
    /// Unknown command
    #[error("Command not recognized")]
    Unknown,
}

impl From<SyntaxError> for CommandParseError {
    fn from(err: SyntaxError) -> Self {
        CommandParseError::Syntax(err)
    }
}

impl<'a> Command<'a> {
    fn parse(mut line: &'a [u8]) -> Result<Self, CommandParseError> {
        if line.ends_with(b"\r\n") {
            line = &line[..line.len() - 2];
        }

        let mut line = Buffer::new(line);
        let command = crate::syntax::atom(&mut line)?;

        let command = if command.eq_ignore_ascii_case("HELO") {
            Command::parse_helo(&mut line)?
        } else if command.eq_ignore_ascii_case("EHLO") {
            Command::parse_ehlo(&mut line)?
        } else if command.eq_ignore_ascii_case("MAIL") {
            Command::parse_mail(&mut line)?
        } else if command.eq_ignore_ascii_case("RCPT") {
            Command::parse_rcpt(&mut line)?
        } else if command.eq_ignore_ascii_case("DATA") {
            Command::Data
        } else if command.eq_ignore_ascii_case("RSET") {
            Command::Reset
        } else if command.eq_ignore_ascii_case("VRFY") {
            Command::parse_vrfy(&mut line)?
        } else if command.eq_ignore_ascii_case("EXPN") {
            Command::parse_expn(&mut line)?
        } else if command.eq_ignore_ascii_case("HELP") {
            Command::parse_help(&mut line)?
        } else if command.eq_ignore_ascii_case("NOOP") {
            Command::parse_noop(&mut line)?
        } else if command.eq_ignore_ascii_case("QUIT") {
            Command::Quit
        } else {
            return Err(CommandParseError::Unknown);
        };

        line.expect_empty()?;
        Ok(command)
    }

    fn parse_helo(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        line.expect(b" ")?;
        Ok(Command::Hello(Hello {
            extended: false,
            client: DomainRefOrAddr::Domain(syntax::domain(line)?),
        }))
    }

    fn parse_ehlo(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        line.expect(b" ")?;
        Ok(Command::Hello(Hello {
            extended: true,
            client: syntax::domain_or_address(line)?,
        }))
    }

    fn parse_mail(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        line.expect_caseless(b" FROM:")?;
        let from = syntax::reverse_path(line)?;

        // TODO: extensions

        Ok(Command::Mail(Mail { from }))
    }

    fn parse_rcpt(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        line.expect_caseless(b" TO:")?;
        let to = syntax::forward_path(line)?;

        // TODO: extensions

        Ok(Command::Recipient(Recipient { to }))
    }

    fn parse_vrfy(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        line.expect(b" ")?;
        Ok(Command::Verify(syntax::string(line)?))
    }

    fn parse_expn(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        line.expect(b" ")?;
        Ok(Command::Expand(syntax::string(line)?))
    }

    fn parse_help(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        let topic = match line.expect(b" ") {
            Ok(_) => Some(syntax::string(line)?),
            Err(_) => None,
        };
        Ok(Command::Help(topic))
    }

    fn parse_noop(line: &mut Buffer<'a>) -> Result<Self, CommandParseError> {
        if line.expect(b" ").is_ok() {
            syntax::string(line)?;
        }
        Ok(Command::Noop)
    }
}
