use crate::util;
use crate::workers::card::Card;

#[derive(Clone, serde::Serialize)]
pub struct Block {
    pub header: String,
    pub icon: String,
    pub filter: String,
    pub cards: Option<Vec<Card>>,
}

impl Block {
    pub fn default() -> Self {
        Block {
            header: String::from("UNDEFINED"),
            icon: util::icon_url("⚠️"),
            filter: String::from(""),
            cards: None,
        }
    }
}
