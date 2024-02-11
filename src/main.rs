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
use simple_logger::SimpleLogger;
use tokio::runtime;

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
    let channel_pic = workers::tg::download_pic(&client, task.clone(), &ctx).await;
    match channel_pic {
        Ok(pic) => {
            println!("Downloaded pic: {}", pic.to_str().unwrap());
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    let post_top = workers::tg::get_top_posts(&client, task.clone()).await?;

    let html_renderer: HtmlRenderer = HtmlRenderer::new(&ctx)?;
    let card_renderer = CardRenderer::new().await?;

    match &task.command {
        Commands::Digest {} => {
            let digest_context = workers::digest::create_context(post_top, task.clone())?;
            let digest_file = html_renderer.render_to_file(
                format!("{}/digest_template.html", task.mode).as_str(),
                &digest_context,
            )?;
            println!("Digest file rendered: {}", digest_file.to_str().unwrap());
        }
        Commands::Cards { .. } => {
            let render_context = workers::cards::create_context(post_top, task.clone())?;
            let rendered_html = html_renderer.render(
                format!("{}/render_template.html", task.mode).as_str(),
                &render_context,
            )?;
            println!(
                "Render file rendered to html: lenght={}",
                rendered_html.len()
            );

            card_renderer
                .render_html(&ctx.output_dir, &rendered_html)
                .await?;
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
