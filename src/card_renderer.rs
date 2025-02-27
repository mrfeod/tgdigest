use crate::util::*;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::handler::viewport::Viewport;
use futures_util::StreamExt;
use std::path::Path;

pub struct CardRenderer {
    browser: Browser,
}

impl CardRenderer {
    pub async fn new() -> Result<CardRenderer> {
        let viewport = Viewport {
            width: 2000,
            height: 30000,
            device_scale_factor: Some(1.0),
            emulating_mobile: false,
            is_landscape: false,
            has_touch: false,
        };

        let (browser, mut handler) = Browser::launch(
            BrowserConfig::builder()
                .window_size(2000, 30000)
                .viewport(viewport)
                .build()?,
        )
        .await?;

        // spawn a new task that continuously polls the handler
        tokio::task::spawn(async move {
            while let Some(h) = handler.next().await {
                if let Err(e) = h {
                    log::warn!("Browser handler error: {e:?}");
                    break;
                }
            }
        });

        Ok(CardRenderer { browser })
    }

    async fn render_page(&self, output_dir: &Path, page: chromiumoxide::Page) -> Result<()> {
        let cards = page.find_elements("div").await?;

        for (i, card) in cards.iter().enumerate() {
            let card_path = output_dir.join(format!("card_{:02}.png", i));
            let _ = card
                .save_screenshot(CaptureScreenshotFormat::Png, &card_path)
                .await?;
            log::debug!("Card rendered: {}", card_path.to_str().unwrap());
        }

        page.close().await?;
        Ok(())
    }

    pub async fn render_url(&self, output_dir: &Path, url: &str) -> Result<()> {
        log::trace!("Opening URL for rendering: {url}");
        let page = self.browser.new_page(url).await?;
        self.render_page(output_dir, page).await
    }

    pub async fn render_file(&self, output_dir: &Path, file: &Path) -> Result<()> {
        log::trace!("Opening file for rendering: {}", file.to_str().unwrap());
        let url = String::from("file://") + file.to_str().unwrap();
        self.render_url(output_dir, url.as_str()).await
    }

    pub async fn render_html(&self, output_dir: &Path, html: &str) -> Result<()> {
        let page = self.browser.new_page("about:blank").await?;
        page.set_content(html).await?;
        page.wait_for_navigation().await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        self.render_page(output_dir, page).await
    }

    pub async fn close(&mut self) -> Result<()> {
        log::info!("Closing browser...");
        self.browser.close().await?;
        match self.browser.wait().await {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
