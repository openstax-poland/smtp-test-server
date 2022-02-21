use anyhow::Result;

mod mail;
mod smtp;
mod state;
mod syntax;
mod util;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let state = state::State::new();

    smtp::server::start(state.clone()).await
}
