use rand::{distributions::Uniform, seq::index::sample, Rng};
use sea_orm::{entity::prelude::*, ActiveValue, QuerySelect};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "hands")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub player_id: i32,
    pub chat_id: i32,
    pub card_id: i32,
    pub picked_on_turn: i32,
    pub played_on_turn: Option<i32>,
    pub seq: i32,
    pub won: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::card::Entity")]
    Card,
    #[sea_orm(has_many = "super::player::Entity")]
    Player,
    #[sea_orm(has_many = "super::chat::Entity")]
    Chat,
}

impl Related<super::card::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Card.def()
    }
}

impl Related<super::player::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Player.def()
    }
}

impl Related<super::chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Chat.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(Some(5))")]
pub enum Color {
    #[sea_orm(string_value = "black")]
    Black,
    #[sea_orm(string_value = "white")]
    White,
}

pub async fn draw<C>(
    conn: &C,
    player_id: i32,
    chat_id: i32,
    turn: i32,
    draw_black: bool,
) -> Result<Option<super::card::Model>, crate::Error>
where
    C: ConnectionTrait,
{
    let already_picked = Entity::find()
        .filter(Column::ChatId.eq(chat_id))
        .select_only()
        .column(Column::CardId)
        .into_tuple::<i32>()
        .all(conn)
        .await?;
    let black_cards = if draw_black {
        Some(
            super::card::Entity::find()
                .filter(
                    super::card::Column::Color
                        .eq(super::card::Color::Black)
                        .and(super::card::Column::Id.is_not_in(already_picked.clone())),
                )
                .all(conn)
                .await?,
        )
    } else {
        None
    };
    let white_cards = super::card::Entity::find()
        .filter(
            super::card::Column::Color
                .eq(super::card::Color::White)
                .and(super::card::Column::Id.is_not_in(already_picked)),
        )
        .select_only()
        .column(super::card::Column::Id)
        .into_tuple::<i32>()
        .all(conn)
        .await?;

    // rando carlissian hack
    let player_cards = if player_id > 0 {
        Entity::find()
            .filter(
                Column::PlayerId
                    .eq(player_id)
                    .and(Column::PlayedOnTurn.is_null()),
            )
            .select_only()
            .column_as(Column::Id.count(), "ids")
            .into_tuple::<Option<i64>>()
            .one(conn)
            .await?
            .flatten()
            .unwrap_or_default()
    } else {
        9
    };

    let mut rng = rand::thread_rng();

    let res = if let Some(mut black_cards) = black_cards {
        let card_index = rng.sample(Uniform::new(0, black_cards.len()));
        let black_card = black_cards.remove(card_index);
        ActiveModel {
            player_id: ActiveValue::Set(player_id),
            chat_id: ActiveValue::Set(chat_id),
            card_id: ActiveValue::Set(black_card.id),
            picked_on_turn: ActiveValue::Set(turn),
            played_on_turn: ActiveValue::Set(Some(turn)),
            ..Default::default()
        }
        .insert(conn)
        .await?;
        Some(black_card)
    } else {
        None
    };

    if player_cards < 10 {
        let card_indexes = sample(&mut rng, white_cards.len(), 10 - player_cards as usize);
        for card_index in card_indexes {
            ActiveModel {
                player_id: ActiveValue::Set(player_id),
                chat_id: ActiveValue::Set(chat_id),
                card_id: ActiveValue::Set(white_cards[card_index]),
                picked_on_turn: ActiveValue::Set(turn),
                played_on_turn: ActiveValue::Set((player_id == 0).then_some(turn)), // rando carlissian hack
                ..Default::default()
            }
            .insert(conn)
            .await?;
        }
    }

    Ok(res)
}
