use std::{borrow::Cow, collections::HashMap, future, option::IntoIter};

use futures_util::{stream, TryStreamExt};
use rand::Rng;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, StreamTrait,
};
use tgbot::{
    api::Client,
    types::{
        AnswerInlineQuery, InlineKeyboardButton, InlineQueryResult, InlineQueryResultArticle,
        InputMessageContentText, ParseMode, User,
    },
};

use crate::{
    entities::{card, chat, hand, player},
    Error,
};

static SILLY_RESPONSES: &[&str] = &[
    "I'm a silly person and I press on errors",
    "Are you gonna eat that?",
    "If I close my eyes I can't see you\\!",
    "I'm reviewing my life",
    "Now I'm in a good mood",
    "I want to be better, I swear\\!",
    "If you had my life, you'd be happy too",
    "There's a lot in my head",
    "I stay up late thinking about it and it's overwhelming",
    "Everything is awesome\\! Like awesome awesome awesome awesome awesome awesome\\-er than before\\!",
    "I'm on the verge of tears, but it's okay",
    "I have a lot to cry about",
    "It all depends on how YOU feel\\.\\.\\.",
    "Oh, I'm not fine",
    "I don't want to talk about it",
    "Crazy is one\\-word people should use to describe me",
    "I have too much on my mind right now",
    "I'm on the road to being awesome",
    "It's been a while since I've been great",
    "I'm more of a dog person",
    "Asking me is like asking an apple how it feels about oranges",
    "I'm more of a cat person",
    "I am so overwhelmed right now, I think my brain is going to explode",
    "I'm in a good mood, but not great",
    "I'm in a bad mood, but it's okay because I don't want to make you feel awkward by telling you how much better I would be if you weren't here right now",
    "People always say that you shouldn't complain about your life, but what else should I talk about?",
    "I just had a deep conversation with myself",
    "I don't think that it went very well",
    "Good Morning\\! Now that I have woken up, my day is ruined",
    "It's hard to wake up in the morning when you're always tired",
    "I am like a box of chocolates; nobody knows what they're going to get",
    "I feel like a chicken in a burger factory",
    "My favorite phrase is “it could be worse”",
    "I'm feeling pretty good about myself, though I can't quite remember why right now",
    "My therapist told me to stay off the internet until she approves my new profile picture",
    "There are times when I sit and look at my hands and wonder, “What if they were feet?”",
    "Happiness is just around the corner\\.\\.\\.let's go around again\\!",
    "I wish I had the energy of a newborn baby\\.\\.\\.oh, wait\\. That would require getting out of bed",
    "I would say that I don't have enough information to answer “how are you”, but that wouldn't be true",
    "The difference between “I’m fine” and “I’ve been better” is about 3 coffees",
    "I tried to think of something deep and meaningful, but I thought too hard and hurt myself",
    "I just remembered what you said and then apparently forgot again",
    "I’m good because I listen to Katy Perry",
    "I’ve been pretty well, but then I woke up and realized that was all a dream",
    "I’m bad at directions so it’s difficult for me to tell",
    "I’m not sure, you tell me",
    "I haven’t done anything particularly noteworthy",
    "Sometimes, words are not necessary",
    "Nobody tells me how to live my life\\!",
    "I’m happy because today I saw a dog wearing sunglasses and it was adorable\\!",
    "Just another day in my wonderful life",
    "If you check out enough monkeys, sooner or later one of them will start typing Shakespeare",
    "They're taking the hobbits to Isengard\\!",
    "In case you haven’t noticed, I’m weird\\. I’m a weirdo",
    "Supercalifragilisticexpialidocious\\!",
    "May the Force be with you",
    "There's no place like home",
    "I’m the king of the world\\!",
    "My mama always said life was like a box of chocolates\\. You never know what you're gonna get",
    "You're gonna need a bigger boat",
    "My precious\\!",
    "Houston, we have a problem",
    "E\\.T\\. phone home",
    "You can't handle the truth\\!",
    "A martini\\. Shaken, not stirred",
    "I am your father",
    "What we've got here is failure to communicate",
    "Hasta la vista, baby",
    "Bond\\. James Bond",
    "Roads? Where we're going we don't need roads",
    "Nobody puts Baby in a corner",
    "Well, nobody's perfect",
    "They may take our lives, but they'll never take our freedom\\!",
    "To infinity and beyond\\!",
    "Toto, I've a feeling we're not in Kansas anymore",
    "Harambe died for our sins\\!",
];

