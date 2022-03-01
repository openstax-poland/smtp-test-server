// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use anyhow::Result;
use argh::FromArgs;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub smtp: Smtp,
    pub http: Http,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Smtp {
    pub port: u16,
    pub message_size: usize,
}

impl Default for Smtp {
    fn default() -> Self {
        Smtp {
            // RFC 6409 specifies 587 as the SMTP TCP port
            port: 587,
            // RFC 5321 section 4.5.3.1.7 specified 64k octets as smallest
            // allowed upper limit on message length.
            message_size: 64 * 1024,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Http {
    pub port: u16,
}

impl Default for Http {
    fn default() -> Self {
        Http { port: 80 }
    }
}

/// SMTP test server
#[derive(FromArgs)]
struct Args {
    /// configuration file to use
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,
    /// port to run HTTP server on
    #[argh(option)]
    http_port: Option<u16>,
    /// port to run SMTP server on
    #[argh(option)]
    smtp_port: Option<u16>,
}

pub fn load() -> Result<Config> {
    let args: Args = argh::from_env();

    let mut config = match args.config {
        None => Config::default(),
        Some(path) => {
            let data = fs::read_to_string(path)?;
            toml::from_str(&data)?
        }
    };

    if let Some(port) = args.http_port {
        config.http.port = port;
    }

    if let Some(port) = args.smtp_port {
        config.smtp.port = port;
    }

    Ok(config)
}
