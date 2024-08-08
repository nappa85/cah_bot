use sea_orm::entity::prelude::*;

use super::{hand, pack};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "cards")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub pack_id: i32,
    pub color: Color,
    pub pick: Option<i32>,
    text: String,
}

impl Model {
    pub fn text(&self) -> String {
        crate::utils::escape_markdown(&self.text)
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "pack::Entity")]
    Pack,
    #[sea_orm(has_many = "hand::Entity")]
    Hand,
}

impl Related<pack::Entity> for Entity {
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
    pub fn pick(&self) -> i32 {
        self.pick.unwrap_or(1)
    }

    pub fn descr(&self) -> String {
        let mut descr = format!("*{}*", self.text());
        if let Some(pick) = self.pick {
            if pick > 1 {
                descr.push_str("\nPick ");
                descr.push_str(&pick.to_string());
            }
        }
        descr
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(5))")]
pub enum Color {
    #[sea_orm(string_value = "black")]
    Black,
    #[sea_orm(string_value = "white")]
    White,
}
