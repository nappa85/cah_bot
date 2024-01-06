use std::time::Duration;

use sea_orm::{ConnectionTrait, StreamTrait, TransactionTrait};
use tgbot::{
    api::Client,
    types::{
        CallbackQuery, ChosenInlineResult, GetUpdates, InlineQuery, Message, MessageData, Text,
        UpdateType,
    },
};

use crate::Error;

mod parser;

pub async fn execute<C>(conn: &C, token: String, name: &str) -> Result<(), Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    let client = Client::new(token)?;

    let mut offset = -1;
    loop {
        let updates = client
            .execute(
                GetUpdates::default()
                    .with_timeout(Duration::from_secs(3600))
                    .with_offset(offset + 1),
            )
            .await?;
        for update in updates {
            let Some(user) = update.get_user() else {
                continue;
            };

            match update.update_type {
                UpdateType::Message(Message {
                    id,
                    ref chat,
                    data: MessageData::Text(Text { ref data, .. }),
                    ..
                }) => {
                    parser::parse_message(&client, conn, name, user, id, data, chat.get_id())
                        .await?;
                }
                UpdateType::InlineQuery(InlineQuery {
                    ref id, ref query, ..
                }) => {
                    parser::parse_inline_query(&client, conn, user, id, query).await?;
                }
                UpdateType::ChosenInlineResult(ChosenInlineResult { ref result_id, .. }) => {
                    parser::parse_inline_query_response(&client, conn, user, result_id).await?;
                }
                UpdateType::CallbackQuery(CallbackQuery {
                    message: Some(ref msg),
                    data: Some(ref data),
                    ..
                }) => parser::parse_callback_query(&client, conn, msg, data).await?,
                _ => println!("Update {update:?}"),
            }
            offset = update.id;
        }
    }
}
