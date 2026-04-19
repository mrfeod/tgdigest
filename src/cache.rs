//! # Post Cache Logic
//!
//! Fetching rules form a hierarchy — each level extends the previous one:
//!
//! 1. **Newest 200 posts** in the requested range are always re-fetched
//!    (debounce: at most once per minute). This keeps the "head" of the
//!    channel accurate — views, reactions, etc. change quickly on
//!    recent posts.
//!
//! 2. **`top_count`** (capped at 1 000 by default) determines how many
//!    *additional* posts to pull into the cache beyond the 200 head.
//!    Each successive request resumes from the oldest cached post,
//!    progressively walking back in history.
//!
//! 3. **`force_limit=true`** removes the 1 000 cap — `top_count` is used
//!    as-is (e.g. `top_count=10000`). If the resulting number exceeds
//!    30 000 (the per-request limit of grammers `iter_messages`),
//!    multiple Telegram calls are made automatically.
//!
//! 4. **`force=true`** ignores the cache entirely: every post in the
//!    requested range is re-fetched from scratch and the cache is
//!    overwritten.
//!
//! Additionally, **posts from the last 7 days** whose `fetched_at` is
//! older than 24 hours are re-fetched (TTL = 1 day). Posts older than
//! 7 days are considered permanently fresh and are never re-fetched
//! unless `force=true`.

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::post::Post;
use crate::util::Result;

const DAY: i64 = 86400;
const WEEK: i64 = 7 * DAY;

/// Maximum number of posts per single grammers `iter_messages` call.
pub const MAX_FETCH_PER_REQUEST: usize = 30_000;

/// The newest N posts in a requested range are always re-fetched.
pub const ALWAYS_REFRESH_HEAD: usize = 200;

/// Default cap on progressive fetch when `force_limit` is off.
pub const DEFAULT_FETCH_CAP: usize = 1_000;

const MAX_CACHED_MEDIA_SIZE: i64 = 10 * 1024 * 1024; // 10 MB

/// Describes what needs to be fetched from Telegram.
/// Each entry is `(from_date, to_date, limit)`.
pub struct FetchPlan {
    pub ranges: Vec<(i64, i64, usize)>,
}

impl FetchPlan {
    fn new(ranges: Vec<(i64, i64, usize)>) -> Self {
        Self { ranges }
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn total_limit(&self) -> usize {
        self.ranges.iter().map(|r| r.2).sum()
    }
}

pub struct PostCache {
    conn: std::sync::Mutex<Connection>,
    media_dir: PathBuf,
    cache_limit_bytes: i64,
}

impl PostCache {
    pub fn new(db_path: &Path, media_dir: &Path, cache_limit_mb: u64) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS posts (
                channel TEXT NOT NULL,
                id INTEGER NOT NULL,
                date INTEGER NOT NULL,
                views INTEGER,
                forwards INTEGER,
                replies INTEGER,
                reactions INTEGER,
                message TEXT,
                image INTEGER,
                fetched_at INTEGER NOT NULL DEFAULT 0,
                grouped_id INTEGER,
                PRIMARY KEY (channel, id)
            );
            CREATE TABLE IF NOT EXISTS media_cache (
                channel TEXT NOT NULL,
                msg_id INTEGER NOT NULL,
                media_id INTEGER NOT NULL,
                mime TEXT NOT NULL,
                size INTEGER NOT NULL,
                last_accessed INTEGER NOT NULL,
                PRIMARY KEY (channel, msg_id)
            );
            CREATE TABLE IF NOT EXISTS channel_fetch_bounds (
                channel TEXT NOT NULL PRIMARY KEY,
                min_fetched_date INTEGER NOT NULL,
                max_fetched_date INTEGER NOT NULL
            );",
        )?;

