use sea_orm::{entity::prelude::*, ActiveValue, QuerySelect};

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
    #[sea_orm(has_many = "super::chat::Entity")]
    Chat,
    #[sea_orm(has_many = "super::hand::Entity")]
    Hand,
}

impl Related<super::chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Chat.def()
    }
}

impl Related<super::hand::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Hand.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn is_my_turn(&self, chat: &super::chat::Model) -> bool {
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

// #[cfg(test)]
// pub mod tests {
//     use chrono::Utc;
//     use sea_orm::{DbBackend, EntityTrait, MockDatabase};

//     pub fn mock_player() -> [super::Model; 1] {
//         [super::Model {
//             id: 1,
//             telegram_id: 1,
//             chat_id: 1,
//             name: String::from("pippo"),
//         }]
//     }

//     #[tokio::test]
//     async fn score() {
//         // queries must be in the order they are executed
//         let conn: sea_orm::DatabaseConnection = MockDatabase::new(DbBackend::Sqlite)
//             .append_query_results([mock_player()])
//             .append_query_results([crate::entities::team::tests::mock_team()])
//             .append_query_results([crate::entities::position::tests::mock_positions()])
//             .into_connection();

//         let player = super::Entity::find_by_id(1)
//             .one(&conn)
//             .await
//             .unwrap()
//             .unwrap();
//         let score = player.score(&conn, Utc::now().naive_utc()).await.unwrap();
//         assert_eq!(score, 9);
//     }

//     #[tokio::test]
//     async fn empty_score() {
//         // queries must be in the order they are executed
//         let conn: sea_orm::DatabaseConnection = MockDatabase::new(DbBackend::Sqlite)
//             .append_query_results([mock_player()])
//             .append_query_results::<crate::entities::team::Model, _, _>([[]])
//             .into_connection();

//         let player = super::Entity::find_by_id(1)
//             .one(&conn)
//             .await
//             .unwrap()
//             .unwrap();
//         let score = player.score(&conn, Utc::now().naive_utc()).await.unwrap();
//         assert_eq!(score, 0);
//     }
// }
