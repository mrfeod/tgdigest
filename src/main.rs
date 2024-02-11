mod action;
mod card_renderer;
mod cli;
mod context;
mod html_renderer;
mod path_util;
mod post;
mod task;
mod tg;
mod util;
mod workers;

use crate::card_renderer::CardRenderer;
use crate::cli::*;
use crate::html_renderer::HtmlRenderer;
use crate::task::*;
use crate::util::*;

use log;
use rocket::fs::NamedFile;
use rocket::response::content;
use rocket::response::content::RawHtml;
use rocket::tokio::task::spawn_blocking;
use simple_logger::SimpleLogger;

#[macro_use]
extern crate rocket;

#[macro_use]
extern crate lazy_static;

struct App {
    args: Args,
    ctx: context::AppContext,
    tg: tg::TelegramAPI,
    html_renderer: HtmlRenderer,
    card_renderer: CardRenderer,
}

impl App {
    fn new() -> Result<App> {
        let args = Args::parse_args();

        let ctx = match context::AppContext::new(args.config.clone()) {
            Ok(ctx) => ctx,
            Err(e) => {
                panic!("Error: {}", e);
            }
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let tg = rt.block_on(tg::TelegramAPI::create(&ctx))?;
        let html_renderer: HtmlRenderer = HtmlRenderer::new(&ctx)?;
        let card_renderer: CardRenderer = rt.block_on(CardRenderer::new())?;

        Ok(App {
            args,
            ctx,
            tg,
            html_renderer,
            card_renderer,
        })
    }
}

lazy_static! {
    static ref APP: App = App::new().unwrap();
}

#[get("/")]
async fn index() -> RawHtml<String> {
    return digest("ithueti", "ithueti", None, None, None, None).await;
}

#[get("/pic/<channel>")]
async fn image(channel: &str) -> Option<NamedFile> {
    let app = &APP;
    let task = Task {
        channel_name: channel.to_string(),
        command: Commands::Digest {},
        ..Task::from_cli(&app.args)
    };
    println!("Working on task: {}", task.to_string().unwrap());

    let file = app
        .ctx
        .output_dir
        .join(format!("{}.png", task.channel_name));
    if file.exists() {
        return NamedFile::open(file).await.ok();
    }

    let tg_task = task.clone();
    let handle = spawn_blocking(|| async {
        let tg = tg::TelegramAPI::create(&app.ctx).await.unwrap();
        let client = tg.client();
        workers::tg::download_pic(client, tg_task, &app.ctx).await
    })
    .await
    .unwrap();

    let file = handle.await.unwrap();

    NamedFile::open(file).await.ok()
}

#[get("/digest/<mode>/<channel>?<top_count>&<editor_choice>&<from_date>&<to_date>")]
async fn digest(
    mode: &str,
    channel: &str,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    from_date: Option<i64>,
    to_date: Option<i64>,
) -> RawHtml<String> {
    let app = &APP;
    let task = Task::from_cli(&app.args);
    let task = Task {
        command: Commands::Digest {},
        mode: mode.to_string(),
        channel_name: channel.to_string(),
        top_count: top_count.unwrap_or(task.top_count),
        editor_choice_post_id: editor_choice.unwrap_or(task.editor_choice_post_id),
        from_date: from_date.unwrap_or(task.from_date),
        to_date: to_date.unwrap_or(task.to_date),
        ..task
    };
    println!("Working on task: {}", task.to_string().unwrap());

    let tg_task = task.clone();
    let handle = spawn_blocking(|| async {
        let tg = tg::TelegramAPI::create(&app.ctx).await.unwrap();
        let client = tg.client();
        workers::tg::get_top_posts(client, tg_task).await
    })
    .await
    .unwrap();

    let post_top = match handle.await {
        Ok(post_top) => post_top,
        Err(e) => return content::RawHtml(e.to_string()),
    };

    let digest_context = match workers::digest::create_context(post_top, task.clone()) {
        Ok(digest_context) => digest_context,
        Err(e) => {
            return content::RawHtml(e.to_string());
        }
    };
    let digest = match app.html_renderer.render(
        format!("{}/digest_template.html", task.mode).as_str(),
        &digest_context,
    ) {
        Ok(digest) => digest,
        Err(e) => {
            return content::RawHtml(e.to_string());
        }
    };
    println!("Digest html rendered: lenght={}", digest.len());
    content::RawHtml(digest)
}

#[launch]
fn rocket() -> _ {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    {
        let app = &APP;
        println!(
            "Load app with config from {}",
            app.args.config.as_ref().unwrap().to_str().unwrap()
        );
    }

    rocket::build().mount("/", routes![index, digest, image])
}

// async fn async_main() -> Result<()> {
//     let cli = Args::parse_args();
//     let ctx = context::AppContext::new(cli.config.clone())?;
//     let task = Task::from_cli(cli);

//     SimpleLogger::new()
//         .with_level(log::LevelFilter::Debug)
//         .init()
//         .unwrap();

//     let tg = tg::TelegramAPI::create(&ctx).await?;
//     let client = tg.client();

//     let post_top = workers::tg::get_top_posts(&client, task.clone()).await?;

//

//     match &task.command {
//         Commands::Digest {} => {
//             let digest_context = workers::digest::create_context(post_top, task.clone())?;
//             let digest_file = html_renderer.render_to_file(
//                 format!("{}/digest_template.html", task.mode).as_str(),
//                 &digest_context,
//             )?;
//             println!("Digest file rendered: {}", digest_file.to_str().unwrap());
//         }
//         Commands::Cards { .. } => {
//             let render_context = workers::cards::create_context(post_top, task.clone())?;

//             let channel_pic = workers::tg::download_pic(&client, task.clone(), &ctx).await;
//             match channel_pic {
//                 Ok(pic) => {
//                     println!("Downloaded pic: {}", pic.to_str().unwrap());
//                 }
//                 Err(e) => {
//                     println!("Error: {}", e);
//                 }
//             }

//             let rendered_html = html_renderer.render(
//                 format!("{}/render_template.html", task.mode).as_str(),
//                 &render_context,
//             )?;
//             println!(
//                 "Render file rendered to html: lenght={}",
//                 rendered_html.len()
//             );

//             card_renderer
//                 .render_html(&ctx.output_dir, &rendered_html)
//                 .await?;
//         }
//     }

//     // End

//     Ok(())
// }

// fn main() -> Result<()> {
//     runtime::Builder::new_current_thread()
//         .enable_all()
//         .build()
//         .unwrap()
//         .block_on(async_main())
// }
