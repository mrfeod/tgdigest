use crate::action::ActionType;
use crate::post::TopPost;
use crate::task::*;
use crate::util::*;
use crate::workers::block::Block;
use crate::workers::card::Card;

pub fn create_context(post_top: TopPost, task: Task, channel_title: &str) -> Result<RenderingContext> {
    log::debug!("Creating digest.html");
    let get_posts = |action: ActionType| post_top.index(action);
    let blocks = vec![
        Block {
            header: String::from("По комментариям"),
            icon: icon_url("💬"),
            cards: Card::create_cards(get_posts(ActionType::Replies), ActionType::Replies),
            ..Block::default()
        },
        Block {
            header: String::from("По реакциям"),
            icon: icon_url("👏"),
            cards: Card::create_cards(get_posts(ActionType::Reactions), ActionType::Reactions),
            ..Block::default()
        },
        Block {
            header: String::from("По репостам"),
            icon: icon_url("🔁"),
            filter: String::from("filter-blue"),
            cards: Card::create_cards(get_posts(ActionType::Forwards), ActionType::Forwards),
        },
        Block {
            header: String::from("По просмотрам"),
            icon: icon_url("👁️"),
            filter: String::from("filter-blue"),
            cards: Card::create_cards(get_posts(ActionType::Views), ActionType::Views),
        },
    ]
    .into_iter()
    .filter(|b| b.cards.is_some())
    .collect::<Vec<Block>>();

    let mut context = RenderingContext::new();
    context.insert("blocks", &blocks);
    context.insert("editor_choice_id", &task.editor_choice_post_id);
    context.insert("channel_name", &task.channel_name.as_str());
    context.insert("channel_title", channel_title);
    context.insert("userpic_url", &format!("/userpic/{}", &task.channel_name));

    Ok(context)
}
