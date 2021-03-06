// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

//! SMTP protocol state machine

use std::{io::Write as _, fmt, net::SocketAddr, mem};
use thiserror::Error;

use crate::{syntax::*, state::StateRef, util};
use super::syntax::{self, DomainRefOrAddr, ForwardPathRef, ReversePathRef, ReversePath, ForwardPath};

pub struct Connection {
    global: StateRef,
    name: SocketAddr,
    remote: SocketAddr,
    state: State,
    reverse_path: Option<ReversePath>,
    forward_path: Vec<ForwardPath>,
    /// Line buffer
    line: Vec<u8>,
    /// Message buffer
    message: Vec<u8>,
    /// Length of message
    message_length: usize,
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
    pub fn new(config: &crate::config::Smtp, global: StateRef, name: SocketAddr, remote: SocketAddr)
    -> Connection {
        Connection {
            global,
            name,
            remote,
            state: State::Handshake,
            reverse_path: None,
            forward_path: vec![],
            // RFC 5321 section 4.5.3.1.6 specifies 1000 octets as smallest
            // allowed upper limit on length of a single line.
            line: Vec::with_capacity(1000),
            message: Vec::with_capacity(config.message_size),
            message_length: 0,
            response: vec![],
        }
    }

    pub fn connect(&mut self) -> Response {
        Response::new(&mut self.response, 220, format!("{} Service ready", self.name))
    }

    pub fn buffer(&mut self) -> &mut Vec<u8> {
        match self.state {
            State::Data => &mut self.message,
            _ => &mut self.line,
        }
    }

    /// Handle single line
    pub async fn line(&mut self, overflow: bool) -> Option<Response<'_>> {
        if overflow {
            return Some(match self.state {
                State::Data => {
                    self.state = State::Relaxed;
                    Response::TOO_MUCH_MAIL_DATA
                }
                _ => Response::LINE_TOO_LONG,
            });
        }

        if self.state == State::Data {
            return self.data_line().await;
        }

        log::trace!(">> {}", util::maybe_ascii(&self.line));

        if !self.line.iter().all(u8::is_ascii) {
            return Some(Response::INVALID_CHARACTERS);
        }

        let new_line = Vec::with_capacity(self.line.capacity());
        let line = mem::replace(&mut self.line, new_line);

        let command = match Command::parse(&line) {
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

    // ---------------------------------------------------- command handlers ---

    fn handshake(&mut self, hello: Hello) -> Response {
        log::info!("client {:?} ({}) connected", hello.client, self.remote);
        self.reset_buffers();

        let mut rsp = Response::new_multiline(&mut self.response, 250,
                format!("{} greets {}", self.name, hello.client));

        if hello.extended {
            rsp.line(format!("SIZE {}", self.message.capacity()));
        }

        rsp.finish()
    }

    fn mail(&mut self, mail: Mail) -> Response {
        if let Some(size) = mail.size {
            if size >= self.message.capacity() {
                return Response::MESSAGE_EXCEEDS_MAXIMUM_SIZE;
            }
        }

        self.reset_buffers();
        self.reverse_path = Some(mail.from.to_owned());
        self.state = State::Recipients;

        Response::OK_250
    }

    fn recipient(&mut self, recipient: Recipient) -> Response {
        if self.state != State::Recipients {
            return Response::BAD_SEQUENCE_OF_COMMANDS;
        }

        self.forward_path.push(recipient.to.to_owned());

        Response::OK_250
    }

    async fn data_line(&mut self) -> Option<Response<'_>> {
        log::trace!(">> {}", util::maybe_ascii(&self.message[self.message_length..]));

        if self.message.ends_with(b"\r\n.\r\n") {
            self.state = State::Relaxed;
            self.message_length = self.message.len() - 3;
            return Some(self.submit_message().await);
        }

        if self.message[self.message_length..].starts_with(b".") {
            self.message.remove(self.message_length);
        }

        self.message_length = self.message.len();

        None
    }

    fn data(&mut self) -> Response {
        if self.state != State::Recipients || self.forward_path.is_empty() {
            return Response::BAD_SEQUENCE_OF_COMMANDS;
        }

        self.state = State::Data;
        Response::START_MAIL_INPUT
    }

    fn reset(&mut self) -> Response {
        self.reset_buffers();
        Response::OK_250
    }

    fn reset_buffers(&mut self) {
        self.reverse_path = None;
        self.forward_path.clear();
        self.state = State::Relaxed;
        self.message.clear();
        self.message_length = 0;
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

    // -------------------------------------------------- message processing ---

    async fn submit_message(&mut self) -> Response<'_> {
        if !self.message.iter().all(u8::is_ascii) {
            let at = self.message.iter().position(|c| !c.is_ascii()).unwrap();
            log::trace!("not everything is ASCII: {} at {at}", self.message[at]);
            return Response::INVALID_CHARACTERS;
        }

        match self.global.submit_message(&self.message[..self.message_length]).await {
            Ok(()) => Response::OK_250,
            Err(err) => Response::new(&mut self.response, err.code(), err),
        }
    }
}

impl<'a> Response<'a> {
    const OK_250: Response<'static> = Response {
        data: b"250 OK\r\n",
        close_connection: false,
    };

    const START_MAIL_INPUT: Response<'static> = Response {
        data: b"354 Start mail input; end with <CRLF>.<CRLF>\r\n",
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

    const LINE_TOO_LONG: Response<'static> = Response {
        data: b"500 Line too long\r\n",
        close_connection: false,
    };

    const BAD_SEQUENCE_OF_COMMANDS: Response<'static> = Response {
        data: b"503 Bad sequence of commands\r\n",
        close_connection: false,
    };

    const TOO_MUCH_MAIL_DATA: Response<'static> = Response {
        data: b"552 Too much mail data\r\n",
        close_connection: false,
    };

    const MESSAGE_EXCEEDS_MAXIMUM_SIZE: Response<'static> = Response {
        data: b"552 Message size exceeds fixed maximium message size",
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
    size: Option<usize>,
}

struct Recipient<'a> {
    to: ForwardPathRef<'a>,
}

#[derive(Debug, Error)]
enum CommandParseError {
    #[error("Syntax error - {0}")]
    Syntax(#[from] Located<SyntaxError>),
    /// Unknown command
    #[error("Command not recognized")]
    Unknown,
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

        let mut size = None;

        while line.expect(b" ").is_ok() {
            let location = line.location();
            let (extension, value) = syntax::parameter(line)?;

            match_ignore_ascii_case! { extension;
                "SIZE" => {
                    if size.is_some() {
                        return Err(Located::new(
                            location, format!("duplicate extension {extension}")).into())
                    }

                    match value.parse::<usize>() {
                        Ok(value) => size = Some(value),
                        Err(err) => return Err(Located::new(
                            location, err.to_string()).into()),
                    }
                }
                _ => return Err(Located::new(
                    location, format!("unknown extension {extension}")).into()),
            }
        }

        Ok(Command::Mail(Mail { from, size }))
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