#[derive(thiserror::Error, Debug)]
pub enum PlayError {
    #[error("")]
    Clear,
    #[error("🛑 This game have been closed")]
    GameEnded,
    #[error("⚠️ Not enough players in the game")]
    NotEnoughPlayers,
    #[error("⛔ You're not part of this game, use /start to join")]
    PlayerNotFound,
    #[error("🪳 No black card in game (this is a bug)")]
    NoBlackCard,
    #[error("⏳ It's not your turn to play")]
    NotJudgeTurn,
    #[error("⌛ You already played this turn")]
    AlreadyPlayed,
}

impl IntoIterator for PlayError {
    type Item = InlineQueryResult;
    type IntoIter = IntoIter<InlineQueryResult>;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            PlayError::Clear => None.into_iter(),
            err => {
                let mut rng = rand::thread_rng();
                let index = rng.gen_range(0..SILLY_RESPONSES.len());
                Some(InlineQueryResult::Article(InlineQueryResultArticle::new(
                    ";",
                    InputMessageContentText::new(SILLY_RESPONSES[index]),
                    err.to_string(),
                )))
                .into_iter()
            }
        }
    }
}

pub async fn execute<C>(
    client: &Client,
    conn: &C,
    user: &User,
    query_id: &str,
    chat: &chat::Model,
) -> Result<Result<(), PlayError>, Error>
where
    C: ConnectionTrait + StreamTrait,
{
    if chat.end_date.is_some() {
        return Ok(Err(PlayError::GameEnded));
    }

    // rando carlissian counts as a player
    if 3 > chat.players + chat.rando_carlissian as i32 {
        return Ok(Err(PlayError::NotEnoughPlayers));
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
        return Ok(Err(PlayError::PlayerNotFound));
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
) -> Result<Result<(), PlayError>, Error>
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
        return Ok(Err(PlayError::NotEnoughPlayers));
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
        return Ok(Err(PlayError::NoBlackCard));
    };
    if hands.len() < players.len() || hands.values().map(Vec::len).min() < Some(chat.pick as usize)
    {
        return Ok(Err(PlayError::NotJudgeTurn));
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
            cards.insert(card.id, card.text());
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

            let lines = split_multiline_cards(hand_texts.join(" - "), hand_ids.join(" "));

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
                    .with_parse_mode(ParseMode::MarkdownV2),
                    crate::utils::unescape_markdown(line),
                ))
            })
        })
        .collect::<Vec<_>>();

    client
        .execute(AnswerInlineQuery::new(query_id, inline).with_cache_time(0))
        .await?;

    Ok(Ok(()))
}

async fn as_player<C>(
    client: &Client,
    conn: &C,
    player: &player::Model,
    query_id: &str,
    chat: &chat::Model,
) -> Result<Result<(), PlayError>, Error>
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
        return Ok(Err(PlayError::AlreadyPlayed));
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
            let lines = split_multiline_cards(card.text(), hands[&card.id].to_string());

            stream::iter(lines.into_iter().map(|(id, text)| {
                let res = InlineQueryResultArticle::new(
                    id,
                    if chat.pick == 1 {
                        InputMessageContentText::new("I've choosen my card")
                    } else {
                        InputMessageContentText::new(format!(
                            "I've choosen my {}° card",
                            played + 1
                        ))
                    },
                    text,
                );
                Ok::<_, DbErr>(InlineQueryResult::Article(if played + 1 < chat.pick {
                    res.with_reply_markup([[
                        InlineKeyboardButton::for_switch_inline_query_current_chat(
                            "Open cards hand",
                            chat.id.to_string(),
                        ),
                    ]])
                } else {
                    res
                }))
            }))
        })
        .try_flatten()
        .try_collect::<Vec<_>>()
        .await?;

    client
        .execute(AnswerInlineQuery::new(query_id, cards).with_cache_time(0))
        .await?;

    Ok(Ok(()))
}

/// split text in multiple lines if needed
/// official line limit is 127 chars
/// but text is trucated based on screen width
/// so we'll use 50 chars to better fit screen
///
/// Telegram doesn't accept multiple inputs with the same id
/// so we are appending a ";{index}"
fn split_multiline_cards(text: String, id: String) -> Vec<(String, String)> {
    let text_len = text.chars().count();
    if text_len > 50 {
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
    }
}
