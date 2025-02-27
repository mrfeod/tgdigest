use crate::action::ActionType;
use crate::post::Post;
use crate::util;

#[derive(Clone, serde::Serialize)]
pub struct Card {
    pub id: i32,
    pub count: Option<i32>,
    pub header: String,
    pub icon: String,
    pub filter: String,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            id: -1,
            count: None,
            header: String::from("UNDEFINED"),
            icon: util::icon_url("⚠️"),
            filter: String::from(""),
        }
    }
}

impl Card {
    pub fn create_card(post: Option<&Post>, action: ActionType) -> Card {
        post.map(|post| Card {
            id: post.id,
            count: post.count(action),
            ..Card::default()
        })
        .unwrap_or_default()
    }

    pub fn create_cards(posts: &[Post], action: ActionType) -> Option<Vec<Card>> {
        let cards = posts
            .iter()
            .map(|p| Card::create_card(Some(p), action))
            .filter(|c| c.count.is_some())
            .collect::<Vec<Card>>();

        (!cards.is_empty()).then_some(cards)
    }
}
