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
    let replies: [i32; 3] = [posts[0].id, posts[1].id, posts[2].id];
    println!("Top 3 by comments:");
    println!(
        "\t1. {}: {}\t({})",
        posts[0].id, posts[0].replies, posts[0].date
    );
    println!(
        "\t2. {}: {}\t({})",
        posts[1].id, posts[1].replies, posts[1].date
    );
    println!(
        "\t3. {}: {}\t({})",
        posts[2].id, posts[2].replies, posts[2].date
    );
    println!("");

    posts.partial_sort(3, |a, b| b.reactions.cmp(&a.reactions));
    let reactions: [i32; 3] = [posts[0].id, posts[1].id, posts[2].id];
    println!("Top 3 by reactions:");
    println!(
        "\t1. {}: {}\t({})",
        posts[0].id, posts[0].reactions, posts[0].date
    );
    println!(
        "\t2. {}: {}\t({})",
        posts[1].id, posts[1].reactions, posts[1].date
    );
    println!(
        "\t3. {}: {}\t({})",
        posts[2].id, posts[2].reactions, posts[2].date
    );
    println!("");

    posts.partial_sort(3, |a, b| b.forwards.cmp(&a.forwards));
    let forwards: [i32; 3] = [posts[0].id, posts[1].id, posts[2].id];
    println!("Top 3 by forwards:");
    println!(
        "\t1. {}: {}\t({})",
        posts[0].id, posts[0].forwards, posts[0].date
    );
    println!(
        "\t2. {}: {}\t({})",
        posts[1].id, posts[1].forwards, posts[1].date
    );
    println!(
        "\t3. {}: {}\t({})",
        posts[2].id, posts[2].forwards, posts[2].date
    );
    println!("");

    posts.partial_sort(3, |a, b| b.views.cmp(&a.views));
    let views: [i32; 3] = [posts[0].id, posts[1].id, posts[2].id];
    println!("Top 3 by views:");
    println!(
        "\t1. {}: {}\t({})",
        posts[0].id, posts[0].views, posts[0].date
    );
    println!(
        "\t2. {}: {}\t({})",
        posts[1].id, posts[1].views, posts[1].date
    );
    println!(
        "\t3. {}: {}\t({})",
        posts[2].id, posts[2].views, posts[2].date
    );
    println!("");

    let style = r#"
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

    let header = "<h1><a href=digest.html>Айти Тудэй Дайджест</a></h1>";

    let by_replies: String = HtmlPage::new()
        .with_style(style)
        .with_title("Айти Тудэй Дайджест")
        .with_raw(header)
        .with_header(2, "По комментариям:")
        .with_header(3, "1.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            replies[0]
        ))
        .with_header(3, "2.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            replies[1]
        ))
        .with_header(3, "3.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            replies[2]
        ))
        .to_html_string();

    let by_reactions: String = HtmlPage::new()
        .with_style(style)
        .with_title("Айти Тудэй Дайджест")
        .with_raw(header)
        .with_header(2, "По реакциям:")
        .with_header(3, "1.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            reactions[0]
        ))
        .with_header(3, "2.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            reactions[1]
        ))
        .with_header(3, "3.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            reactions[2]
        ))
        .to_html_string();

    let by_reposts: String = HtmlPage::new()
        .with_style(style)
        .with_title("Айти Тудэй Дайджест")
        .with_raw(header)
        .with_header(2, "По репостам:")
        .with_header(3, "1.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            forwards[0]
        ))
        .with_header(3, "2.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            forwards[1]
        ))
        .with_header(3, "3.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            forwards[2]
        ))
        .to_html_string();

    let by_views: String = HtmlPage::new()
        .with_style(style)
        .with_title("Айти Тудэй Дайджест")
        .with_raw(header)
        .with_header(2, "По просмотрам:")
        .with_header(3, "1.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            views[0]
        ))
        .with_header(3, "2.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            views[1]
        ))
        .with_header(3, "3.")
        .with_raw(format!(
            "<div><script async src=\"https://telegram.org/js/telegram-widget.js?22\"\
        data-telegram-post=\"ithueti/{}\" data-width=\"100%\"\
        data-userpic=\"false\" data-color=\"343638\" data-dark-color=\"FFFFFF\"></script></div>",
            views[2]
        ))
        .to_html_string();

    let digest: String = HtmlPage::new()
        .with_style(style)
        .with_title("Айти Тудэй Дайджест")
        .with_raw(header)
        .with_raw("<h2><a href=replies.html>По комментариям</a></h2>")
        .with_raw("<h2><a href=reactions.html>По реакциям</a></h2>")
        .with_raw("<h2><a href=reposts.html>По репостам</a></h2>")
        .with_raw("<h2><a href=views.html>По просмотрам</a></h2>")
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
