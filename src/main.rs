mod action;
mod cli;
mod context;
mod html_renderer;
mod path_util;
mod post;
mod task;
mod tg;
mod util;
mod workers;

use crate::action::*;
use crate::cli::*;
use crate::html_renderer::HtmlRenderer;
use crate::post::*;
use crate::task::*;
use crate::util::*;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use futures_util::stream::StreamExt;
use log;
use simple_logger::SimpleLogger;
use tokio::runtime;
use tokio::time::sleep;
use tokio::time::Duration;

#[derive(Clone, serde::Serialize)]
struct Card {
    id: i32,
    count: Option<i32>,
    header: String,
    icon: String,
    filter: String,
}

impl Card {
    fn default() -> Self {
        Card {
            id: -1,
            count: None,
            header: String::from("UNDEFINED"),
            icon: util::icon_url("⚠️"),
            filter: String::from(""),
        }
    }

    fn create_card(post: &Post, action: ActionType) -> Card {
        Card {
            id: post.id,
            count: post.count(action),
            ..Card::default()
        }
    }

    fn create_cards(posts: &Vec<Post>, action: ActionType) -> Option<Vec<Card>> {
        match posts
            .iter()
            .map(|p| Card::create_card(p, action))
            .filter(|c| c.count.is_some())
            .collect::<Vec<Card>>()
        {
            cards if !cards.is_empty() => Some(cards),
            _ => None,
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct Block {
    header: String,
    icon: String,
    filter: String,
    cards: Option<Vec<Card>>,
}

impl Block {
    fn default() -> Self {
        Block {
            header: String::from("UNDEFINED"),
            icon: util::icon_url("⚠️"),
            filter: String::from(""),
            cards: None,
        }
    }
}

async fn async_main() -> Result<()> {
    let cli = Args::parse_args();
    let ctx = context::AppContext::new(cli.config.clone())?;
    let task = Task::from_cli(cli);

    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let tg = tg::TelegramAPI::create(&ctx).await?;
    let client = tg.client();

    // LOAD PIC
    let channel_pic = workers::download_pic(&client, task.clone(), &ctx).await;
    match channel_pic {
        Ok(pic) => {
            println!("Downloaded pic: {}", pic.to_str().unwrap());
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    // GET MESSAGES
    let post_top = workers::get_top_posts(&client, task.clone()).await?;

    // Template part

    let html_renderer = HtmlRenderer::new(&ctx)?;

    match &task.command {
        Commands::Digest {} => {
            println!("Creating digest.html");
            let get_posts = |action: ActionType| post_top.index(action);
            let blocks = vec![
                Block {
                    header: String::from("По комментариям"),
                    icon: util::icon_url("💬"),
                    cards: Card::create_cards(get_posts(ActionType::Replies), ActionType::Replies),
                    ..Block::default()
                },
                Block {
                    header: String::from("По реакциям"),
                    icon: util::icon_url("👏"),
                    cards: Card::create_cards(
                        get_posts(ActionType::Reactions),
                        ActionType::Reactions,
                    ),
                    ..Block::default()
                },
                Block {
                    header: String::from("По репостам"),
                    icon: util::icon_url("🔁"),
                    filter: String::from("filter-blue"),
                    cards: Card::create_cards(
                        get_posts(ActionType::Forwards),
                        ActionType::Forwards,
                    ),
                },
                Block {
                    header: String::from("По просмотрам"),
                    icon: util::icon_url("👁️"),
                    filter: String::from("filter-blue"),
                    cards: Card::create_cards(get_posts(ActionType::Views), ActionType::Views),
                },
            ]
            .into_iter()
            .filter(|b| b.cards.is_some())
            .collect::<Vec<Block>>();

            // Digest rendering

            let mut digest_context = tera::Context::new();
            digest_context.insert("blocks", &blocks);
            digest_context.insert("editor_choice_id", &task.editor_choice_post_id);
            digest_context.insert("channel_name", &task.channel_name.as_str());

            let digest_file = html_renderer.render(
                format!("{}/digest_template.html", task.mode).as_str(),
                &digest_context,
            )?;
            print!("Digest file rendered: {}", digest_file.to_str().unwrap());
        }
        Commands::Cards {
            replies,
            reactions,
            forwards,
            views,
        } => {
            println!("Creating render.html and *.png cards");
            let card_post_index = [*replies - 1, *reactions - 1, *forwards - 1, *views - 1];

            let get_post =
                |action: ActionType| &post_top.index(action)[card_post_index[action as usize]];
            let cards = vec![
                Card {
                    header: String::from("Лучший по комментариям"),
                    icon: util::icon_url("💬"),
                    ..Card::create_card(get_post(ActionType::Replies), ActionType::Replies)
                },
                Card {
                    header: String::from("Лучший по реакциям"),
                    icon: util::icon_url("👏"),
                    ..Card::create_card(get_post(ActionType::Reactions), ActionType::Reactions)
                },
                Card {
                    header: String::from("Лучший по репостам"),
                    icon: util::icon_url("🔁"),
                    filter: String::from("filter-blue"),
                    ..Card::create_card(get_post(ActionType::Forwards), ActionType::Forwards)
                },
                Card {
                    header: String::from("Лучший по просмотрам"),
                    icon: util::icon_url("👁️"),
                    filter: String::from("filter-blue"),
                    ..Card::create_card(get_post(ActionType::Views), ActionType::Views)
                },
            ];
            let cards: Vec<Card> = cards.into_iter().filter(|c| c.count.is_some()).collect();

            // Card rendering

            let mut render_context = tera::Context::new();
            render_context.insert("cards", &cards);
            render_context.insert("editor_choice_id", &task.editor_choice_post_id);
            render_context.insert("channel_name", &task.channel_name.as_str());

            let render_file = html_renderer.render(
                format!("{}/render_template.html", task.mode).as_str(),
                &render_context,
            )?;
            print!("Render file rendered: {}", render_file.to_str().unwrap());

            // Browser part

            let viewport = chromiumoxide::handler::viewport::Viewport {
                width: 2000,
                height: 30000,
                device_scale_factor: Some(1.0),
                emulating_mobile: false,
                is_landscape: false,
                has_touch: false,
            };

            let (mut browser, mut handler) = Browser::launch(
                BrowserConfig::builder()
                    .window_size(2000, 30000)
                    .viewport(viewport)
                    .build()?,
            )
            .await?;

            // spawn a new task that continuously polls the handler
            let handle: tokio::task::JoinHandle<()> = tokio::task::spawn(async move {
                while let Some(h) = handler.next().await {
                    if h.is_err() {
                        break;
                    }
                }
            });

            // create a new browser page and navigate to the url
            let render_page = render_file.to_str().unwrap();
            let render_page_file = String::from("file://") + render_page;
            println!("Opening page for rendering: {render_page_file}");
            let page = browser.new_page(render_page_file).await?;

            // Wait to load? How much time is enough? Can we know the exact moment and wait synchronously?
            sleep(Duration::from_millis(100)).await;

            // find the search bar type into the search field and hit `Enter`,
            // this triggers a new navigation to the search result page
            let cards = page.find_elements("div").await?;

            // page.bring_to_front().await?;
            for (i, card) in cards.iter().enumerate() {
                let card_path = ctx.output_dir.join(format!("card_{:02}.png", i));
                let _ = card
                    .save_screenshot(CaptureScreenshotFormat::Png, &card_path)
                    .await?;
                println!("Generated: {}", card_path.to_str().unwrap());
            }

            browser.close().await?;
            let _ = handle.await;
        }
    }

    // End

    Ok(())
}

fn main() -> Result<()> {
    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}
