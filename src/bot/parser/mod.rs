use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, StreamTrait, TransactionTrait,
};
use tgbot::{
    api::Client,
    types::{
        AnswerInlineQuery, ChatPeerId, MaybeInaccessibleMessage, ParseMode, ReplyParameters,
        SendMessage, User,
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

pub async fn parse_message<C>(
    client: &Client,
    conn: &C,
    name: &str,
    user: &User,
    message_id: i64,
    msg: &str,
    chat_id: ChatPeerId,
) -> Result<(), Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    let chat = chat::find_or_insert(conn, chat_id).await?;

    let mut iter = msg.split_whitespace();
    let res = match iter.next().map(|msg| msg.strip_suffix(name).unwrap_or(msg)) {
        Some("/help") => help::execute(client, message_id, &chat, name)
            .await
            .map(Ok)?,
        Some("/start") => start::execute(client, conn, user, message_id, &chat).await?,
        Some("/settings") => settings::execute(client, conn, user, message_id, &chat, None).await?,
        Some("/status") => status::execute(client, conn, message_id, &chat).await?,
        Some("/rank") => rank::execute(client, conn, message_id, &chat).await?,
        Some("/close") => close::execute(client, conn, user, message_id, &chat).await?,
        _ => return Ok(()),
    };

    if let Err(err) = res {
        client
            .execute(
                SendMessage::new(chat.telegram_id, format!("Error: {err}"))
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
    let (chat_id, message_id) = match message {
        MaybeInaccessibleMessage::InaccessibleMessage(im) => (im.chat.get_id(), im.message_id),
        MaybeInaccessibleMessage::Message(m) => (m.chat.get_id(), m.id),
    };
    let chat = chat::find_or_insert(conn, chat_id).await?;

    let res = settings::execute(client, conn, user, message_id, &chat, Some(data)).await?;

    if let Err(err) = res {
        client
            .execute(
                SendMessage::new(chat.telegram_id, format!("Error: {err}"))
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
    if parse_inline_query_inner(client, conn, user, query_id, msg).await? {
        client
            .execute(AnswerInlineQuery::new(query_id, []).with_cache_time(0))
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
) -> Result<bool, Error>
where
    C: ConnectionTrait + StreamTrait + TransactionTrait,
{
    let Ok(chat_id) = msg.parse::<i32>() else {
        return Ok(true);
    };
    let Some(chat) = chat::Entity::find_by_id(chat_id).one(conn).await? else {
        return Ok(true);
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
    let Ok(hand_ids) = result_id
        .split_whitespace()
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return Ok(());
    };

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
