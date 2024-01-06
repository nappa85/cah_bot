use std::borrow::Cow;

use chrono::Utc;
use futures_util::TryStreamExt;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, StreamTrait,
};
use tgbot::{
    api::Client,
    types::{ParseMode, ReplyParameters, SendMessage},
};

use crate::{entities::chat::Model as Chat, Error};

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    message_id: i64,
    chat: &Chat,
) -> Result<Result<(), Cow<'static, str>>, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    let stream = crate::entities::player::Entity::find()
        .filter(
            crate::entities::player::Column::ChatId
                .eq(chat.id)
                .and(crate::entities::player::Column::Points.gt(0)),
        )
        .order_by_desc(crate::entities::player::Column::Points)
        .stream(conn)
        .await?;

    let mut players = stream
        .map_ok(|player| (player.points, player.tg_link()))
        .try_collect::<Vec<_>>()
        .await?;

    if chat.rando_carlissian {
        let won = crate::entities::hand::Entity::find()
            .filter(
                crate::entities::hand::Column::ChatId
                    .eq(chat.id)
                    .and(crate::entities::hand::Column::PlayerId.eq(0))
                    .and(crate::entities::hand::Column::Won.eq(true)),
            )
            .select_only()
            .column_as(crate::entities::hand::Column::Id.count(), "ids")
            .into_tuple::<Option<i64>>()
            .one(conn)
            .await?
            .flatten()
            .unwrap_or_default();
        if won > 0 {
            players.push((won as i32, String::from("Rando Carlissian")));
            players.sort_by(|(points_a, _), (points_b, _)| points_b.cmp(points_a));
        }
    }

    let msg = if players.is_empty() {
        String::from("No points assigned in this game")
    } else {
        crate::entities::chat::ActiveModel {
            id: ActiveValue::Set(chat.id),
            end_date: ActiveValue::Set(Some(Utc::now().naive_utc())),
            ..Default::default()
        }
        .update(conn)
        .await?;

        let winner_points = players[0].0;
        let winners = players
            .into_iter()
            .map_while(|(points, player)| (points == winner_points).then_some(player))
            .collect::<Vec<_>>();

        if winners.len() == 1 {
            format!(
                "After {} turns the winner is {} with {} points",
                chat.turn, winners[0], winner_points
            )
        } else {
            format!(
                "After {} turns the winners are {} with {} points",
                chat.turn,
                winners.join(" and "),
                winner_points
            )
        }
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
