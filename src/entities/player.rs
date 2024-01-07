use sea_orm::{entity::prelude::*, ActiveValue, QuerySelect};

use super::{chat, hand};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "players")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub telegram_id: i64,
    pub chat_id: i32,
    pub name: String,
    pub turn: i32,
    pub points: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "chat::Entity")]
    Chat,
    #[sea_orm(has_many = "hand::Entity")]
    Hand,
}

impl Related<chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Chat.def()
    }
}

impl Related<hand::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Hand.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn is_my_turn(&self, chat: &chat::Model) -> bool {
        let mut turn = chat.turn % chat.players;
        if turn == 0 {
            turn = chat.players;
        }
        self.turn == turn
    }

    pub fn tg_link(&self) -> String {
        format!("[{}](tg://user?id={})", self.name, self.telegram_id)
    }
}

pub async fn insert<C: ConnectionTrait>(
    conn: &C,
    telegram_id: impl Into<i64>,
    chat_id: i32,
    name: String,
) -> Result<Model, DbErr> {
    let turn = Entity::find()
        .filter(Column::ChatId.eq(chat_id))
        .select_only()
        .column_as(Column::Turn.max(), "turn")
        .into_tuple::<Option<i32>>()
        .one(conn)
        .await?
        .flatten()
        .unwrap_or_default();

    ActiveModel {
        telegram_id: ActiveValue::Set(telegram_id.into()),
        chat_id: ActiveValue::Set(chat_id),
        name: ActiveValue::Set(name),
        turn: ActiveValue::Set(turn + 1),
        ..Default::default()
    }
    .insert(conn)
    .await
}
