use std::{borrow::Cow, collections::HashMap, future};

use futures_util::TryStreamExt;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, StreamTrait};
use tgbot::{
    api::Client,
    types::{InlineKeyboardButton, ParseMode, ReplyParameters, SendMessage},
};

use crate::{
    entities::{card, chat, hand, player},
    Error,
};

#[derive(thiserror::Error, Debug)]
pub enum StatusError {
    #[error("Not enough players in the game")]
    NotEnoughPlayers,
    #[error("No judge in game (this is a bug)")]
    NoJudge,
    #[error("No black card in game (this is a bug)")]
    NoBlackCard,
    #[error("Multiple black card in game (this is a bug)")]
    MultiBlackCard,
    #[error("Invalid black card in game (this is a bug)")]
    InvalidBlackCard,
}

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    message_id: i64,
    chat: &chat::Model,
) -> Result<Result<(), StatusError>, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    if 3 > chat.players + chat.rando_carlissian as i32 {
        return Ok(Err(StatusError::NotEnoughPlayers));
    }

    let stream = player::Entity::find()
        .filter(player::Column::ChatId.eq(chat.id))
        .stream(conn)
        .await?;
    let (judge, mut players) = stream
        .try_fold((None, Vec::new()), |(mut judge, mut players), player| {
            if player.is_my_turn(chat) {
                judge = Some(player);
            } else {
                players.push((player.id, Cow::Owned(player.tg_link())));
            }
            future::ready(Ok((judge, players)))
        })
        .await?;

    let Some(judge) = judge else {
        return Ok(Err(StatusError::NoJudge));
    };

    if chat.rando_carlissian {
        players.push((0, Cow::Borrowed(crate::RANDO_CARLISSIAN)));
    }

    let stream = hand::Entity::find()
        .filter(
            hand::Column::ChatId
                .eq(chat.id)
                .and(hand::Column::PlayedOnTurn.eq(chat.turn)),
        )
        .stream(conn)
        .await?;

    let cards = stream
        .try_fold(
            HashMap::with_capacity(chat.players as usize),
            |mut cards, hand| {
                let hands: &mut Vec<_> = cards.entry(hand.player_id).or_default();
                hands.push(hand.card_id);
                future::ready(Ok(cards))
            },
        )
        .await?;

    let Some(judge_cards) = cards.get(&judge.id) else {
        return Ok(Err(StatusError::NoBlackCard));
    };
    if judge_cards.len() != 1 {
        return Ok(Err(StatusError::MultiBlackCard));
    }
    let Some(black_card) = card::Entity::find_by_id(judge_cards[0])
        .filter(card::Column::Color.eq(card::Color::Black))
        .one(conn)
        .await?
    else {
        return Ok(Err(StatusError::InvalidBlackCard));
    };

    let mut msg = format!(
        "Turn {}\n\n*{}*\n\nJudge is {}",
        chat.turn,
        black_card.text,
        judge.tg_link()
    );
    for player in players {
        msg.push_str(&format!(
            "\n{} have{} played{}",
            player.1,
            if cards.contains_key(&player.0) {
                ""
            } else {
                "n't"
            },
            if chat.pick == 1 {
                String::new()
            } else if let Some(hand) = cards.get(&player.0) {
                format!(
                    " {} card{}",
                    hand.len(),
                    if hand.len() > 1 { "s" } else { "" }
                )
            } else {
                String::new()
            }
        ));
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
