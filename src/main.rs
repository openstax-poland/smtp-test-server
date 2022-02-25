// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use anyhow::Result;
use std::future::Future;

mod config;
mod mail;
mod mime;
mod smtp;
mod state;
mod syntax;
mod util;
mod web;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let config = config::load()?;
    let state = state::State::new();

    let smtp = try_spawn(smtp::server::start(config.smtp, state.clone()));
    let web = web::start(config.http, state);

    tokio::try_join!(smtp, web)?;

    Ok(())
}

async fn try_spawn(fut: impl Future<Output = Result<()>> + Send + Sync + 'static) -> Result<()> {
    match tokio::spawn(fut).await {
        Ok(result) => result,
        Err(err) => match err.try_into_panic() {
            Ok(payload) => std::panic::resume_unwind(payload),
            Err(_) => Ok(()),
        },
    }
}
