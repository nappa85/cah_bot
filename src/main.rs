use std::env;

use sea_orm::{Database, DbErr};
use tgbot::api::{ClientError, ExecuteError};

mod bot;
mod entities;
mod utils;

const PACKS: &str = include_str!("../cah-cards-full.json");
static RANDO_CARLISSIAN: &str = "Rando Carlissian";

// those are all unrecoverable errors
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Missing env var BOT_TOKEN")]
    MissingBotToken,
    #[error("Missing env var BOT_NAME")]
    MissingBotName,
    #[error("Sea-orm error: {0}")]
    SeaOrm(#[from] DbErr),
    #[error("Telegram client error: {0}")]
    TelegramClient(#[from] ClientError),
    #[error("Telegram execute error: {0}")]
    TelegramExec(#[from] ExecuteError),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let token = env::var("BOT_TOKEN").map_err(|_| Error::MissingBotToken)?;
    let name = env::var("BOT_NAME").map_err(|_| Error::MissingBotName)?;
    let name = format!("@{}", name.strip_prefix('@').unwrap_or(name.as_str()));
    let conn = Database::connect("sqlite:cah.sqlite3").await?;

    entities::pack::init(&conn).await?;

    bot::execute(&conn, token, &name).await
}
