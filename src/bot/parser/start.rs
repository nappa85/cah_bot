use std::borrow::Cow;

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    TransactionTrait,
};
use tgbot::{
    api::Client,
    types::{InlineKeyboardButton, ParseMode, ReplyParameters, SendMessage, User},
};

use crate::{entities::chat::Model as Chat, Error};

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    message_id: i64,
    chat: &Chat,
) -> Result<Result<(), Cow<'static, str>>, Error>
where
    C: ConnectionTrait + TransactionTrait,
{
    if crate::entities::player::Entity::find()
        .filter(
            crate::entities::player::Column::TelegramId
                .eq(i64::from(user.id))
                .and(crate::entities::player::Column::ChatId.eq(chat.id)),
        )
        .one(conn)
        .await?
        .is_some()
    {
        return Ok(Err("Player already exists".into()));
    }

    let txn = conn.begin().await?;

    let player = crate::entities::player::insert(
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

    let chat = crate::entities::chat::ActiveModel {
        id: ActiveValue::Set(chat.id),
        players: ActiveValue::Set(chat.players + 1),
        ..Default::default()
    }
    .update(&txn)
    .await?;

    let black_card = crate::entities::hand::draw(
        &txn,
        player.id,
        chat.id,
        chat.turn,
        player.is_my_turn(&chat),
    )
    .await?;

    if let Some(card) = black_card.as_ref() {
        let pick = card.pick();

        if chat.rando_carlissian {
            for _ in 0..pick {
                crate::entities::hand::draw(&txn, 0, chat.id, chat.turn, false).await?;
            }
        }

        crate::entities::chat::ActiveModel {
            id: ActiveValue::Set(chat.id),
            pick: ActiveValue::Set(pick),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }

    txn.commit().await?;

    let msg = format!(
        "Player created{}{}{}",
        match chat.players {
            1 => ", you're the first one on this chat",
            2 => ", you're the second one on this chat",
            _ => "",
        },
        if chat.players < 2 + !chat.rando_carlissian as i32 {
            ", a game will start as soon as someone else joins"
        } else {
            ""
        },
        if let Some(card) = black_card {
            format!("\n\n{}", card.descr())
        } else {
            String::new()
        }
    );

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
                .with_parse_mode(ParseMode::Markdown),
        )
        .await?;

    Ok(Ok(()))
}
