use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, StreamTrait, TransactionTrait,
};
use tgbot::{
    api::Client,
    types::{
        AnswerInlineQuery, Chat, MaybeInaccessibleMessage, ParseMode, ReplyParameters, SendMessage,
        User,
    },
};

use crate::{
    entities::{chat, hand},
    Error,
};

mod choose;
mod close;
mod help;
mod play;
mod rank;
mod settings;
mod start;
mod status;

#[derive(thiserror::Error, Debug)]
enum BotError {
    #[error(transparent)]
    Chat(#[from] chat::ChatError),
    #[error(transparent)]
    Hand(#[from] hand::PickError),
    #[error(transparent)]
    Start(#[from] start::StartError),
    #[error(transparent)]
    Settings(#[from] settings::SettingsError),
    #[error(transparent)]
    Status(#[from] status::StatusError),
    #[error(transparent)]
    Close(#[from] close::CloseError),
}

pub async fn parse_message<C>(
    client: &Client,
    conn: &C,
    name: &str,
    user: &User,
    message_id: i64,
    msg: &str,
    tg_chat: &Chat,
) -> Result<(), Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    let res = match chat::find_or_insert(conn, tg_chat).await? {
        Ok(chat) => {
            let mut iter = msg.split_whitespace();
            match iter.next().map(|msg| msg.strip_suffix(name).unwrap_or(msg)) {
                Some("/help") => Ok(help::execute(client, message_id, &chat, name).await?),
                Some("/start") => start::execute(client, conn, user, message_id, &chat)
                    .await?
                    .map_err(BotError::from),
                Some("/settings") => settings::execute(client, conn, user, message_id, &chat, None)
                    .await?
                    .map_err(BotError::from),
                Some("/status") => status::execute(client, conn, message_id, &chat)
                    .await?
                    .map_err(BotError::from),
                Some("/rank") => Ok(rank::execute(client, conn, message_id, &chat).await?),
                Some("/close") => close::execute(client, conn, user, message_id, &chat)
                    .await?
                    .map_err(BotError::from),
                _ => return Ok(()),
            }
        }
        Err(e) => Err(BotError::from(e)),
    };

    if let Err(err) = res {
        client
            .execute(
                SendMessage::new(tg_chat.get_id(), format!("Error: {err}"))
                    .with_reply_parameters(ReplyParameters::new(message_id))
                    .with_parse_mode(ParseMode::Markdown),
            )
            .await?;
    }

    Ok(())
}

pub async fn parse_callback_query<C>(
    client: &Client,
    conn: &C,
    user: &User,
    message: &MaybeInaccessibleMessage,
    data: &str,
) -> Result<(), Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    let (tg_chat, message_id) = match message {
        MaybeInaccessibleMessage::InaccessibleMessage(im) => (&im.chat, im.message_id),
        MaybeInaccessibleMessage::Message(m) => (&m.chat, m.id),
    };

    let res = match chat::find_or_insert(conn, tg_chat).await? {
        Ok(chat) => settings::execute(client, conn, user, message_id, &chat, Some(data))
            .await?
            .map_err(BotError::from),
        Err(e) => Err(BotError::from(e)),
    };

    if let Err(err) = res {
        client
            .execute(
                SendMessage::new(tg_chat.get_id(), format!("Error: {err}"))
                    .with_reply_parameters(ReplyParameters::new(message_id))
                    .with_parse_mode(ParseMode::Markdown),
            )
            .await?;
    }

    Ok(())
}

pub async fn parse_inline_query<C>(
    client: &Client,
    conn: &C,
    user: &User,
    query_id: &str,
    msg: &str,
) -> Result<(), Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    if let Err(err) = parse_inline_query_inner(client, conn, user, query_id, msg).await? {
        client
            .execute(AnswerInlineQuery::new(query_id, err).with_cache_time(0))
            .await?;
    }

    Ok(())
}

async fn parse_inline_query_inner<C>(
    client: &Client,
    conn: &C,
    user: &User,
    query_id: &str,
    msg: &str,
) -> Result<Result<(), play::PlayError>, Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    let Ok(chat_id) = msg.parse::<i32>() else {
        return Ok(Err(play::PlayError::Clear));
    };
    let Some(chat) = chat::Entity::find_by_id(chat_id).one(conn).await? else {
        return Ok(Err(play::PlayError::Clear));
    };

    play::execute(client, conn, user, query_id, &chat).await
}

pub async fn parse_inline_query_response<C>(
    client: &Client,
    conn: &C,
    user: &User,
    result_id: &str,
) -> Result<(), Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    // remove anything after a ';' then split it by whitespace and convert to i32
    let Ok(hand_ids) = result_id
        .split_once(';')
        .map(|(s, _)| s)
        .unwrap_or(result_id)
        .split_whitespace()
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return Ok(());
    };
    if hand_ids.is_empty() {
        return Ok(());
    }

    let len = hand_ids.len();
    let hands = hand::Entity::find()
        .filter(hand::Column::Id.is_in(hand_ids))
        .all(conn)
        .await?;
    if hands.len() != len {
        return Ok(());
    }

    choose::execute(client, conn, user, &hands).await
}
