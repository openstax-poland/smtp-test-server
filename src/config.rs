use anyhow::Result;
use argh::FromArgs;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Default, Deserialize)]
pub struct Config {
    pub smtp: Smtp,
    pub http: Http,
}

#[derive(Deserialize)]
pub struct Smtp {
    pub port: u16,
}

impl Default for Smtp {
    fn default() -> Self {
        // RFC 6409 specifies 587 as the SMTP TCP port
        Smtp { port: 587 }
    }
}

#[derive(Deserialize)]
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
