use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect, TransactionTrait,
};
use tgbot::{
    api::Client,
    types::{
        EditMessageReplyMarkup, InlineKeyboardButton, ParseMode, ReplyParameters, SendMessage, User,
    },
};

use crate::{
    entities::{chat, chat_pack, pack, player},
    Error,
};

const ENABLED: &str = "☑";
const DISABLED: &str = "◻";

#[derive(thiserror::Error, Debug)]
pub enum SettingsError {
    #[error("You're not the game owner, only {0} can use this command")]
    NotOwner(String),
    #[error("You can't change setting on an already started game")]
    AlreadyStarter,
    #[error(transparent)]
    Chat(#[from] chat::ChatError),
}

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    message_id: i64,
    chat: &chat::Model,
    query_data: Option<&str>,
) -> Result<Result<(), SettingsError>, Error>
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

    if chat.turn > 1 {
        return Ok(Err(SettingsError::AlreadyStarter));
    }

    if chat.owner != Some(player.id) {
        // avoid spamming errors at every button clicked
        if query_data.is_none() {
            let Some(owner) = player::Entity::find_by_id(chat.owner.unwrap_or_default())
                .one(conn)
                .await?
            else {
                return Ok(Ok(()));
            };

            return Ok(Err(SettingsError::NotOwner(owner.tg_link())));
        } else {
            return Ok(Ok(()));
        }
    }

    let packs = pack::Entity::find().all(conn).await?;
    let officials = packs
        .iter()
        .filter_map(|pack| pack.official.then_some(pack.id))
        .collect::<Vec<_>>();

    let mut enabled = chat_pack::Entity::find()
        .filter(chat_pack::Column::ChatId.eq(chat.id))
        .select_only()
        .column(chat_pack::Column::PackId)
        .into_tuple::<i32>()
        .all(conn)
        .await?;
    let mut all_officials_enabled = officials.iter().all(|id| enabled.contains(id));

    let mut rando_carlissian = chat.rando_carlissian;
    let mut close = false;
    let mut start = 0;
    if let Some(data) = query_data {
        match data {
            "close" => {
                close = true;
            }
            action if action.starts_with("skip") => {
                start = action[4..].parse().unwrap_or_default();
            }
            action if action.starts_with("rando") => {
                start = action[5..].parse().unwrap_or_default();
                let txn = conn.begin().await?;

                let chat = chat::ActiveModel {
                    id: ActiveValue::Set(chat.id),
                    rando_carlissian: ActiveValue::Set(!chat.rando_carlissian),
                    ..Default::default()
                }
                .update(&txn)
                .await?;
                rando_carlissian = chat.rando_carlissian;

                let msg = match chat.reset(&txn).await? {
                    Ok(msg) => msg,
                    Err(e) => return Ok(Err(SettingsError::from(e))),
                };
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
                                .with_parse_mode(ParseMode::MarkdownV2),
                        )
                        .await?;
                }
            }
            action if action.starts_with("all") => {
                start = action[3..].parse().unwrap_or_default();
                if packs.len() == enabled.len() {
                    for pack in &packs {
                        chat_pack::ActiveModel {
                            chat_id: ActiveValue::Set(chat.id),
                            pack_id: ActiveValue::Set(pack.id),
                        }
                        .delete(conn)
                        .await?;
                    }
                    enabled.clear();
                    all_officials_enabled = false;
                } else {
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
                    all_officials_enabled = true;
                }
            }
            action if action.starts_with("official") => {
                start = action[8..].parse().unwrap_or_default();
                if all_officials_enabled {
                    for official in &officials {
                        if let Some(index) =
                            enabled.iter().position(|enabled_id| enabled_id == official)
                        {
                            chat_pack::ActiveModel {
                                chat_id: ActiveValue::Set(chat.id),
                                pack_id: ActiveValue::Set(*official),
                            }
                            .delete(conn)
                            .await?;
                            enabled.remove(index);
                        }
                    }
                    all_officials_enabled = false;
                } else {
                    for official in &officials {
                        if !enabled.contains(official) {
                            chat_pack::ActiveModel {
                                chat_id: ActiveValue::Set(chat.id),
                                pack_id: ActiveValue::Set(*official),
                            }
                            .insert(conn)
                            .await?;
                            enabled.push(*official);
                        }
                    }
                    all_officials_enabled = true;
                }
            }
            id => {
                if let Some((Ok(id), s)) = id
                    .split_once('-')
                    .map(|(id, start)| (id.parse::<i32>(), start.parse().unwrap_or_default()))
                {
                    start = s;
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
            format!("rando{start}"),
        )]);
        keyboard.push(vec![InlineKeyboardButton::for_callback_data(
            format!(
                "{} all packs",
                if packs.len() == enabled.len() {
                    "Disable"
                } else {
                    "Enable"
                }
            ),
            format!("all{start}"),
        )]);
        keyboard.push(vec![InlineKeyboardButton::for_callback_data(
            format!(
                "{} official packs",
                if all_officials_enabled {
                    "Disable"
                } else {
                    "Enable"
                }
            ),
            format!("official{start}"),
        )]);
        for pack in packs.iter().skip(start).take(15) {
            keyboard.push(vec![InlineKeyboardButton::for_callback_data(
                format!(
                    "{} {}",
                    pack.name(),
                    if enabled.contains(&pack.id) {
                        ENABLED
                    } else {
                        DISABLED
                    }
                ),
                format!("{}-{start}", pack.id),
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
