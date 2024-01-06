use tgbot::{
    api::Client,
    types::{InlineKeyboardButton, ParseMode, ReplyParameters, SendMessage},
};

use crate::{entities::chat::Model as Chat, Error};

pub async fn execute(
    client: &Client,
    message_id: i64,
    chat: &Chat,
    bot_name: &str,
) -> Result<(), Error> {
    client
        .execute(
            SendMessage::new(
                chat.telegram_id,
                format!(
                    "[Cards Against Humanity Bot](https://github.com/nappa85/cah_bot/)

/help - this message
/start - start or join the game in this chat
/status - show game status
/rank - show players ranking

To view you hand and choose a card for this chat always use the inline command `@{bot_name} {}`
                ",
                    chat.id
                ),
            )
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

    Ok(())
}
