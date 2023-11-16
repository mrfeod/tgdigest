use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chrono::{DateTime, Local, Utc};
use futures_util::stream::StreamExt;
use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;
use log;
use partial_sort::PartialSort;
use simple_logger::SimpleLogger;
use std::fs;
use std::io::{self, BufRead as _, Write as _};
use tera::Tera;
use tokio::runtime;
use tokio::time::sleep;
use tokio::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SESSION_FILE: &str = "dialogs.session";

#[derive(Copy, Clone)]
enum ActionType {
    Replies = 0,
    Reactions,
    Forwards,
    Views,
}

impl ActionType {
    fn from(value: usize) -> ActionType {
        match value {
            0 => ActionType::Replies,
            1 => ActionType::Reactions,
            2 => ActionType::Forwards,
            3 => ActionType::Views,
            _ => panic!("No ActionType for {value}"),
        }
    }
}

#[derive(Copy, Clone, serde::Serialize)]
pub struct Post {
    date: i64,
    id: i32,
    views: i32,
    forwards: i32,
    replies: i32,
    reactions: i32,
}

impl Post {
    async fn get_by_date(
        messages: &mut grammers_client::client::messages::MessageIter,
        from_date: DateTime<Utc>,
        to_date: DateTime<Utc>,
    ) -> Result<Vec<Post>> {
        let mut posts: Vec<Post> = Vec::new();

        while let Some(message) = messages.next().await? {
            let message: grammers_client::types::Message = message;

            let date = DateTime::<Utc>::from_timestamp(message.date().timestamp(), 0).unwrap();
            if date > to_date {
                continue;
            }
            if date < from_date {
                break;
            }

            // let text = message.text().substring(0, 21);
            posts.push(Post {
                date: date.timestamp(),
                id: message.id(),
                views: message.view_count().unwrap_or(-1),
                forwards: message.forward_count().unwrap_or(-1),
                replies: message.reply_count().unwrap_or(-1),
                reactions: message.reaction_count().unwrap_or(-1),
            });
        }

        Result::Ok(posts)
    }

    fn count(&self, index: ActionType) -> i32 {
        match index {
            ActionType::Replies => self.replies,
            ActionType::Reactions => self.reactions,
            ActionType::Forwards => self.forwards,
            ActionType::Views => self.views,
        }
    }
}

#[derive(serde::Serialize)]
struct TopPost<const N: usize> {
    replies: Vec<Post>,
    reactions: Vec<Post>,
    forwards: Vec<Post>,
    views: Vec<Post>,
}

impl<const N: usize> TopPost<N> {
    fn get_top_by(posts: &mut Vec<Post>, action: ActionType) -> Vec<Post> {
        if posts.len() < N {
            panic!("Size of posts less than {N}")
        }

        posts.partial_sort(N, |a, b| b.count(action).cmp(&a.count(action)));
        posts[0..N].to_vec()
    }

    fn get_top(posts: &mut Vec<Post>) -> TopPost<N> {
        TopPost {
            replies: Self::get_top_by(posts, ActionType::Replies),
            reactions: Self::get_top_by(posts, ActionType::Reactions),
            forwards: Self::get_top_by(posts, ActionType::Forwards),
            views: Self::get_top_by(posts, ActionType::Views),
        }
    }

    fn index(&self, index: ActionType) -> &Vec<Post> {
        match index {
            ActionType::Replies => &self.replies,
            ActionType::Reactions => &self.reactions,
            ActionType::Forwards => &self.forwards,
            ActionType::Views => &self.views,
        }
    }

