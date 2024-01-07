use std::borrow::Cow;

use futures_util::TryStreamExt;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, StreamTrait,
};
use tgbot::{
    api::Client,
    types::{InlineKeyboardButton, ParseMode, ReplyParameters, SendMessage},
};

use crate::{
    entities::{chat::Model as Chat, hand, player},
    Error,
};

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    message_id: i64,
    chat: &Chat,
) -> Result<Result<(), Cow<'static, str>>, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    let stream = player::Entity::find()
        .filter(player::Column::ChatId.eq(chat.id))
        .order_by_desc(player::Column::Points)
        .stream(conn)
        .await?;

    let mut players = stream
        .map_ok(|player| {
            (
                player.points as i64,
                format!("\n{} {} points", player.tg_link(), player.points),
            )
        })
        .try_collect::<Vec<_>>()
        .await?;

    if chat.rando_carlissian {
        let won = hand::Entity::find()
            .filter(
                hand::Column::ChatId
                    .eq(chat.id)
                    .and(hand::Column::PlayerId.eq(0))
                    .and(hand::Column::Won.eq(true)),
            )
            .select_only()
            .column_as(hand::Column::Id.count(), "ids")
            .into_tuple::<Option<i64>>()
            .one(conn)
            .await?
            .flatten()
            .unwrap_or_default();
        players.push((won, format!("\n{} {won} points", crate::RANDO_CARLISSIAN)));
        players.sort_by(|(points_a, _), (points_b, _)| points_b.cmp(points_a));
    }

    let mut msg = format!("Turn {}\n", chat.turn);
    for (_, player) in players {
        msg.push_str(&player);
    }

    client
        .execute(
            SendMessage::new(chat.telegram_id, msg)
                .with_reply_parameters(ReplyParameters::new(message_id))
                .with_reply_markup(
                    [[InlineKeyboardButton::for_switch_inline_query_current_chat(
                        "Open cards hand",
                        chat.id.to_string(),
                    )]],
                )
                .with_parse_mode(ParseMode::Markdown),
        )
        .await?;

    Ok(Ok(()))
}
