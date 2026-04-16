use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::post::Post;
use crate::util::Result;

const DAY: i64 = 86400;
const MONTH: i64 = 30 * DAY;

const MAX_CACHED_MEDIA_SIZE: i64 = 10 * 1024 * 1024; // 10 MB

/// Returns the cache TTL for a post based on its age.
/// - Older than 1 month: None (never refresh)
/// - Less than 1 month: 1 day
fn post_ttl(post_date: i64, now: i64) -> Option<i64> {
    let age = now - post_date;
    if age > MONTH {
        None // never refresh
    } else {
        Some(DAY)
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
            conn.execute_batch("DROP TABLE IF EXISTS posts; DROP TABLE IF EXISTS fetch_log;")?;
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

    // ── Post cache ─────────────────────────────────────────────────────

    /// Returns (fresh_posts, stale_ranges).
    /// Fresh posts are returned directly; stale_ranges are date intervals that
    /// need re-fetching from Telegram.
    pub fn get_posts_and_stale_ranges(
        &self,
        channel: &str,
        from_date: i64,
        to_date: i64,
    ) -> Result<(Vec<Post>, Vec<(i64, i64)>)> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        // Check if we have ANY cached posts for this range
        let cached_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM posts WHERE channel = ?1 AND date >= ?2 AND date <= ?3",
            params![channel, from_date, to_date],
            |row| row.get(0),
        )?;

        if cached_count == 0 {
            // No cached posts — check if we've previously fetched this range
            let fetch_bounds: Option<(i64, i64)> = conn.query_row(
                "SELECT min_fetched_date, max_fetched_date FROM channel_fetch_bounds WHERE channel = ?1",
                params![channel],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).ok();

            let uncovered = match fetch_bounds {
                Some((min_fetched, max_fetched)) => {
                    let mut ranges = Vec::new();
                    if from_date < min_fetched {
                        ranges.push((from_date, min_fetched));
                    }
                    if to_date > max_fetched {
                        ranges.push((max_fetched, to_date));
                    }
                    ranges
                }
                None => vec![(from_date, to_date)],
            };

            log::debug!("Cache miss for {} [{} .. {}]: {} uncovered ranges", channel, from_date, to_date, uncovered.len());
            return Ok((vec![], uncovered));
        }

        let mut stmt = conn.prepare(
            "SELECT id, date, views, forwards, replies, reactions, message, image, fetched_at, grouped_id
             FROM posts WHERE channel = ?1 AND date >= ?2 AND date <= ?3
             ORDER BY date ASC, id ASC",
        )?;

        let mut fresh_posts = Vec::new();
        let mut stale_dates: Vec<i64> = Vec::new();
        let mut seen_groups: std::collections::HashSet<i64> = std::collections::HashSet::new();

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

        let mut all_min_date = i64::MAX;
        let mut all_max_date = i64::MIN;

        for row in rows {
            let (post, fetched_at) = row?;
            // Deduplicate album posts: keep only the smallest id per grouped_id
            if let Some(gid) = post.grouped_id {
                if !seen_groups.insert(gid) {
                    continue; // already seen this group, skip (ORDER BY id ASC ensures smallest first)
                }
            }
            all_min_date = all_min_date.min(post.date);
            all_max_date = all_max_date.max(post.date);
            let ttl = post_ttl(post.date, now);
            let is_fresh = match ttl {
                None => true, // never expires
                Some(ttl) => (now - fetched_at) < ttl,
            };
            if is_fresh {
                fresh_posts.push(post);
            } else {
                stale_dates.push(post.date);
            }
        }

        // Build contiguous stale ranges from stale post dates
        let mut stale_ranges = if stale_dates.is_empty() {
            vec![]
        } else {
            Self::build_stale_ranges(&stale_dates, from_date, to_date)
        };

        // Check for uncovered edges beyond what we've previously fetched
        let fetch_bounds: Option<(i64, i64)> = conn.query_row(
            "SELECT min_fetched_date, max_fetched_date FROM channel_fetch_bounds WHERE channel = ?1",
            params![channel],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();

        match fetch_bounds {
            Some((min_fetched, max_fetched)) => {
                if from_date < min_fetched {
                    stale_ranges.push((from_date, min_fetched));
                }
                if to_date > max_fetched {
                    stale_ranges.push((max_fetched, to_date));
                }
            }
            None => {
                // No fetch bounds recorded yet — use cached post dates as estimate
                if from_date < all_min_date {
                    stale_ranges.push((from_date, all_min_date));
                }
                if to_date > all_max_date {
                    stale_ranges.push((all_max_date, to_date));
                }
            }
        }

        log::debug!(
            "Cache for {} [{} .. {}]: {} fresh, {} stale ranges",
            channel, from_date, to_date,
            fresh_posts.len(), stale_ranges.len()
        );

        Ok((fresh_posts, stale_ranges))
    }

    /// Build stale range: all stale posts are within the last month,
    /// so we return a single range covering from the boundary to to_date.
    fn build_stale_ranges(stale_dates: &[i64], from_date: i64, to_date: i64) -> Vec<(i64, i64)> {
        if stale_dates.is_empty() {
            return vec![];
        }

        let now = chrono::Utc::now().timestamp();
        let boundary_1m = now - MONTH;

        // Only posts younger than 1 month can be stale
        if stale_dates.iter().any(|&d| d > boundary_1m) {
            vec![(boundary_1m.max(from_date), to_date)]
        } else {
            vec![]
        }
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
