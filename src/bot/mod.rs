use std::time::Duration;

use sea_orm::{ConnectionTrait, StreamTrait, TransactionTrait};
use tgbot::{
    api::{Client, ExecuteError},
    types::{
        CallbackQuery, ChosenInlineResult, GetUpdates, InlineQuery, Message, MessageData, Text,
        UpdateType,
    },
};
use tokio::time;
use tracing::{debug, error, warn};

use crate::Error;

mod parser;

// ignores non-fatal errors
fn clear_error(res: Result<(), Error>) -> Result<(), Error> {
    if let Err(Error::TelegramExec(ExecuteError::Response(response_error))) = &res {
        if response_error.error_code() == Some(400) {
            warn!("Ignoring Telegram error: {response_error}");
            return Ok(());
        }
    }
    res
}

pub async fn execute<C>(conn: &C, token: String, name: &str) -> Result<(), Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    let client = Client::new(token)?;

    let mut offset = -1;
    loop {
        let updates = match client
            .execute(
                GetUpdates::default()
                    .with_timeout(Duration::from_secs(3600))
                    .with_offset(offset + 1),
            )
            .await
        {
            Ok(updates) => updates,
            Err(err) => {
                error!("Telegram poll error: {err}");
                time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        for update in updates {
            let Some(user) = update.get_user() else {
                continue;
            };

            clear_error(match update.update_type {
                UpdateType::Message(Message {
                    id,
                    ref chat,
                    data: MessageData::Text(Text { ref data, .. }),
                    ..
                }) => parser::parse_message(&client, conn, name, user, id, data, chat).await,
                UpdateType::InlineQuery(InlineQuery {
                    ref id, ref query, ..
                }) => parser::parse_inline_query(&client, conn, user, id, query).await,
                UpdateType::ChosenInlineResult(ChosenInlineResult { ref result_id, .. }) => {
                    parser::parse_inline_query_response(&client, conn, user, result_id).await
                }
                UpdateType::CallbackQuery(CallbackQuery {
                    message: Some(ref msg),
                    data: Some(ref data),
                    ..
                }) => parser::parse_callback_query(&client, conn, user, msg, data).await,
                _ => {
                    debug!("Ignoring update {update:?}");
                    Ok(())
                }
            })?;

            offset = update.id;
        }
    }
}
