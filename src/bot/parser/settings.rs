use std::borrow::Cow;

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect,
};
use tgbot::{
    api::Client,
    types::{EditMessageReplyMarkup, InlineKeyboardButton, ReplyParameters, SendMessage},
};

use crate::{entities::chat::Model as Chat, Error};

const ENABLED: &str = "☑";
const DISABLED: &str = "◻";

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    message_id: i64,
    chat: &Chat,
    query_data: Option<&str>,
) -> Result<Result<(), Cow<'static, str>>, Error>
where
    C: ConnectionTrait,
{
    let packs = crate::entities::pack::Entity::find().all(conn).await?;

    let mut enabled = crate::entities::chat_pack::Entity::find()
        .filter(crate::entities::chat_pack::Column::ChatId.eq(chat.id))
        .select_only()
        .column(crate::entities::chat_pack::Column::PackId)
        .into_tuple::<i32>()
        .all(conn)
        .await?;

    let mut rando_carlissian = chat.rando_carlissian;
    let mut close = false;
    let mut start = 0;
    if let Some(data) = query_data {
        match data {
            "rando" => {
                crate::entities::chat::ActiveModel {
                    id: ActiveValue::Set(chat.id),
                    rando_carlissian: ActiveValue::Set(!chat.rando_carlissian),
                    ..Default::default()
                }
                .update(conn)
                .await?;
                rando_carlissian = !chat.rando_carlissian;

                if rando_carlissian {
                    crate::entities::hand::draw(conn, 0, chat.id, chat.turn, false).await?;
                } else {
                    let hands = crate::entities::hand::Entity::find()
                        .filter(
                            crate::entities::hand::Column::ChatId
                                .eq(chat.id)
                                .and(crate::entities::hand::Column::PlayerId.eq(0)),
                        )
                        .all(conn)
                        .await?;
                    for hand in hands {
                        crate::entities::hand::ActiveModel {
                            id: ActiveValue::Set(hand.id),
                            ..Default::default()
                        }
                        .delete(conn)
                        .await?;
                    }
                }
            }
            "all" => {
                for pack in &packs {
                    if !enabled.contains(&pack.id) {
                        crate::entities::chat_pack::ActiveModel {
                            chat_id: ActiveValue::Set(chat.id),
                            pack_id: ActiveValue::Set(pack.id),
                        }
                        .insert(conn)
                        .await?;
                        enabled.push(pack.id);
                    }
                }
            }
            "official" => {
                for pack in &packs {
                    match (
                        pack.official,
                        enabled.iter().position(|enabled_id| *enabled_id == pack.id),
                    ) {
                        (true, None) => {
                            crate::entities::chat_pack::ActiveModel {
                                chat_id: ActiveValue::Set(chat.id),
                                pack_id: ActiveValue::Set(pack.id),
                            }
                            .insert(conn)
                            .await?;
                            enabled.push(pack.id);
                        }
                        (false, Some(index)) => {
                            crate::entities::chat_pack::ActiveModel {
                                chat_id: ActiveValue::Set(chat.id),
                                pack_id: ActiveValue::Set(pack.id),
                            }
                            .delete(conn)
                            .await?;
                            enabled.remove(index);
                        }
                        _ => {}
                    }
                }
            }
            "close" => {
                close = true;
            }
            action if action.starts_with("skip") => {
                start = action[4..].parse().unwrap_or_default();
            }
            id => {
                if let Ok(id) = id.parse::<i32>() {
                    if let Some(index) = enabled.iter().position(|enabled_id| *enabled_id == id) {
                        crate::entities::chat_pack::ActiveModel {
                            chat_id: ActiveValue::Set(chat.id),
                            pack_id: ActiveValue::Set(id),
                        }
                        .delete(conn)
                        .await?;
                        enabled.remove(index);
                    } else {
                        crate::entities::chat_pack::ActiveModel {
                            chat_id: ActiveValue::Set(chat.id),
                            pack_id: ActiveValue::Set(id),
                        }
                        .insert(conn)
                        .await?;
                        enabled.push(id);
                    }
                }
            }
        }
    }

    let keyboard = if close {
        Vec::new()
    } else {
        let mut keyboard = Vec::with_capacity(20);
        keyboard.push(vec![InlineKeyboardButton::for_callback_data(
            format!(
                "Rando Carlissian {}",
                if rando_carlissian { ENABLED } else { DISABLED }
            ),
            "rando",
        )]);
        keyboard.push(vec![InlineKeyboardButton::for_callback_data(
            "Enable all packs",
            "all",
        )]);
        keyboard.push(vec![InlineKeyboardButton::for_callback_data(
            "Enable only official packs",
            "official",
        )]);
        for pack in packs.iter().skip(start).take(15) {
            keyboard.push(vec![InlineKeyboardButton::for_callback_data(
                format!(
                    "{} {}",
                    pack.name,
                    if enabled.contains(&pack.id) {
                        ENABLED
                    } else {
                        DISABLED
                    }
                ),
                pack.id.to_string(),
            )]);
        }
        let mut buttons = Vec::new();
        if start > 0 {
            buttons.push(InlineKeyboardButton::for_callback_data(
                "<<",
                format!("skip{}", start - 15),
            ));
        }
        if start < packs.len() - 15 {
            buttons.push(InlineKeyboardButton::for_callback_data(
                ">>",
                format!("skip{}", start + 15),
            ));
        }
        keyboard.push(buttons);
        keyboard.push(vec![InlineKeyboardButton::for_callback_data(
            "Close settings",
            "close",
        )]);
        keyboard
    };

    if query_data.is_none() {
        client
            .execute(
                SendMessage::new(chat.telegram_id, "Chat settings")
                    .with_reply_parameters(ReplyParameters::new(message_id))
                    .with_reply_markup(keyboard),
            )
            .await?;
    } else {
        client
            .execute(
                EditMessageReplyMarkup::for_chat_message(chat.telegram_id, message_id)
                    .with_reply_markup(keyboard),
            )
            .await?;
    }

    Ok(Ok(()))
}
