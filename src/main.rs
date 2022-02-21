use anyhow::Result;
use std::future::Future;

mod mail;
mod smtp;
mod state;
mod syntax;
mod util;
mod web;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let state = state::State::new();

    let smtp = try_spawn(smtp::server::start(state.clone()));
    let web = web::start(state);

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
