use crate::util;
use crate::workers::card::Card;

#[derive(Clone, serde::Serialize)]
pub struct Block {
    pub header: String,
    pub icon: String,
    pub filter: String,
    pub cards: Option<Vec<Card>>,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            header: String::from("UNDEFINED"),
            icon: util::icon_url("⚠️"),
            filter: String::from(""),
            cards: None,
        }
    }
}
