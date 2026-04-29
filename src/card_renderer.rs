use crate::util::*;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::handler::viewport::Viewport;
use futures_util::StreamExt;
use std::path::Path;
use tokio::sync::{Mutex, Semaphore};

pub struct CardRenderer {
    browser: Browser,
    render_pages: Mutex<Vec<chromiumoxide::Page>>,
    page_pool_permits: Semaphore,
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
                .no_sandbox()
                .arg("--no-sandbox")
                .arg("--disable-setuid-sandbox")
                .arg("--disable-dev-shm-usage")
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

        Ok(CardRenderer {
            browser,
            render_pages: Mutex::new(Vec::new()),
            page_pool_permits: Semaphore::new(2),
        })
    }

    async fn render_page(&self, output_dir: &Path, page: &chromiumoxide::Page) -> Result<()> {
        let cards = page.find_elements("div").await?;

        for (i, card) in cards.iter().enumerate() {
            let card_path = output_dir.join(format!("card_{:02}.png", i));
            let _ = card
                .save_screenshot(CaptureScreenshotFormat::Png, &card_path)
                .await?;
            log::debug!("Card rendered: {}", card_path.to_str().unwrap());
        }

        Ok(())
    }

    pub async fn render_url(&self, output_dir: &Path, url: &str) -> Result<()> {
        log::trace!("Opening URL for rendering: {url}");
        let page = self.browser.new_page(url).await?;
        self.render_page(output_dir, &page).await?;
        page.close().await?;
        Ok(())
    }

    pub async fn render_file(&self, output_dir: &Path, file: &Path) -> Result<()> {
        log::trace!("Opening file for rendering: {}", file.to_str().unwrap());
        let url = String::from("file://") + file.to_str().unwrap();
        self.render_url(output_dir, url.as_str()).await
    }

    pub async fn render_html(&self, output_dir: &Path, html: &str) -> Result<()> {
        let _permit = self.page_pool_permits.acquire().await?;

        // Navigate to the server so the page has the right origin for iframe loading
        let port = std::env::var("ROCKET_PORT").unwrap_or_else(|_| "8000".to_string());
        let page = match self.render_pages.lock().await.pop() {
            Some(page) => page,
            None => self.browser.new_page(format!("http://127.0.0.1:{}", port)).await?,
        };

        if let Err(e) = async {
            page.set_content(html).await?;
            page.wait_for_navigation().await?;

            // Wait for window.__READY flag set by render templates (up to 30s)
            let wait_js = r#"
                new Promise((resolve) => {
                    if (window.__READY) { resolve(); return; }
                    const check = setInterval(() => {
                        if (window.__READY) { clearInterval(check); resolve(); }
                    }, 100);
                    setTimeout(() => { clearInterval(check); resolve(); }, 30000);
                })
            "#;
            let _ = page.evaluate(wait_js).await;

            self.render_page(output_dir, &page).await
        }
        .await
        {
            drop(page);
            return Err(e);
        }

        self.render_pages.lock().await.push(page);
        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        log::info!("Closing browser...");
        let pages = std::mem::take(&mut *self.render_pages.lock().await);
        for page in pages {
            page.close().await?;
        }
        self.browser.close().await?;
        match self.browser.wait().await {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
