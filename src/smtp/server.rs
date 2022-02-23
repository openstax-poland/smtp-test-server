// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

//! SMTP server

use anyhow::{Context, Result};
use std::net::{Ipv4Addr, Ipv6Addr};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};

use crate::{state::StateRef, util, config};
use super::proto::{Connection, Response};

pub async fn start(config: config::Smtp, state: StateRef) -> Result<()> {
    // IPv4 TCP listener on port 587 (per RFC 6409)
    let listener_ipv4 = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port))
        .await
        .with_context(|| format!("could not bind TCP socket on localhost:{}", config.port))?;

    // IPv6 TCP listener on port 587 (per RFC 6409)
    let listener_ipv6 = TcpListener::bind((Ipv6Addr::LOCALHOST, config.port))
        .await
        .with_context(|| format!("could not bind TCP socket on localhost:{}", config.port))?;

    log::info!("Started SMTP server on {}", listener_ipv4.local_addr()?);
    log::info!("Started SMTP server on {}", listener_ipv6.local_addr()?);

    tokio::try_join!(
        handle_listener(state.clone(), listener_ipv4),
        handle_listener(state.clone(), listener_ipv6),
    )?;

    Ok(())
}

async fn handle_listener(state: StateRef, listener: TcpListener) -> Result<()> {
    loop {
        let (socket, addr) = listener.accept()
            .await
            .context("could not accept connection")?;

        let state = state.clone();

        tokio::spawn(async move {
            if let Err(err) = handle_client(state, socket).await {
                log::error!("error serving {addr}: {err:?}");
            }
        });
    }
}

/// Handle one SMTP connection
async fn handle_client(state: StateRef, mut socket: TcpStream) -> Result<()> {
    let mut smtp = Connection::new(state, socket.local_addr()?);

    {
        let response = smtp.connect();
        socket.write_all(response.data).await?;

        if response.close_connection {
            return Ok(());
        }
    }

    if let Err(err) = handle_commands(&mut smtp, &mut socket).await {
        let _ = socket.write_all(smtp.close().data).await;
        return Err(err);
    }

    Ok(())
}

async fn handle_commands(smtp: &mut Connection, socket: &mut TcpStream) -> Result<()> {
    loop {
        let response = match read_line(socket, smtp.buffer()).await? {
            None => smtp.line().await,
            Some(response) => Some(response),
        };

        if let Some(response) = response {
            log::trace!("<< {}", util::maybe_ascii(response.data));
            socket.write_all(response.data).await?;
            socket.flush().await?;

            if response.close_connection {
                break;
            }
        }
    }

    Ok(())
}

/// Read single line into a line buffer
///
/// Returns number of octets in current line, including terminating `\r\n`.
async fn read_line(socket: &mut TcpStream, line: &mut Vec<u8>)
-> Result<Option<Response<'static>>> {
    let mut offset = 0;

    loop {
        socket.read_buf(line).await?;

        while offset < line.len() {
            match line[offset..].iter().position(|&c| c == b'\r') {
                None => offset = line.len(),
                Some(o) => {
                    offset += o;

                    if line[offset..].starts_with(b"\r\n") {
                        return Ok(None);
                    }
                }
            }

            if line.ends_with(b"\r") {
                offset -= 1;
            }
        }

        if offset >= line.capacity() {
            log::trace!(">> {}", util::maybe_ascii(line));
            log::trace!("offset {offset} > limit {}, returning 500", line.capacity());
            return Ok(Some(Response::LINE_TOO_LONG));
        }
    }
}
