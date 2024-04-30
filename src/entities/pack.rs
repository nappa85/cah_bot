use std::borrow::Cow;

use sea_orm::{entity::prelude::*, ActiveValue, QuerySelect, TransactionTrait};
use serde::Deserialize;

use crate::Error;

use super::{card, chat_pack};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "packs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    name: String,
    pub official: bool,
}

impl Model {
    pub fn name(&self) -> String {
        crate::utils::escape_markdown(&self.name)
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "card::Entity")]
    Card,
    #[sea_orm(has_many = "chat_pack::Entity")]
    Chat,
}

impl Related<card::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Card.def()
    }
}

impl Related<chat_pack::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Chat.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

pub async fn init<C>(conn: &C) -> Result<(), Error>
where
    C: ConnectionTrait + TransactionTrait,
{
    let packs = Entity::find()
        .select_only()
        .column_as(Column::Id.count(), "count")
        .into_tuple::<Option<i64>>()
        .one(conn)
        .await?;
    if packs.flatten().unwrap_or_default() > 0 {
        return Ok(());
    }

    println!("database init started");
    let txn = conn.begin().await?;
    let packs: Vec<Pack> = serde_json::from_str(crate::PACKS)?;
    for pack in packs {
        let model = ActiveModel {
            name: ActiveValue::Set(pack.name.into_owned()),
            official: ActiveValue::Set(pack.official),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        for card in pack.black {
            card::ActiveModel {
                color: ActiveValue::Set(card::Color::Black),
                pack_id: ActiveValue::Set(model.id),
                pick: ActiveValue::Set(card.pick),
                text: ActiveValue::Set(card.text.into_owned()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
        }
        for card in pack.white {
            card::ActiveModel {
                color: ActiveValue::Set(card::Color::White),
                pack_id: ActiveValue::Set(model.id),
                pick: ActiveValue::Set(card.pick),
                text: ActiveValue::Set(card.text.into_owned()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
        }
    }
    txn.commit().await?;
    println!("database init completed");

    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack<'a> {
    name: Cow<'a, str>,
    white: Vec<Card<'a>>,
    black: Vec<Card<'a>>,
    official: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Card<'a> {
    text: Cow<'a, str>,
    pick: Option<i32>,
    #[serde(rename = "pack")]
    _pack: i32,
}
