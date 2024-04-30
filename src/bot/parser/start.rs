use std::borrow::Cow;

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    TransactionTrait,
};
use tgbot::{
    api::Client,
    types::{InlineKeyboardButton, ParseMode, ReplyParameters, SendMessage, User},
};

use crate::{
    entities::{chat, player},
    Error,
};

#[derive(thiserror::Error, Debug)]
pub enum StartError {
    #[error("Player already exists")]
    AlreadyExists,
    #[error(transparent)]
    Chat(#[from] chat::ChatError),
}

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    message_id: i64,
    chat: &chat::Model,
) -> Result<Result<(), StartError>, Error>
where
    C: ConnectionTrait + TransactionTrait,
{
    if player::Entity::find()
        .filter(
            player::Column::TelegramId
                .eq(i64::from(user.id))
                .and(player::Column::ChatId.eq(chat.id)),
        )
        .one(conn)
        .await?
        .is_some()
    {
        return Ok(Err(StartError::AlreadyExists));
    }

    let txn = conn.begin().await?;

    let player = player::insert(
        &txn,
        user.id,
        chat.id,
        if let Some(last_name) = &user.last_name {
            format!("{} {last_name}", user.first_name)
        } else {
            user.first_name.clone()
        },
    )
    .await?;

    let chat = chat::ActiveModel {
        id: ActiveValue::Set(chat.id),
        owner: if chat.players == 0 {
            ActiveValue::Set(Some(player.id))
        } else {
            ActiveValue::NotSet
        },
        players: ActiveValue::Set(chat.players + 1),
        ..Default::default()
    }
    .update(&txn)
    .await?;

    let msg = format!(
        "Player created{}\n\n{}",
        match chat.players {
            1 => Cow::Borrowed(", you're the owner of this game, that means you're the only one who can use /settings and /close the game, you can start to play as soon as someone else joins"),
            2 => Cow::Owned(format!(", you're the second one on this game, you can start playing by enabling {} from /settings", crate::RANDO_CARLISSIAN)),
            3 => Cow::Owned(format!(", you're the third one on this game, you can now play freely without {}", crate::RANDO_CARLISSIAN)),
            _ => Cow::Borrowed(""),
        },
        if chat.players + chat.rando_carlissian as i32 > 2 {
            match chat.reset(&txn).await? {
                Ok(msg) => msg,
                Err(e) => return Ok(Err(StartError::from(e))),
            }
        } else {
            String::new()
        }
    );
    txn.commit().await?;

    client
        .execute(
            SendMessage::new(chat.telegram_id, msg)
                .with_reply_markup(
                    [[InlineKeyboardButton::for_switch_inline_query_current_chat(
                        "Open cards hand",
                        chat.id.to_string(),
                    )]],
                )
                .with_reply_parameters(ReplyParameters::new(message_id))
                .with_parse_mode(ParseMode::MarkdownV2),
        )
        .await?;

    Ok(Ok(()))
}
