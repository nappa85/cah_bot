use chrono::{NaiveDateTime, Utc};
use sea_orm::{entity::prelude::*, ActiveValue, TransactionTrait};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chats")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub telegram_id: i64,
    pub start_date: NaiveDateTime,
    pub end_date: Option<NaiveDateTime>,
    pub players: i32,
    pub turn: i32,
    pub rando_carlissian: bool,
    pub pick: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::player::Entity")]
    Player,
    #[sea_orm(has_many = "super::chat_pack::Entity")]
    Pack,
    #[sea_orm(has_many = "super::hand::Entity")]
    Hand,
}

impl Related<super::player::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Player.def()
    }
}

impl Related<super::chat_pack::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Pack.def()
    }
}

impl Related<super::hand::Entity> for Entity {
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
