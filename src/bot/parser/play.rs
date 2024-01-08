use std::{borrow::Cow, collections::HashMap, future};

use futures_util::TryStreamExt;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, StreamTrait};
use tgbot::{
    api::Client,
    types::{
        AnswerInlineQuery, InlineQueryResult, InlineQueryResultArticle, InputMessageContentText,
        ParseMode, User,
    },
};

use crate::{
    entities::{card, chat, hand, player},
    Error,
};

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    query_id: &str,
    chat: &chat::Model,
) -> Result<bool, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    // rando carlissian counts as a player
    if 3 > chat.players + chat.rando_carlissian as i32 {
        return Ok(true);
    }

    let Some(player) = player::Entity::find()
        .filter(
            player::Column::TelegramId
                .eq(i64::from(user.id))
                .and(player::Column::ChatId.eq(chat.id)),
        )
        .one(conn)
        .await?
    else {
        return Ok(true);
    };

    // when you're the judge
    if player.is_my_turn(chat) {
        as_judge(client, conn, &player, query_id, chat).await
    } else {
        as_player(client, conn, &player, query_id, chat).await
    }
}

async fn as_judge<C>(
    client: &Client,
    conn: &C,
    player: &player::Model,
    query_id: &str,
    chat: &chat::Model,
) -> Result<bool, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    let stream = player::Entity::find()
        .filter(
            player::Column::ChatId
                .eq(chat.id)
                .and(player::Column::Id.ne(player.id)),
        )
        .stream(conn)
        .await?;
    let mut players = stream
        .map_ok(|player| (player.id, Cow::Owned(player.tg_link())))
        .try_collect::<HashMap<_, _>>()
        .await?;
    if players.is_empty() {
        return Ok(true);
    }
    if chat.rando_carlissian {
        players.insert(0, Cow::Borrowed(crate::RANDO_CARLISSIAN));
    }

    let stream = hand::Entity::find()
        .filter(
            hand::Column::ChatId
                .eq(chat.id)
                .and(hand::Column::PlayedOnTurn.eq(chat.turn)),
        )
        .order_by_asc(hand::Column::Seq)
        .stream(conn)
        .await?;
    let (judge_card, hands) = stream
        .try_fold(
            (None, HashMap::with_capacity(players.len())),
            |(mut judge_card, mut hands), hand| {
                if hand.player_id == player.id {
                    judge_card = Some(hand.card_id);
                } else {
                    let player: &mut Vec<_> = hands.entry(hand.player_id).or_default();
                    player.push(hand);
                }
                future::ready(Ok((judge_card, hands)))
            },
        )
        .await?;
    let Some(judge_card) = judge_card else {
        return Ok(true);
    };
    if hands.len() < players.len() || hands.values().map(Vec::len).min() < Some(chat.pick as usize)
    {
        return Ok(true);
    }

    let stream = card::Entity::find()
        .filter(
            card::Column::Id.is_in(
                hands
                    .values()
                    .flat_map(|hand| hand.iter().map(|h| h.card_id))
                    .chain([judge_card]),
            ),
        )
        .stream(conn)
        .await?;
    let cards = stream
        .try_fold(HashMap::with_capacity(hands.len()), |mut cards, card| {
            cards.insert(card.id, card.text);
            future::ready(Ok(cards))
        })
        .await?;

    let inline = hands
        .into_iter()
        .flat_map(|(player_id, hand)| {
            let player = &players[&player_id];
            let len = hand.len();
            let (hand_ids, hand_texts) = hand.into_iter().fold(
                (Vec::with_capacity(len), Vec::with_capacity(len)),
                |(mut ids, mut texts), hand| {
                    ids.push(hand.id.to_string());
                    texts.push(cards[&hand.card_id].as_str());
                    (ids, texts)
                },
            );

            // split text in multiple lines if needed
            // official line limit is 127 chars
            // but text is trucated based on screen width
            // so we'll use 50 chars to better fit screen
            //
            // Telegram doesn't accept multiple inputs with the same id
            // so we are appending a ";{index}"
            let text = hand_texts.join(" - ");
            let text_len = text.chars().count();
            let id = hand_ids.join(" ");
            let lines = if text_len > 50 {
                text.split_whitespace()
                    .fold(Vec::<(String, String)>::new(), |mut acc, word| {
                        if let Some((_, last)) = acc.last_mut() {
                            if last.chars().count() + 1 + word.chars().count() > 50 {
                                acc.push((
                                    format!("{id};{}", acc.len()),
                                    format!("{}: {word}", acc.len() + 1),
                                ));
                            } else {
                                last.push(' ');
                                last.push_str(word)
                            }
                        } else {
                            acc.push((format!("{id};0"), format!("1: {word}")));
                        }
                        acc
                    })
            } else {
                vec![(id, text)]
            };

            let black_card = &cards[&judge_card];
            let text = hand_texts.join("\n");
            lines.into_iter().map(move |(id, line)| {
                InlineQueryResult::Article(InlineQueryResultArticle::new(
                    id,
                    InputMessageContentText::new(format!(
                        "*{}*\n\nI've choosen {}'s card{}:\n\n*{}*",
                        black_card,
                        player,
                        if len > 1 { "s" } else { "" },
                        text
                    ))
                    .with_parse_mode(ParseMode::Markdown),
                    line,
                ))
            })
        })
        .collect::<Vec<_>>();

    client
        .execute(AnswerInlineQuery::new(query_id, inline).with_cache_time(0))
        .await?;

    Ok(false)
}

async fn as_player<C>(
    client: &Client,
    conn: &C,
    player: &player::Model,
    query_id: &str,
    chat: &chat::Model,
) -> Result<bool, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    let stream = hand::Entity::find()
        .filter(
            hand::Column::PlayerId.eq(player.id).and(
                hand::Column::PlayedOnTurn
                    .eq(chat.turn)
                    .or(hand::Column::PlayedOnTurn.is_null()),
            ),
        )
        .stream(conn)
        .await?;

    let (played, hands) = stream
        .try_fold((0, HashMap::new()), |(played, mut hands), hand| {
            future::ready(Ok((played + hand.played_on_turn.is_some() as i32, {
                if hand.played_on_turn.is_none() {
                    hands.insert(hand.card_id, hand.id);
                }
                hands
            })))
        })
        .await?;
    if played >= chat.pick {
        return Ok(true);
    }

    let stream = card::Entity::find()
        .filter(
            card::Column::Id
                .is_in(hands.keys().copied())
                .and(card::Column::Color.eq(card::Color::White)),
        )
        .stream(conn)
        .await?;
    let cards = stream
        .map_ok(|card| {
            InlineQueryResult::Article(InlineQueryResultArticle::new(
                hands[&card.id].to_string(),
                if chat.pick == 1 {
                    InputMessageContentText::new("I've choosen my card")
                } else {
                    InputMessageContentText::new(format!("I've choosen my {}° card", played + 1))
                },
                card.text.clone(),
            ))
        })
        .try_collect::<Vec<_>>()
        .await?;

    client
        .execute(AnswerInlineQuery::new(query_id, cards).with_cache_time(0))
        .await?;

    Ok(false)
}