        // Migrate: if the schema doesn't match (missing columns), recreate the posts table
        let schema_ok = conn
            .prepare("SELECT id, date, views, forwards, replies, reactions, message, image, fetched_at, grouped_id FROM posts LIMIT 0")
            .is_ok();
        if !schema_ok {
            log::info!("Posts table schema mismatch — recreating");
            conn.execute_batch("DROP TABLE IF EXISTS posts; DROP TABLE IF EXISTS fetch_log; DELETE FROM channel_fetch_bounds;")?;
            conn.execute_batch(
                "CREATE TABLE posts (
                    channel TEXT NOT NULL,
                    id INTEGER NOT NULL,
                    date INTEGER NOT NULL,
                    views INTEGER,
                    forwards INTEGER,
                    replies INTEGER,
                    reactions INTEGER,
                    message TEXT,
                    image INTEGER,
                    fetched_at INTEGER NOT NULL DEFAULT 0,
                    grouped_id INTEGER,
                    PRIMARY KEY (channel, id)
                );",
            )?;
        }

        std::fs::create_dir_all(media_dir)?;

        Ok(Self {
            conn: std::sync::Mutex::new(conn),
            media_dir: media_dir.to_path_buf(),
            cache_limit_bytes: cache_limit_mb as i64 * 1024 * 1024,
        })
    }

    pub fn media_dir(&self) -> &Path {
        &self.media_dir
    }

    // ── Post cache ─────────────────────────────────────────────────────

    /// Returns all cached posts in the range (always returned, even if a
    /// re-fetch is also needed) and a `FetchPlan` describing what must be
    /// fetched from Telegram.
    pub fn get_posts_and_fetch_plan(
        &self,
        channel: &str,
        from_date: i64,
        to_date: i64,
        force_limit: Option<usize>,
        force: bool,
    ) -> Result<(Vec<Post>, FetchPlan)> {
        // ── force mode: ignore cache, re-fetch everything ──────────
        if force {
            let limit = force_limit.unwrap_or(MAX_FETCH_PER_REQUEST);
            return Ok((vec![], FetchPlan::new(vec![(from_date, to_date, limit)])));
        }

        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        // Load all cached posts in the range (date ASC, id ASC)
        let mut stmt = conn.prepare(
            "SELECT id, date, views, forwards, replies, reactions, message, image, fetched_at, grouped_id
             FROM posts WHERE channel = ?1 AND date >= ?2 AND date <= ?3
             ORDER BY date ASC, id ASC",
        )?;

        let mut all_posts: Vec<(Post, i64)> = Vec::new();
        let mut seen_groups: std::collections::HashSet<i64> = std::collections::HashSet::new();

        // Track which posts need refresh (newest 1000 + stale weekly)
        let mut needs_head_refresh = false;
        let mut needs_weekly_refresh = false;
        let mut weekly_stale_min_date = i64::MAX;

        let rows = stmt.query_map(params![channel, from_date, to_date], |row| {
            Ok((
                Post {
                    id: row.get(0)?,
                    date: row.get(1)?,
                    views: row.get(2)?,
                    forwards: row.get(3)?,
                    replies: row.get(4)?,
                    reactions: row.get(5)?,
                    message: row.get(6)?,
                    image: row.get(7)?,
                    grouped_id: row.get(9)?,
                },
                row.get::<_, i64>(8)?, // fetched_at
            ))
        })?;

        for row in rows {
            let (post, fetched_at) = row?;
            if let Some(gid) = post.grouped_id {
                if !seen_groups.insert(gid) {
                    continue;
                }
            }

            // Check weekly staleness: posts < 7 days old with fetched_at > 1 day ago
            let age = now - post.date;
            if age < WEEK && (now - fetched_at) >= DAY {
                needs_weekly_refresh = true;
                weekly_stale_min_date = weekly_stale_min_date.min(post.date);
            }

            all_posts.push((post, fetched_at));
        }

        let cached_count = all_posts.len();

        // The newest 200 posts are always refreshed, but at most once per minute.
        if cached_count > 0 {
            let head_start = cached_count.saturating_sub(ALWAYS_REFRESH_HEAD);
            let oldest_head_fetch = all_posts[head_start..]
                .iter()
                .map(|(_, fa)| *fa)
                .min()
                .unwrap_or(0);
            if (now - oldest_head_fetch) >= 60 {
                needs_head_refresh = true;
            }
        }

        let all_posts: Vec<Post> = all_posts.into_iter().map(|(p, _)| p).collect();

        // ── Build the fetch plan ───────────────────────────────────
        let mut ranges: Vec<(i64, i64, usize)> = Vec::new();

        // 1. Head refresh: newest 200 posts — fetch from to_date with limit 200
        if needs_head_refresh || cached_count == 0 {
            let head_limit = if cached_count == 0 {
                // Nothing cached yet — do an initial fetch
                force_limit.unwrap_or(ALWAYS_REFRESH_HEAD)
            } else {
                ALWAYS_REFRESH_HEAD
            };
            ranges.push((from_date, to_date, head_limit));
        }

        // 2. Weekly refresh: posts < 7 days old that are stale
        if needs_weekly_refresh {
            let weekly_from = (now - WEEK).max(from_date);
            // Avoid duplicate range if head refresh already covers this
            let dominated_by_head = needs_head_refresh && weekly_from >= from_date;
            if !dominated_by_head || weekly_stale_min_date < (now - WEEK).max(from_date) {
                // Only add a separate weekly range if it's different from head range
                let already_covered = ranges.iter().any(|&(f, t, _)| f <= weekly_from && t >= to_date);
                if !already_covered {
                    ranges.push((weekly_from, to_date, MAX_FETCH_PER_REQUEST));
                }
            }
        }

        // 3. Progressive fetch (force_limit): fetch N more posts below the oldest cached post
        if let Some(limit) = force_limit {
            if cached_count > 0 {
                let oldest_cached_date: i64 = conn.query_row(
                    "SELECT MIN(date) FROM posts WHERE channel = ?1 AND date >= ?2 AND date <= ?3",
                    params![channel, from_date, to_date],
                    |row| row.get(0),
                )?;
                // Fetch from [from_date .. oldest_cached_date] to walk backward
                if oldest_cached_date > from_date {
                    ranges.push((from_date, oldest_cached_date, limit));
                }
            }
            // If cached_count == 0, the head_limit above already uses force_limit
        }

        // 4. Check for uncovered edges beyond what we've previously fetched
        if ranges.is_empty() || force_limit.is_some() {
            let fetch_bounds: Option<(i64, i64)> = conn.query_row(
                "SELECT min_fetched_date, max_fetched_date FROM channel_fetch_bounds WHERE channel = ?1",
                params![channel],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).ok();

            if let Some((min_fetched, max_fetched)) = fetch_bounds {
                let edge_limit = force_limit.unwrap_or(ALWAYS_REFRESH_HEAD);
                if from_date < min_fetched {
                    ranges.push((from_date, min_fetched, edge_limit));
                }
                if to_date > max_fetched {
                    ranges.push((max_fetched, to_date, edge_limit));
                }
            }
        }

        log::debug!(
            "Cache for {} [{} .. {}]: {} cached posts, {} fetch ranges",
            channel, from_date, to_date, cached_count, ranges.len()
        );

        Ok((all_posts, FetchPlan::new(ranges)))
    }

    /// Return the number of cached posts for a channel in a date range.
    pub fn count_cached_posts(&self, channel: &str, from_date: i64, to_date: i64) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM posts WHERE channel = ?1 AND date >= ?2 AND date <= ?3",
            params![channel, from_date, to_date],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn store_posts(&self, channel: &str, posts: &[Post]) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        let tx = conn.unchecked_transaction()?;

        for post in posts {
            tx.execute(
                "INSERT OR REPLACE INTO posts (channel, id, date, views, forwards, replies, reactions, message, image, fetched_at, grouped_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![channel, post.id, post.date, post.views, post.forwards, post.replies, post.reactions, post.message, post.image, now, post.grouped_id],
            )?;
        }

        tx.commit()?;
        log::debug!("Cached {} posts for {}", posts.len(), channel);
        Ok(())
    }

    /// Touch fetched_at for all cached posts in a range so they're no longer stale.
    /// Covers posts that exist in cache but were not returned by the API (e.g. deleted).
    pub fn touch_posts_in_range(&self, channel: &str, from_date: i64, to_date: i64) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();
        let updated = conn.execute(
            "UPDATE posts SET fetched_at = ?1 WHERE channel = ?2 AND date >= ?3 AND date <= ?4",
            params![now, channel, from_date, to_date],
        )?;
        if updated > 0 {
            log::debug!("Touched fetched_at for {} posts in range [{} .. {}] for {}", updated, from_date, to_date, channel);
        }
        Ok(())
    }

    pub fn update_fetch_bounds(&self, channel: &str, from_date: i64, to_date: i64) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT INTO channel_fetch_bounds (channel, min_fetched_date, max_fetched_date)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(channel) DO UPDATE SET
             min_fetched_date = MIN(min_fetched_date, excluded.min_fetched_date),
             max_fetched_date = MAX(max_fetched_date, excluded.max_fetched_date)",
            params![channel, from_date, to_date],
        )?;
        Ok(())
    }

    // ── Media cache ────────────────────────────────────────────────────

    fn media_path(&self, media_id: i64, mime: &str) -> PathBuf {
        let ext = crate::post_data::mime_ext(mime);
        self.media_dir.join(format!("{}.{}", media_id, ext))
    }

    /// Returns (file_path, mime_type, file_size) if cached.
    pub fn get_cached_media(&self, channel: &str, msg_id: i32) -> Result<Option<(PathBuf, String, i64)>> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        let result: std::result::Result<(String, i64, i64), _> = conn.query_row(
            "SELECT mime, size, media_id FROM media_cache WHERE channel = ?1 AND msg_id = ?2",
            params![channel, msg_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        match result {
            Ok((mime, size, media_id)) => {
                let path = self.media_path(media_id, &mime);
                if path.exists() {
                    // Update last_accessed (LRU touch)
                    let _ = conn.execute(
                        "UPDATE media_cache SET last_accessed = ?1 WHERE channel = ?2 AND msg_id = ?3",
                        params![now, channel, msg_id],
                    );
                    log::debug!("Media cache hit: {}/{}", channel, msg_id);
                    Ok(Some((path, mime, size)))
                } else {
                    // File missing — remove stale DB entry
                    let _ = conn.execute(
                        "DELETE FROM media_cache WHERE channel = ?1 AND msg_id = ?2",
                        params![channel, msg_id],
                    );
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Store media bytes to disk and register in DB. Runs LRU eviction if needed.
    /// Only caches files <= 10 MB.
    pub fn store_cached_media(&self, channel: &str, msg_id: i32, media_id: i64, mime: &str, data: &[u8]) -> Result<()> {
        let size = data.len() as i64;
        if size > MAX_CACHED_MEDIA_SIZE {
            return Ok(()); // too large, skip
        }

        let path = self.media_path(media_id, mime);
        std::fs::write(&path, data)?;

        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT OR REPLACE INTO media_cache (channel, msg_id, media_id, mime, size, last_accessed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![channel, msg_id, media_id, mime, size, now],
        )?;

        // Run LRU eviction
        self.evict_media_locked(&conn)?;

        log::debug!("Media cached: {}/{} ({} bytes)", channel, msg_id, size);
        Ok(())
    }

    fn evict_media_locked(&self, conn: &Connection) -> Result<()> {
        let total_size: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size), 0) FROM media_cache",
            [],
            |row| row.get(0),
        )?;

        if total_size <= self.cache_limit_bytes {
            return Ok(());
        }

        let to_free = total_size - self.cache_limit_bytes;
        let mut freed: i64 = 0;

        let mut stmt = conn.prepare(
            "SELECT channel, msg_id, size, media_id, mime FROM media_cache ORDER BY last_accessed ASC",
        )?;
        let entries: Vec<(String, i32, i64, i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        for (ch, mid, sz, media_id, mime) in entries {
            if freed >= to_free {
                break;
            }
            let path = self.media_path(media_id, &mime);
            let _ = std::fs::remove_file(&path);
            conn.execute(
                "DELETE FROM media_cache WHERE channel = ?1 AND msg_id = ?2",
                params![ch, mid],
            )?;
            freed += sz;
            log::debug!("Media evicted: {}/{} ({} bytes)", ch, mid, sz);
        }

        log::info!("Media cache eviction: freed {} bytes", freed);
        Ok(())
    }
}
