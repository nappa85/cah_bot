use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, StreamTrait};
use tgbot::{
    api::Client,
    types::{ParseMode, ReplyParameters, SendMessage, User},
};

use crate::{
    entities::{chat, player},
    Error,
};

#[derive(thiserror::Error, Debug)]
pub enum CloseError {
    #[error("You're not the game owner, only {0} can use this command")]
    NotOwner(String),
    #[error("You can't close an unstarted game")]
    Unstarted,
    #[error(transparent)]
    Chat(#[from] chat::ChatError),
}

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    message_id: i64,
    chat: &chat::Model,
) -> Result<Result<(), CloseError>, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    let Some(player) = player::Entity::find()
        .filter(
            player::Column::TelegramId
                .eq(i64::from(user.id))
                .and(player::Column::ChatId.eq(chat.id)),
        )
        .one(conn)
        .await?
    else {
        return Ok(Ok(()));
    };

    if chat.owner != Some(player.id) {
        let Some(owner) = player::Entity::find_by_id(chat.owner.unwrap_or_default())
            .one(conn)
            .await?
        else {
            return Ok(Ok(()));
        };

        return Ok(Err(CloseError::NotOwner(owner.tg_link())));
    }

    if chat.turn <= 1 {
        return Ok(Err(CloseError::Unstarted));
    }

    let msg = match chat.close(conn).await? {
        Ok(msg) => msg,
        Err(e) => return Ok(Err(CloseError::from(e))),
    };

    client
        .execute(
            SendMessage::new(chat.telegram_id, msg)
                .with_reply_parameters(ReplyParameters::new(message_id))
                .with_parse_mode(ParseMode::Markdown),
        )
        .await?;

    Ok(Ok(()))
}
