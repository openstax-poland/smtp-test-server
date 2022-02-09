//! SMTP server

use anyhow::{Context, Result};
use std::net::Ipv4Addr;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};

use super::proto::Connection;

pub async fn start() -> Result<()> {
    // IPv4 TCP listener on port 587 (per RFC 6409)
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 587))
        .await
        .context("could not bind TCP socket on localhost:587")?;

    loop {
        let (socket, addr) = listener.accept()
            .await
            .context("could not accept connection")?;

        tokio::spawn(async move {
            if let Err(err) = handle_client(socket).await {
                log::error!("error serving {addr}: {err:?}");
            }
        });
    }
}

/// Handle one SMTP connection
async fn handle_client(mut socket: TcpStream) -> Result<()> {
    let mut smtp = Connection::new(socket.local_addr()?);

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
    // RFC 5321 section 4.5.3.1.6 specifies 1000 octets as smallest allowed
    // upper limit on length of a single line.
    let mut line: Vec<u8> = Vec::with_capacity(1000);

    loop {
        let len = read_line(socket, &mut line).await?;

        if let Some(response) = smtp.line(&line[..len]) {
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
async fn read_line(socket: &mut TcpStream, line: &mut Vec<u8>) -> Result<usize> {
    line.clear();

    let mut offset = 0;

    loop {
        socket.read_buf(line).await?;

        while offset < line.len() {
            match line[offset..].iter().position(|&c| c == b'\r') {
                None => offset = line.len(),
                Some(o) => {
                    offset += o;

                    if line[offset..].starts_with(b"\r\n") {
                        return Ok(offset + 2);
                    }
                }
            }

            if line.ends_with(b"\r") {
                offset -= 1;
            }
        }
    }
}
