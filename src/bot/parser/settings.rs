use std::borrow::Cow;

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect, TransactionTrait,
};
use tgbot::{
    api::{Client, ExecuteError},
    types::{
        EditMessageReplyMarkup, InlineKeyboardButton, ParseMode, ReplyParameters, SendMessage, User,
    },
};

use crate::{
    entities::{
        chat::{self, Model as Chat},
        chat_pack, pack, player,
    },
    Error,
};

const ENABLED: &str = "☑";
const DISABLED: &str = "◻";

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    message_id: i64,
    chat: &Chat,
    query_data: Option<&str>,
) -> Result<Result<(), Cow<'static, str>>, Error>
where
    C: ConnectionTrait + TransactionTrait,
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
        // avoid spamming errors at every button clicked
        if query_data.is_none() {
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
        } else {
            return Ok(Ok(()));
        }
    }

    let packs = pack::Entity::find().all(conn).await?;

    let mut enabled = chat_pack::Entity::find()
        .filter(chat_pack::Column::ChatId.eq(chat.id))
        .select_only()
        .column(chat_pack::Column::PackId)
        .into_tuple::<i32>()
        .all(conn)
        .await?;

    let mut rando_carlissian = chat.rando_carlissian;
    let mut close = false;
    let mut start = 0;
    if let Some(data) = query_data {
        match data {
            "rando" => {
                let txn = conn.begin().await?;

                let chat = chat::ActiveModel {
                    id: ActiveValue::Set(chat.id),
                    rando_carlissian: ActiveValue::Set(!chat.rando_carlissian),
                    ..Default::default()
                }
                .update(&txn)
                .await?;
                rando_carlissian = chat.rando_carlissian;

                let msg = chat.reset(&txn).await?;
                txn.commit().await?;

                if chat.players + chat.rando_carlissian as i32 > 2 {
                    client
                        .execute(
                            SendMessage::new(chat.telegram_id, msg)
                                .with_reply_markup([[
                                    InlineKeyboardButton::for_switch_inline_query_current_chat(
                                        "Open cards hand",
                                        chat.id.to_string(),
                                    ),
                                ]])
                                .with_reply_parameters(ReplyParameters::new(message_id))
                                .with_parse_mode(ParseMode::Markdown),
                        )
                        .await?;
                }
            }
            "all" => {
                for pack in &packs {
                    if !enabled.contains(&pack.id) {
                        chat_pack::ActiveModel {
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
                            chat_pack::ActiveModel {
                                chat_id: ActiveValue::Set(chat.id),
                                pack_id: ActiveValue::Set(pack.id),
                            }
                            .insert(conn)
                            .await?;
                            enabled.push(pack.id);
                        }
                        (false, Some(index)) => {
                            chat_pack::ActiveModel {
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
                        chat_pack::ActiveModel {
                            chat_id: ActiveValue::Set(chat.id),
                            pack_id: ActiveValue::Set(id),
                        }
                        .delete(conn)
                        .await?;
                        enabled.remove(index);
                    } else {
                        chat_pack::ActiveModel {
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
                "{} {}",
                crate::RANDO_CARLISSIAN,
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
        let res = client
            .execute(
                EditMessageReplyMarkup::for_chat_message(chat.telegram_id, message_id)
                    .with_reply_markup(keyboard),
            )
            .await;
        // if the page displayed doesn't change, we get a futile error
        if let Err(ExecuteError::Response(ref err)) = res {
            if err.description().contains("message is not modified") {
                return Ok(Ok(()));
            }
        }
        res?;
    }

    Ok(Ok(()))
}
