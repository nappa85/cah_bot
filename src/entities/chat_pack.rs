use sea_orm::{entity::prelude::*, ActiveValue};

use super::{chat, pack};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chat_packs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub chat_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub pack_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "chat::Entity")]
    Chat,
    #[sea_orm(has_one = "pack::Entity")]
    Pack,
}

impl Related<chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Chat.def()
    }
}

impl Related<pack::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Pack.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

pub async fn init<C: ConnectionTrait>(conn: &C, chat_id: i32) -> Result<(), DbErr> {
    let packs = pack::Entity::find().all(conn).await?;

    for pack in packs {
        ActiveModel {
            chat_id: ActiveValue::Set(chat_id),
            pack_id: ActiveValue::Set(pack.id),
        }
        .insert(conn)
        .await?;
    }
    Ok(())
}
