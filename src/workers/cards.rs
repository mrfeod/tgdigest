use crate::action::ActionType;
use crate::post::Post;
use crate::post::TopPost;
use crate::task::Task;
use crate::util::*;
use crate::workers::card::Card;
use crate::Commands::Cards;

pub fn create_context(post_top: TopPost, task: Task) -> Result<RenderingContext> {
    let card_post_index = match task.command {
        Cards {
            replies,
            reactions,
            forwards,
            views,
        } => [replies, reactions, forwards, views],
        _ => panic!("Wrong command"),
    };

    let out_of_range = card_post_index
        .iter()
        .filter(|&x| match x {
            Some(x) => x < &1 || x > &post_top.top_count,
            None => false,
        })
        .count()
        > 0;
    if out_of_range {
        return Err(format!(
            "Index of replies/reactions/forwards/views out of range [1;{}]",
            post_top.top_count
        )
        .into());
    }

    let get_post = |action: ActionType| match card_post_index[action as usize] {
        Some(index) => Some(&post_top.index(action)[index - 1]),
        None => None,
    };
    let cards = vec![
        Card {
            header: String::from("Лучший по комментариям"),
            icon: icon_url("💬"),
            ..Card::create_card(get_post(ActionType::Replies), ActionType::Replies)
        },
        Card {
            header: String::from("Лучший по реакциям"),
            icon: icon_url("👏"),
            ..Card::create_card(get_post(ActionType::Reactions), ActionType::Reactions)
        },
        Card {
            header: String::from("Лучший по репостам"),
            icon: icon_url("🔁"),
            filter: String::from("filter-blue"),
            ..Card::create_card(get_post(ActionType::Forwards), ActionType::Forwards)
        },
        Card {
            header: String::from("Лучший по просмотрам"),
            icon: icon_url("👁️"),
            filter: String::from("filter-blue"),
            ..Card::create_card(get_post(ActionType::Views), ActionType::Views)
        },
    ];
    let cards: Vec<Card> = cards.into_iter().filter(|c| c.count.is_some()).collect();

    let mut context = RenderingContext::new();
    context.insert("cards", &cards);
    context.insert("editor_choice_id", &task.editor_choice_post_id);
    context.insert("channel_name", &task.channel_name.as_str());

    Ok(context)
}

pub fn create_post_context(post: Post, task: Task) -> Result<RenderingContext> {
    let mut context = RenderingContext::new();
    context.insert("id", &post.id);
    context.insert("date", &post.date);
    context.insert("message", &post.message);
    context.insert("image", &post.image);
    context.insert("replies", &post.replies);
    context.insert("reactions", &post.reactions);
    context.insert("forwards", &post.forwards);
    context.insert("views", &post.views);
    context.insert("channel_name", &task.channel_name.as_str());

    Ok(context)
}
