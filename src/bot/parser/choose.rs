use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect, StreamTrait, TransactionTrait,
};
use tgbot::{
    api::Client,
    types::{InlineKeyboardButton, ParseMode, SendMessage, User},
};

use crate::{
    entities::{chat, hand, player},
    Error,
};

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    hands: &[hand::Model],
) -> Result<(), Error>
where
    C: ConnectionTrait + TransactionTrait + StreamTrait,
{
    let mut chat_ids = hands.iter().map(|hand| hand.chat_id).collect::<Vec<_>>();
    chat_ids.dedup();
    if chat_ids.len() != 1 {
        return Ok(());
    }

    let Some(chat) = chat::Entity::find_by_id(chat_ids[0]).one(conn).await? else {
        return Ok(());
    };

    let Some(player) = player::Entity::find()
        .filter(
            player::Column::TelegramId
                .eq(i64::from(user.id))
                .and(player::Column::ChatId.eq(chat.id)),
        )
        .one(conn)
        .await?
    else {
        return Ok(());
    };

    // when you're the judge
    let res = if player.is_my_turn(&chat) {
        as_judge(client, conn, &chat, hands).await?
    } else {
        if hands.len() != 1 {
            return Ok(());
        }

        as_player(client, conn, &player, &chat, &hands[0]).await?
    };

    if let Err(e) = res {
        let msg = match chat.close(conn).await? {
            Ok(msg) => format!("{e}\n\n{msg}"),
            Err(err) => format!("{e}\n\nError: {err}"),
        };

        client
            .execute(SendMessage::new(chat.telegram_id, msg).with_parse_mode(ParseMode::Markdown))
            .await?;
    }

    Ok(())
}

async fn as_judge<C>(
    client: &Client,
    conn: &C,
    chat: &chat::Model,
    hands: &[hand::Model],
) -> Result<Result<(), chat::ChatError>, Error>
where
    C: ConnectionTrait + TransactionTrait,
{
    let txn = conn.begin().await?;

    let mut player_ids = hands
        .iter()
        .map(|hand: &hand::Model| hand.player_id)
        .collect::<Vec<_>>();
    player_ids.dedup();
    if player_ids.len() != 1 {
        return Ok(Ok(()));
    }
    let player_id = player_ids[0];

    if player_id > 0 {
        let Some(player) = player::Entity::find_by_id(player_id).one(&txn).await? else {
            return Ok(Ok(()));
        };

        player::ActiveModel {
            id: ActiveValue::Set(player.id),
            points: ActiveValue::Set(player.points + 1),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }

    for hand in hands {
        hand::ActiveModel {
            id: ActiveValue::Set(hand.id),
            won: ActiveValue::Set(true),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }

    let chat = chat::ActiveModel {
        id: ActiveValue::Set(chat.id),
        turn: ActiveValue::Set(chat.turn + 1),
        ..Default::default()
    }
    .update(&txn)
    .await?;

    let msg = match chat.reset(&txn).await? {
        Ok(msg) => msg,
        Err(e) => return Ok(Err(e)),
    };
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
                .with_parse_mode(ParseMode::Markdown),
        )
        .await?;

    Ok(Ok(()))
}

async fn as_player<C>(
    client: &Client,
    conn: &C,
    player: &player::Model,
    chat: &chat::Model,
    hand: &hand::Model,
) -> Result<Result<(), chat::ChatError>, Error>
where
    C: ConnectionTrait,
{
    let played = hand::Entity::find()
        .filter(
            hand::Column::ChatId
                .eq(chat.id)
                .and(hand::Column::PlayedOnTurn.eq(Some(chat.turn)))
                .and(hand::Column::PlayerId.eq(player.id)),
        )
        .select_only()
        .column_as(hand::Column::Id.count(), "ids")
        .into_tuple::<Option<i64>>()
        .one(conn)
        .await?
        .flatten()
        .unwrap_or_default();
    if played >= chat.pick as i64 {
        return Ok(Ok(()));
    }

    hand::ActiveModel {
        id: ActiveValue::Set(hand.id),
        played_on_turn: ActiveValue::Set(Some(chat.turn)),
        seq: ActiveValue::Set(played as i32),
        ..Default::default()
    }
    .update(conn)
    .await?;

    let played = hand::Entity::find()
        .filter(
            hand::Column::ChatId
                .eq(chat.id)
                .and(hand::Column::PlayedOnTurn.eq(Some(chat.turn)))
                .and(hand::Column::PlayerId.gt(0)),
        )
        .select_only()
        .column_as(hand::Column::Id.count(), "count")
        .into_tuple::<Option<i64>>()
        .one(conn)
        .await?
        .flatten()
        .unwrap_or_default();

    // judge always plays only 1 card
    if played > (chat.players as i64 - 1) * chat.pick as i64 {
        let Some(judge) = player::Entity::find()
            .filter(
                player::Column::ChatId
                    .eq(chat.id)
                    .and(player::Column::Turn.eq(chat.next_player_turn())),
            )
            .one(conn)
            .await?
        else {
            return Ok(Ok(()));
        };

        let msg = format!(
            "All players have choosen their card{}, now {} can choose the winner",
            if chat.pick > 1 { "s" } else { "" },
            judge.tg_link()
        );

        client
            .execute(
                SendMessage::new(chat.telegram_id, msg)
                    .with_reply_markup([[
                        InlineKeyboardButton::for_switch_inline_query_current_chat(
                            "Open cards hand",
                            chat.id.to_string(),
                        ),
                    ]])
                    .with_parse_mode(ParseMode::Markdown),
            )
            .await?;
    }

    Ok(Ok(()))
}
