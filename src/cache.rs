use rusqlite::{params, Connection};

use crate::post::Post;
use crate::util::Result;

const CACHE_TTL_SECS: i64 = 86400; // 24 hours

pub struct PostCache {
    conn: std::sync::Mutex<Connection>,
}

impl PostCache {
    pub fn new(db_path: &std::path::Path) -> Result<Self> {
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
                PRIMARY KEY (channel, id)
            );
            CREATE TABLE IF NOT EXISTS fetch_log (
                channel TEXT NOT NULL,
                from_date INTEGER NOT NULL,
                to_date INTEGER NOT NULL,
                fetched_at INTEGER NOT NULL,
                PRIMARY KEY (channel, from_date, to_date)
            );",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    pub fn get_cached_posts(
        &self,
        channel: &str,
        from_date: i64,
        to_date: i64,
    ) -> Result<Option<Vec<Post>>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        let fresh: bool = conn.query_row(
            "SELECT COUNT(*) FROM fetch_log WHERE channel = ?1 AND from_date = ?2 AND to_date = ?3 AND fetched_at > ?4",
            params![channel, from_date, to_date, now - CACHE_TTL_SECS],
            |row| row.get::<_, i64>(0),
        )? > 0;

        if !fresh {
            return Ok(None);
        }

        let mut stmt = conn.prepare(
            "SELECT id, date, views, forwards, replies, reactions, message, image FROM posts WHERE channel = ?1 AND date >= ?2 AND date <= ?3",
        )?;
        let posts = stmt
            .query_map(params![channel, from_date, to_date], |row| {
                Ok(Post {
                    id: row.get(0)?,
                    date: row.get(1)?,
                    views: row.get(2)?,
                    forwards: row.get(3)?,
                    replies: row.get(4)?,
                    reactions: row.get(5)?,
                    message: row.get(6)?,
                    image: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        log::debug!(
            "Cache hit for {} [{} .. {}]: {} posts",
            channel,
            from_date,
            to_date,
            posts.len()
        );
        Ok(Some(posts))
    }

    pub fn store_posts(
        &self,
        channel: &str,
        from_date: i64,
        to_date: i64,
        posts: &[Post],
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        let tx = conn.unchecked_transaction()?;

        for post in posts {
            tx.execute(
                "INSERT OR REPLACE INTO posts (channel, id, date, views, forwards, replies, reactions, message, image) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![channel, post.id, post.date, post.views, post.forwards, post.replies, post.reactions, post.message, post.image],
            )?;
        }

        tx.execute(
            "INSERT OR REPLACE INTO fetch_log (channel, from_date, to_date, fetched_at) VALUES (?1, ?2, ?3, ?4)",
            params![channel, from_date, to_date, now],
        )?;

        tx.commit()?;
        log::debug!(
            "Cached {} posts for {} [{} .. {}]",
            posts.len(),
            channel,
            from_date,
            to_date
        );
        Ok(())
    }
}
