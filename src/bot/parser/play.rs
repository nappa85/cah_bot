use std::{collections::HashMap, future};

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
    entities::{chat::Model as Chat, player},
    Error,
};

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    query_id: &str,
    chat: &Chat,
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
    chat: &Chat,
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
        .map_ok(|player| (player.id, player.tg_link()))
        .try_collect::<HashMap<_, _>>()
        .await?;
    if players.is_empty() {
        return Ok(true);
    }
    if chat.rando_carlissian {
        players.insert(0, String::from("Rando Carlissian"));
    }

    let stream = crate::entities::hand::Entity::find()
        .filter(
            crate::entities::hand::Column::ChatId.eq(chat.id).and(
                crate::entities::hand::Column::PlayerId
                    .ne(player.id)
                    .and(crate::entities::hand::Column::PlayedOnTurn.eq(chat.turn)),
            ),
        )
        .order_by_asc(crate::entities::hand::Column::Seq)
        .stream(conn)
        .await?;
    let hands = stream
        .try_fold(HashMap::with_capacity(players.len()), |mut hands, hand| {
            let player: &mut Vec<_> = hands.entry(hand.player_id).or_default();
            player.push(hand);
            future::ready(Ok(hands))
        })
        .await?;
    if hands.len() < players.len() || hands.values().map(Vec::len).min() < Some(chat.pick as usize)
    {
        return Ok(true);
    }

    let stream = crate::entities::card::Entity::find()
        .filter(
            crate::entities::card::Column::Id
                .is_in(
                    hands
                        .values()
                        .flat_map(|hand| hand.iter().map(|h| h.card_id)),
                )
                .and(crate::entities::card::Column::Color.eq(crate::entities::card::Color::White)),
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
        .map(|(player_id, hand)| {
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

            InlineQueryResult::Article(InlineQueryResultArticle::new(
                hand_ids.join(" "),
                InputMessageContentText::new(format!(
                    "I've choosen {}'s card{}:\n\n*{}*",
                    player,
                    if len > 1 { "s" } else { "" },
                    hand_texts.join("\n")
                ))
                .with_parse_mode(ParseMode::Markdown),
                hand_texts.join(" - "),
            ))
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
    chat: &Chat,
) -> Result<bool, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    let stream = crate::entities::hand::Entity::find()
        .filter(
            crate::entities::hand::Column::PlayerId.eq(player.id).and(
                crate::entities::hand::Column::PlayedOnTurn
                    .eq(chat.turn)
                    .or(crate::entities::hand::Column::PlayedOnTurn.is_null()),
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

    let stream = crate::entities::card::Entity::find()
        .filter(
            crate::entities::card::Column::Id
                .is_in(hands.keys().copied())
                .and(crate::entities::card::Column::Color.eq(crate::entities::card::Color::White)),
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
