use std::borrow::Cow;

use chrono::Utc;
use futures_util::TryStreamExt;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, StreamTrait,
};
use tgbot::{
    api::Client,
    types::{ParseMode, ReplyParameters, SendMessage, User},
};

use crate::{
    entities::{
        chat::{self, Model as Chat},
        hand, player,
    },
    Error,
};

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    message_id: i64,
    chat: &Chat,
) -> Result<Result<(), Cow<'static, str>>, Error>
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

        return Ok(Err(format!(
            "You're not the game owner, only {} can use this command",
            owner.tg_link()
        )
        .into()));
    }

    let stream = player::Entity::find()
        .filter(
            player::Column::ChatId
                .eq(chat.id)
                .and(player::Column::Points.gt(0)),
        )
        .order_by_desc(player::Column::Points)
        .stream(conn)
        .await?;

    let mut players = stream
        .map_ok(|player| (player.points as i64, Cow::Owned(player.tg_link())))
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
        if won > 0 {
            players.push((won, Cow::Borrowed(crate::RANDO_CARLISSIAN)));
            players.sort_by(|(points_a, _), (points_b, _)| points_b.cmp(points_a));
        }
    }

    let msg = if players.is_empty() {
        String::from("Error: you can't close an unstarted game")
    } else {
        chat::ActiveModel {
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

        format!(
            "After {} turns the winner{} {} with {} points{}",
            chat.turn - 1,
            if winners.len() > 1 { "s are" } else { " is" },
            winners.join(" and "),
            winner_points,
            if winners.len() == 1 && winners[0] == crate::RANDO_CARLISSIAN {
                "\n\n*SHAME ON YOU!!!*"
            } else {
                ""
            }
        )
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
