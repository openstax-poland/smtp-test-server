use anyhow::Result;

mod smtp;
mod syntax;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    smtp::server::start().await
}
