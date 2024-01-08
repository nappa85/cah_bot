use std::borrow::Cow;

use chrono::{NaiveDateTime, Utc};
use futures_util::TryStreamExt;
use sea_orm::{
    entity::prelude::*, ActiveValue, DatabaseTransaction, QueryOrder, QuerySelect, StreamTrait,
    TransactionTrait,
};
use tgbot::types::Chat;

use crate::Error;

use super::{chat_pack, hand, player};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chats")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub telegram_id: i64,
    pub owner: Option<i32>,
    pub start_date: NaiveDateTime,
    pub end_date: Option<NaiveDateTime>,
    pub players: i32,
    pub turn: i32,
    pub rando_carlissian: bool,
    pub pick: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "player::Entity")]
    Player,
    #[sea_orm(has_many = "chat_pack::Entity")]
    Pack,
    #[sea_orm(has_many = "hand::Entity")]
    Hand,
}

impl Related<player::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Player.def()
    }
}

impl Related<chat_pack::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Pack.def()
    }
}

impl Related<hand::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Hand.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn next_player_turn(&self) -> i32 {
        let turn = self.turn % self.players;
        if turn == 0 {
            self.players
        } else {
            turn
        }
    }

    pub async fn reset(
        &self,
        txn: &DatabaseTransaction,
    ) -> Result<Result<String, ChatError>, Error> {
        let hands = hand::Entity::find()
            .filter(
                hand::Column::ChatId.eq(self.id).and(
                    hand::Column::PickedOnTurn
                        .eq(self.turn)
                        .or(hand::Column::PlayedOnTurn.eq(self.turn)),
                ),
            )
            .all(txn)
            .await?;
        for hand in hands {
            if hand.picked_on_turn == self.turn {
                hand::ActiveModel {
                    id: ActiveValue::Set(hand.id),
                    ..Default::default()
                }
                .delete(txn)
                .await?;
            } else if hand.played_on_turn == Some(self.turn) {
                hand::ActiveModel {
                    id: ActiveValue::Set(hand.id),
                    played_on_turn: ActiveValue::Set(None),
                    ..Default::default()
                }
                .update(txn)
                .await?;
            }
        }

        let players = player::Entity::find()
            .filter(player::Column::ChatId.eq(self.id))
            .all(txn)
            .await?;
        let mut black_card = None;
        for player in players {
            let pick_black = player.is_my_turn(self);

            match hand::pick(txn, player.id, self.id, self.turn, pick_black).await? {
                Ok(Some(card)) => {
                    black_card = Some((player, card));
                }
                Ok(None) => {}
                Err(e) => return Ok(Err(ChatError::from(e))),
            }
        }

        Ok(if let Some((judge, card)) = black_card {
            let pick = card.pick();
            ActiveModel {
                id: ActiveValue::Set(self.id),
                pick: ActiveValue::Set(pick),
                ..Default::default()
            }
            .update(txn)
            .await?;

            if self.rando_carlissian {
                for _ in 0..pick {
                    if let Err(e) = hand::pick(txn, 0, self.id, self.turn, false).await? {
                        return Ok(Err(ChatError::from(e)));
                    }
                }
            }

            Ok(format!(
                "Turn {}\n\n{}\n\nJudge is {}",
                self.turn,
                card.descr(),
                judge.tg_link(),
            ))
        } else {
            Err(ChatError::NoBlackCard)
        })
    }

    pub async fn close<C>(&self, conn: &C) -> Result<Result<String, ChatError>, Error>
    where
        C: ConnectionTrait + StreamTrait,
    {
        let stream = player::Entity::find()
            .filter(
                player::Column::ChatId
                    .eq(self.id)
                    .and(player::Column::Points.gt(0)),
            )
            .order_by_desc(player::Column::Points)
            .stream(conn)
            .await?;

        let mut players = stream
            .map_ok(|player| (player.points as i64, Cow::Owned(player.tg_link())))
            .try_collect::<Vec<_>>()
            .await?;

        if self.rando_carlissian {
            let won = hand::Entity::find()
                .filter(
                    hand::Column::ChatId
                        .eq(self.id)
                        .and(hand::Column::PlayerId.eq(0))
                        .and(hand::Column::Won.eq(true)),
                )
                .select_only()
                .column_as(hand::Column::Id.count(), "ids")
                .into_tuple::<Option<i64>>()
                .one(conn)
                .await?
                .flatten()
                .unwrap_or_default();
            if won > 0 {
                players.push((won, Cow::Borrowed(crate::RANDO_CARLISSIAN)));
                players.sort_by(|(points_a, _), (points_b, _)| points_b.cmp(points_a));
            }
        }

        if players.is_empty() {
            return Ok(Err(ChatError::Empty));
        }

        ActiveModel {
            id: ActiveValue::Set(self.id),
            end_date: ActiveValue::Set(Some(Utc::now().naive_utc())),
            ..Default::default()
        }
        .update(conn)
        .await?;

        let winner_points = players[0].0;
        let winners = players
            .into_iter()
            .map_while(|(points, player)| (points == winner_points).then_some(player))
            .collect::<Vec<_>>();

        Ok(Ok(format!(
            "After {} turns the winner{} {} with {} points{}",
            self.turn - 1,
            if winners.len() > 1 { "s are" } else { " is" },
            winners.join(" and "),
            winner_points,
            if winners.len() == 1 && winners[0] == crate::RANDO_CARLISSIAN {
                "\n\n*SHAME ON YOU!!!*"
            } else {
                ""
            }
        )))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ChatError {
    #[error("This bot doesn't works on channels")]
    Channel,
    #[error("This bot doesn't works on private chats")]
    Private,
    #[error("There seems to be no players in this game (this is probably a bug)")]
    Empty,
    #[error(transparent)]
    Pick(#[from] hand::PickError),
    #[error("No black card picked (this is a bug)")]
    NoBlackCard,
}

pub async fn find_or_insert<C>(conn: &C, tg_chat: &Chat) -> Result<Result<Model, ChatError>, DbErr>
where
    C: ConnectionTrait + TransactionTrait,
{
    let telegram_id = i64::from(tg_chat.get_id());
    let chat = Entity::find()
        .filter(
            Column::TelegramId
                .eq(telegram_id)
                .and(Column::EndDate.is_null()),
        )
        .one(conn)
        .await?;
    if let Some(c) = chat {
        return Ok(Ok(c));
    }

    match tg_chat {
        Chat::Channel(_) => return Ok(Err(ChatError::Channel)),
        Chat::Private(_) => return Ok(Err(ChatError::Private)),
        _ => {}
    }

    let txn = conn.begin().await?;

    let chat = ActiveModel {
        telegram_id: ActiveValue::Set(telegram_id),
        start_date: ActiveValue::Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    super::chat_pack::init(&txn, chat.id).await?;

    txn.commit().await?;

    Ok(Ok(chat))
}