    fn print(&self) {
        let headers = [
            format!("Top {N} by comments:"),
            format!("Top {N} by reactions:"),
            format!("Top {N} by forwards:"),
            format!("Top {N} by views:"),
        ];
        for (index, header) in headers.iter().enumerate() {
            println!("{header}");
            let action = ActionType::from(index);
            for (pos, post) in self.index(action).iter().enumerate() {
                println!(
                    "\t{}. {}: {}\t({})",
                    pos + 1,
                    post.id,
                    post.count(action),
                    DateTime::<Utc>::from_timestamp(post.date, 0).unwrap()
                );
            }
            println!("");
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct Card {
    id: i32,
    header: String,
    icon: String,
    use_filter: bool,
    filter: String,
    count: i32,
}

impl Card {
    fn default() -> Self {
        Card {
            id: -1,
            header: String::from("UNDEFINED"),
            icon: icon_url("⚠️"),
            use_filter: false,
            filter: String::from(""),
            count: -1,
        }
    }

    fn create_card(post: &Post, action: ActionType) -> Card {
        Card {
            id: post.id,
            count: post.count(action),
            ..Card::default()
        }
    }

    fn create_cards(posts: &Vec<Post>, action: ActionType) -> Vec<Card> {
        posts.iter().map(|p| Card::create_card(p, action)).collect()
    }
}

#[derive(Clone, serde::Serialize)]
struct Block {
    header: String,
    icon: String,
    use_filter: bool,
    filter: String,
    cards: Vec<Card>,
}

impl Block {
    fn default() -> Self {
        Block {
            header: String::from("UNDEFINED"),
            icon: icon_url("⚠️"),
            use_filter: false,
            filter: String::from(""),
            cards: vec![],
        }
    }
}

fn icon_url(icon: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/googlefonts/noto-emoji/main/svg/emoji_u{}.svg",
        format!("{:04x}", icon.chars().nth(0).unwrap_or('❌') as u32)
    )
}

fn prompt(message: &str) -> Result<String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}

async fn async_main() -> Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let api_id: i32 = std::env::var("TG_ID")
        .expect("TG_ID is not set")
        .parse()
        .expect("TG_ID is not i32");
    let api_hash = std::env::var("TG_HASH").expect("TG_HASH is not set");

    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id,
        api_hash: api_hash.clone(),
        params: Default::default(),
    })
    .await?;
    println!("Connected!");

    let channel_name = "ithueti";

    let card_post_index: [usize; 4] = [0, 0, 0, 0];
    let editor_choice_post_id = 10894;

    // If we can't save the session, sign out once we're done.
    let mut sign_out = false;

    if !client.is_authorized().await? {
        println!("Signing in...");
        let phone = prompt("Enter your phone number (international format): ")?;
        let token = client.request_login_code(&phone).await?;
        let code = prompt("Enter the code you received: ")?;
        let signed_in = client.sign_in(&token, &code).await;
        match signed_in {
            Err(SignInError::PasswordRequired(password_token)) => {
                // Note: this `prompt` method will echo the password in the console.
                //       Real code might want to use a better way to handle this.
                let hint = password_token.hint().unwrap_or("None");
                let prompt_message = format!("Enter the password (hint {}): ", &hint);
                let password = prompt(prompt_message.as_str())?;

                client
                    .check_password(password_token, password.trim())
                    .await?;
            }
            Ok(_) => (),
            Err(e) => panic!("{}", e),
        };
        println!("Signed in!");
        match client.session().save_to_file(SESSION_FILE) {
            Ok(_) => {}
            Err(e) => {
                println!(
                    "NOTE: failed to save the session, will sign out when done: {}",
                    e
                );
                sign_out = true;
            }
        }
    }

    let working_dir = std::env::current_dir()?;

    // Obtain a `ClientHandle` to perform remote calls while `Client` drives the connection.
    //
    // This handle can be `clone()`'d around and freely moved into other tasks, so you can invoke
    // methods concurrently if you need to. While you do this, the single owned `client` is the
    // one that communicates with the network.
    //
    // The design's annoying to use for trivial sequential tasks, but is otherwise scalable.
    let client_handle = client.clone();

    let ithueti: grammers_client::types::chat::Chat =
        client_handle.resolve_username(channel_name).await?.unwrap();

    let photo = ithueti.photo_downloadable(true);
    match photo {
        Some(photo) => {
            let photo_out = working_dir.join("pic.png");
            println!("Pic {}", photo_out.to_str().unwrap());
            client_handle.download_media(&photo, photo_out).await?;
        }
        _ => {}
    }

    let mut messages = client_handle.iter_messages(ithueti).limit(500);
    let current_date = DateTime::<Utc>::from_timestamp(Local::now().timestamp(), 0).unwrap();
    let week_ago = current_date - chrono::Duration::days(8);
    println!("Now {current_date}");
    println!("8 days ago {week_ago}");

    let mut posts = Post::get_by_date(&mut messages, week_ago, current_date).await?;

    let post_top = TopPost::<3>::get_top(&mut posts);
    post_top.print();

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
            use_filter: true,
            filter: String::from("filter-blue"),
            cards: Card::create_cards(get_posts(ActionType::Forwards), ActionType::Forwards),
        },
        Block {
            header: String::from("По просмотрам"),
            icon: icon_url("👁️"),
            use_filter: true,
            filter: String::from("filter-blue"),
            cards: Card::create_cards(get_posts(ActionType::Views), ActionType::Views),
        },
    ];

    let get_post = |action: ActionType| &post_top.index(action)[card_post_index[action as usize]];
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
            use_filter: true,
            filter: String::from("filter-blue"),
            ..Card::create_card(get_post(ActionType::Forwards), ActionType::Forwards)
        },
        Card {
            header: String::from("Лучший по просмотрам"),
            icon: icon_url("👁️"),
            use_filter: true,
            filter: String::from("filter-blue"),
            ..Card::create_card(get_post(ActionType::Views), ActionType::Views)
        },
    ];

    // Template part

    // Digest rendering

    let mut tera = Tera::default();

    let digest_template = working_dir.join("digest_template.html");
    tera.add_template_file(digest_template, Some("digest.html"))
        .unwrap();

    let mut digest_context = tera::Context::new();
    digest_context.insert("blocks", &blocks);
    digest_context.insert("editor_choice_id", &editor_choice_post_id);
    digest_context.insert("channel_name", &channel_name);

    let rendered = tera.render("digest.html", &digest_context).unwrap();

    let mut file = fs::File::create("digest.html")?;
    file.write_all(rendered.as_bytes())?;

    // Card rendering

    let render_template = working_dir.join("render_template.html");
    tera.add_template_file(render_template, Some("render.html"))
        .unwrap();

    let mut render_context = tera::Context::new();
    render_context.insert("cards", &cards);
    render_context.insert("editor_choice_id", &editor_choice_post_id);
    render_context.insert("channel_name", &channel_name);

    let rendered = tera.render("render.html", &render_context).unwrap();

    let mut file = fs::File::create("render.html")?;
    file.write_all(rendered.as_bytes())?;

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
            .with_head()
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
    let render_page_path = working_dir.join("render.html");
    let render_page = render_page_path.to_str().unwrap();
    let page = browser.new_page(format!("file://{render_page}")).await?;

    sleep(Duration::from_secs(3)).await;

    // find the search bar type into the search field and hit `Enter`,
    // this triggers a new navigation to the search result page
    let cards = page.find_elements("div").await?;

    // page.bring_to_front().await?;
    for (i, card) in cards.iter().enumerate() {
        // card.focus().await?.scroll_into_view().await?;
        let _ = card
            .save_screenshot(CaptureScreenshotFormat::Png, format!("card_{:02}.png", i))
            .await?;
    }

    browser.close().await?;
    let _ = handle.await;

    // End

    if sign_out {
        // TODO revisit examples and get rid of "handle references" (also, this panics)
        drop(client_handle.sign_out_disconnect().await);
    }

    Ok(())
}

fn main() -> Result<()> {
    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}
