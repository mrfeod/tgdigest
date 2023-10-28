use build_html::*;
use chrono::{Local, NaiveDateTime};
use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;
use log;
use partial_sort::PartialSort;
use simple_logger::SimpleLogger;
use std::fs;
use std::io::{self, BufRead as _, Write as _};
use substring::Substring;
use tokio::runtime;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SESSION_FILE: &str = "dialogs.session";

const HTML_STYLE: &str = r#"
    @import url("https://fonts.googleapis.com/css?family=Tenor+Sans&display=swap");
    h1 {
        display: block;
        font-size: 2em;
        margin-block-start: 0.67em;
        margin-block-end: 0.67em;
        margin-inline-start: 0px;
        margin-inline-end: 0px;
        font-weight: bold;
    }
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
    }
    .filter-blue {
        filter: hue-rotate(185deg);
    }"#;

const HTML_ON_LOAD_SCRIPT: &str = r#"
    <script>
    window.addEventListener('load', function () {
        var icons = document.getElementsByTagName("img");
        for (let icon of icons) {
            icon.height = icon.parentElement.clientHeight * 0.7
        }

        const widgets = new Map();
        const no_widgets = [];
        var scripts = document.getElementsByTagName("script");
        for (var i = 0; i < scripts.length; i++) {
            var widget = scripts[i].parentNode.getElementsByTagName("iframe").item(0);
            var post = scripts[i].getAttribute("data-telegram-post");
            if (post) {
                if (widget) widgets.set(post, widget)
                else no_widgets.push(i)
            }
        }
        for (var i = 0; i < no_widgets.length; i++) {
            var script = scripts[no_widgets[i]]
            var widget = widgets.get(script.getAttribute("data-telegram-post"))
            script.parentNode.insertBefore(widget.cloneNode(), script)
        }
    });
    </script>
"#;

const HTML_BLUE_FILTER: &str = "class=\"filter-blue\"";

fn get_utf8_code(char: char) -> String {
    format!("{:04x}", char as u32)
}

fn icon(icon: char, filter: Option<&str>) -> String {
    let src = format!(
        "https://raw.githubusercontent.com/googlefonts/noto-emoji/main/svg/emoji_u{}.svg",
        get_utf8_code(icon)
    );
    format!("<img src=\"{src}\" height=\"0\" {}/>", filter.unwrap_or(""))
}

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

    fn base_page() -> HtmlPage {
        let header = Table::new().with_header_row([
            "<img src=\"https://static.tildacdn.com/tild3834-6636-4436-a331-613738386539/digest_left.png\" height=\"0\" />",
            "<h1><a href=/digest> Айти Тудэй Дайджест </a></h1>",
            "<img src=\"https://static.tildacdn.com/tild3437-3835-4831-b333-383239323034/digest_right.png\" height=\"0\" />"]);
        HtmlPage::new()
            .with_meta(vec![("charset", "UTF-8")])
            .with_style(HTML_STYLE)
            .with_title("Айти Тудэй Дайджест")
            .with_table(header)
    }

    fn generate_page<F>(
        base: HtmlPage,
        posts: &Vec<Post>,
        header: &str,
        icon: String,
        count: F,
    ) -> HtmlPage
    where
        F: Fn(&Post) -> i32,
    {
        base.with_header(2, format!("{header} {icon}"))
            .with_header(3, format!("1. {icon} {}", count(&posts[0])))
            .with_raw(widget(posts[0].id))
            .with_header(3, format!("2. {icon} {}", count(&posts[1])))
            .with_raw(widget(posts[1].id))
            .with_header(3, format!("3. {icon} {}", count(&posts[2])))
            .with_raw(widget(posts[2].id))
    }
    let digest = generate_page(
        base_page(),
        &replies,
        "По комментариям",
        icon('💬', None),
        |post: &Post| post.replies,
    );
    let digest = generate_page(
        digest,
        &reactions,
        "По реакциям",
        icon('👏', None),
        |post: &Post| post.reactions,
    );
    let digest = generate_page(
        digest,
        &forwards,
        "По репостам",
        icon('🔁', Some(HTML_BLUE_FILTER)), //
        |post: &Post| post.forwards,
    );
    let digest = generate_page(
        digest,
        &views,
        "По просмотрам",
        icon('👁', Some(HTML_BLUE_FILTER)),
        |post: &Post| post.views,
    );
    let digest = digest.with_raw(HTML_ON_LOAD_SCRIPT);

    let mut file = fs::File::create("digest.html")?;
    file.write_all(digest.to_html_string().as_bytes())?;

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
