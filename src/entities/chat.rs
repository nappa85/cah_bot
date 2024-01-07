use chrono::{NaiveDateTime, Utc};
use sea_orm::{entity::prelude::*, ActiveValue, DatabaseTransaction, TransactionTrait};

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

    pub async fn reset(&self, txn: &DatabaseTransaction) -> Result<String, Error> {
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
            if let Some(card) =
                hand::draw(txn, player.id, self.id, self.turn, player.is_my_turn(self)).await?
            {
                black_card = Some((player, card));
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
                    hand::draw(txn, 0, self.id, self.turn, false).await?;
                }
            }

            format!(
                "Turn {}\n\n{}\n\nJudge is {}",
                self.turn,
                card.descr(),
                judge.tg_link(),
            )
        } else {
            String::from("Error: no black card picked")
        })
    }
}

pub async fn find_or_insert<C>(conn: &C, telegram_id: impl Into<i64>) -> Result<Model, DbErr>
where
    C: ConnectionTrait + TransactionTrait,
{
    let telegram_id = telegram_id.into();
    let chat = Entity::find()
        .filter(
            Column::TelegramId
                .eq(telegram_id)
                .and(Column::EndDate.is_null()),
        )
        .one(conn)
        .await?;
    if let Some(c) = chat {
        return Ok(c);
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

    Ok(chat)
}
