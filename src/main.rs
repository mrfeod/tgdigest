use build_html::*;
use chrono::{Local, NaiveDateTime};
use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;
use log;
use partial_sort::PartialSort;
use simple_logger::SimpleLogger;
use std::env;
use std::fs;
use std::io::{self, BufRead as _, Write as _};
use substring::Substring;
use tokio::runtime;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SESSION_FILE: &str = "dialogs.session";

const HTML_STYLE: &str = r#"
    @import url("https://fonts.googleapis.com/css?family=Tenor+Sans&display=swap");
    a:link {
        color: black;
        background-color: transparent;
        text-decoration: none;
    }
    a:visited {
        color: black;
        background-color: transparent;
        text-decoration: none;
    }
    div {
        width: 500px;
        max-width: 500px;
        border:3px hidden;
    }
    * {
        font-family: Tenor Sans;
    }"#;

const HTML_HEADER: &str = "<h1><a href=digest.html>Айти Тудэй Дайджест</a></h1>";

fn widget(post_id: i32) -> String {
    format!(
        "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
        post_id
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

    let api_id = env!("TG_ID").parse().expect("TG_ID invalid");
    let api_hash = env!("TG_HASH").to_string();

    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id,
        api_hash: api_hash.clone(),
        params: Default::default(),
    })
    .await?;
    println!("Connected!");

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

    // Obtain a `ClientHandle` to perform remote calls while `Client` drives the connection.
    //
    // This handle can be `clone()`'d around and freely moved into other tasks, so you can invoke
    // methods concurrently if you need to. While you do this, the single owned `client` is the
    // one that communicates with the network.
    //
    // The design's annoying to use for trivial sequential tasks, but is otherwise scalable.
    let client_handle = client.clone();

    let ithueti = client_handle.resolve_username("ithueti").await?.unwrap();
    let mut messages = client_handle.iter_messages(ithueti).limit(500);
    let current_date = Local::now().naive_utc();
    println!("Now {current_date}");

    #[derive(Copy, Clone)]
    pub struct Post {
        date: NaiveDateTime,
        id: i32,
        views: i32,
        forwards: i32,
        replies: i32,
        reactions: i32,
    }
    let mut posts: Vec<Post> = Vec::new();

    while let Some(message) = messages.next().await? {
        let message: grammers_client::types::Message = message;

        let date: NaiveDateTime = message.date().naive_utc();
        let diff = current_date - date;
        if diff.num_days() > 7 {
            break;
        }

        let text = message.text().substring(0, 21);
        println!(
            "{} day(s) ago: id = {}; views = {}; forwards = {}; replies = {}; reactions = {}; text = {}; ",
            diff.num_days(),
            message.id(),
            message.view_count().unwrap_or(-1),
            message.forward_count().unwrap_or(-1),
            message.reply_count().unwrap_or(-1),
            message.reaction_count().unwrap_or(-1),
            text.replace('\n', " ")
        );
        posts.push(Post {
            date: date,
            id: message.id(),
            views: message.view_count().unwrap_or(-1),
            forwards: message.forward_count().unwrap_or(-1),
            replies: message.reply_count().unwrap_or(-1),
            reactions: message.reaction_count().unwrap_or(-1),
        });
    }

    posts.partial_sort(3, |a, b| b.replies.cmp(&a.replies));
    let replies = vec![posts[0], posts[1], posts[2]];

    posts.partial_sort(3, |a, b| b.reactions.cmp(&a.reactions));
    let reactions = vec![posts[0], posts[1], posts[2]];

    posts.partial_sort(3, |a, b| b.forwards.cmp(&a.forwards));
    let forwards = vec![posts[0], posts[1], posts[2]];

    posts.partial_sort(3, |a, b| b.views.cmp(&a.views));
    let views = vec![posts[0], posts[1], posts[2]];

    println!("Top 3 by comments:");
    for (pos, e) in replies.iter().enumerate() {
        println!("\t{}. {}: {}\t({})", pos + 1, e.id, e.reactions, e.date);
    }
    println!("");
    println!("Top 3 by reactions:");
    for (pos, e) in reactions.iter().enumerate() {
        println!("\t{}. {}: {}\t({})", pos + 1, e.id, e.reactions, e.date);
    }
    println!("");
    println!("Top 3 by forwards:");
    for (pos, e) in forwards.iter().enumerate() {
        println!("\t{}. {}: {}\t({})", pos + 1, e.id, e.reactions, e.date);
    }
    println!("");
    println!("Top 3 by views:");
    for (pos, e) in views.iter().enumerate() {
        println!("\t{}. {}: {}\t({})", pos + 1, e.id, e.reactions, e.date);
    }
    println!("");

    fn base_page(title: &str) -> HtmlPage {
        HtmlPage::new()
            .with_head_link(
                "https://static.tildacdn.com/tild6337-3861-4463-a331-313361323738/icon32.png",
                "icon",
            )
            .with_meta(vec![("charset", "UTF-8")])
            .with_style(HTML_STYLE)
            .with_title("Айти Тудэй Дайджест")
            .with_raw(HTML_HEADER)
    }

    fn generate_page<F>(posts: &Vec<Post>, header: &str, icon: &str, count: F) -> String
    where
        F: Fn(&Post) -> i32,
    {
        base_page(format!("{header}: Айти Тудэй Дайджест").as_str())
            .with_header(2, format!("{header} {icon}"))
            .with_header(3, format!("1. {icon}{}", count(&posts[0])))
            .with_raw(widget(posts[0].id))
            .with_header(3, format!("2. {icon}{}", count(&posts[1])))
            .with_raw(widget(posts[1].id))
            .with_header(3, format!("3. {icon}{}", count(&posts[2])))
            .with_raw(widget(posts[2].id))
            .to_html_string()
    }

    let by_replies: String = generate_page(
        &replies,
        "По комментариям",
        "💬",
        |post: &Post| post.replies,
    );
    let by_reactions: String =
        generate_page(&reactions, "По реакциям", "♥", |post: &Post| {
            post.reactions
        });
    let by_reposts: String =
        generate_page(&forwards, "По репостам", "🔁", |post: &Post| {
            post.forwards
        });
    let by_views: String = generate_page(&views, "По просмотрам", "👁", |post: &Post| post.views);
    let digest: String = base_page("Айти Тудэй Дайджест")
        .with_raw("<h2><a href=replies.html>По комментариям 💬</a></h2>")
        .with_raw("<h2><a href=reactions.html>По реакциям ♥</a></h2>")
        .with_raw("<h2><a href=reposts.html>По репостам 🔁</a></h2>")
        .with_raw("<h2><a href=views.html>По просмотрам 👁</a></h2>")
        .to_html_string();

    let mut file = fs::File::create("digest.html")?;
    file.write_all(digest.as_bytes())?;

    let mut file = fs::File::create("replies.html")?;
    file.write_all(by_replies.as_bytes())?;

    let mut file = fs::File::create("reactions.html")?;
    file.write_all(by_reactions.as_bytes())?;

    let mut file = fs::File::create("reposts.html")?;
    file.write_all(by_reposts.as_bytes())?;

    let mut file = fs::File::create("views.html")?;
    file.write_all(by_views.as_bytes())?;

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
