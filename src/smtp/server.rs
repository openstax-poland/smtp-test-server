// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

//! SMTP server

use anyhow::{Context, Result};
use std::net::{Ipv6Addr, SocketAddr};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};

use crate::{state::StateRef, util, config};
use super::proto::Connection;

pub async fn start(config: config::Smtp, state: StateRef) -> Result<()> {
    // IPv6 TCP listener on port 587 (per RFC 6409)
    let listener = TcpListener::bind((Ipv6Addr::UNSPECIFIED, config.port))
        .await
        .with_context(|| format!("could not bind TCP socket on [{}]:{}", Ipv6Addr::UNSPECIFIED, config.port))?;

    log::info!("Started SMTP server on {}", listener.local_addr()?);

    loop {
        let (socket, addr) = listener.accept()
            .await
            .context("could not accept connection")?;

        let state = state.clone();

        tokio::spawn(async move {
            if let Err(err) = handle_client(state, socket, addr).await {
                log::error!("error serving {addr}: {err:?}");
            }
        });
    }
}

/// Handle one SMTP connection
async fn handle_client(state: StateRef, mut socket: TcpStream, addr: SocketAddr) -> Result<()> {
    let mut smtp = Connection::new(state, socket.local_addr()?, addr);

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
            false => smtp.line().await,
            true => Some(smtp.overflow_response()),
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
/// Returns boolean indicating whether a buffer overflow has occurred.
async fn read_line(socket: &mut TcpStream, line: &mut Vec<u8>)
-> Result<bool> {
    let mut offset = 0;

    loop {
        socket.read_buf(line).await?;

        while offset < line.len() {
            match line[offset..].iter().position(|&c| c == b'\r') {
                None => offset = line.len(),
                Some(o) => {
                    offset += o;

                    if line[offset..].starts_with(b"\r\n") {
                        return Ok(false);
                    }
                }
            }

            if line.ends_with(b"\r") {
                offset -= 1;
            }
        }

        if offset >= line.capacity() {
            log::trace!(">> {}", util::maybe_ascii(line));
            return Ok(true);
        }
    }
}
