use rusqlite::{params, Connection, OptionalExtension, Result};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use super::models::SitePermissionEntry;
use super::models::{
    Bookmark, FingerprintStat, GroupTabEntry, HistoryEntry, OmniboxItemType, OmniboxResult,
    PrivacySummary, ReadingItem, TabGroup, TrackerStat,
};

pub const FAVORITES_FOLDER: &str = "Избранное";

const DEFAULT_FAVORITES: [(&str, &str); 4] = [
    ("Apple", "https://apple.com"),
    ("Google", "https://google.com"),
    ("GitHub", "https://github.com"),
    ("DuckDuckGo", "https://duckduckgo.com"),
];

pub struct BrowserDatabase {
    conn: Connection,
}

impl BrowserDatabase {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL lets the background writer and UI-thread readers use separate connections
        // without blocking each other; NORMAL sync is crash-safe under WAL.
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 3000;
            PRAGMA cache_size = -16384;
            PRAGMA temp_store = MEMORY;
            "#,
        )?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        // SQLite ignores ON DELETE CASCADE unless foreign keys are enabled per connection.
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                favicon_url TEXT,
                folder TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                visit_time INTEGER NOT NULL,
                visit_count INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS reading_list (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                domain TEXT NOT NULL,
                preview_text TEXT,
                is_read INTEGER NOT NULL DEFAULT 0,
                added_at INTEGER NOT NULL,
                offline_path TEXT
            );

            CREATE TABLE IF NOT EXISTS tracker_stats (
                domain TEXT PRIMARY KEY,
                tracker_rule TEXT NOT NULL,
                blocked_count INTEGER NOT NULL DEFAULT 1,
                last_blocked_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS fingerprint_stats (
                vector TEXT PRIMARY KEY,
                defended_count INTEGER NOT NULL DEFAULT 0,
                last_defended_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tab_groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                icon TEXT,
                color TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tab_group_tabs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                group_id TEXT NOT NULL,
                title TEXT NOT NULL,
                url TEXT NOT NULL,
                position INTEGER NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (group_id) REFERENCES tab_groups(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS site_permissions (
                origin TEXT NOT NULL,
                permission_type TEXT NOT NULL,
                state TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (origin, permission_type)
            );

            CREATE TABLE IF NOT EXISTS protection_exceptions (
                domain TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS domain_zoom (
                domain TEXT PRIMARY KEY,
                zoom REAL NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_history_visit_time ON history(visit_time DESC);
            CREATE INDEX IF NOT EXISTS idx_reading_added_at ON reading_list(added_at DESC);
            CREATE INDEX IF NOT EXISTS idx_group_tabs ON tab_group_tabs(group_id, position ASC);
            CREATE INDEX IF NOT EXISTS idx_site_permissions_origin ON site_permissions(origin);

            -- FTS5 Full-Text Search for Omnibox autocomplete
            CREATE VIRTUAL TABLE IF NOT EXISTS omnibox_fts USING fts5(
                title,
                url,
                item_type UNINDEXED,
                tokenize='unicode61 remove_diacritics 2'
            );

            CREATE TRIGGER IF NOT EXISTS trg_bookmarks_ai AFTER INSERT ON bookmarks BEGIN
                DELETE FROM omnibox_fts WHERE url = new.url AND item_type = 'bookmark';
                INSERT INTO omnibox_fts(title, url, item_type) VALUES (new.title, new.url, 'bookmark');
            END;

            CREATE TRIGGER IF NOT EXISTS trg_bookmarks_au AFTER UPDATE ON bookmarks BEGIN
                DELETE FROM omnibox_fts WHERE url = old.url AND item_type = 'bookmark';
                INSERT INTO omnibox_fts(title, url, item_type) VALUES (new.title, new.url, 'bookmark');
            END;

            CREATE TRIGGER IF NOT EXISTS trg_bookmarks_ad AFTER DELETE ON bookmarks BEGIN
                DELETE FROM omnibox_fts WHERE url = old.url AND item_type = 'bookmark';
            END;

            CREATE TRIGGER IF NOT EXISTS trg_history_ai AFTER INSERT ON history BEGIN
                DELETE FROM omnibox_fts WHERE url = new.url AND item_type = 'history';
                INSERT INTO omnibox_fts(title, url, item_type) VALUES (new.title, new.url, 'history');
            END;

            CREATE TRIGGER IF NOT EXISTS trg_history_au AFTER UPDATE ON history BEGIN
                DELETE FROM omnibox_fts WHERE url = old.url AND item_type = 'history';
                INSERT INTO omnibox_fts(title, url, item_type) VALUES (new.title, new.url, 'history');
            END;

            CREATE TRIGGER IF NOT EXISTS trg_history_ad AFTER DELETE ON history BEGIN
                DELETE FROM omnibox_fts WHERE url = old.url AND item_type = 'history';
            END;

            INSERT INTO omnibox_fts(title, url, item_type)
            SELECT title, url, 'bookmark' FROM bookmarks
            WHERE NOT EXISTS (SELECT 1 FROM omnibox_fts WHERE item_type = 'bookmark');

            INSERT INTO omnibox_fts(title, url, item_type)
            SELECT title, url, 'history' FROM history
            WHERE NOT EXISTS (SELECT 1 FROM omnibox_fts WHERE item_type = 'history');
            "#,
        )?;

        // Migration: ensure offline_path column exists in reading_list
        let _ = self
            .conn
            .execute("ALTER TABLE reading_list ADD COLUMN offline_path TEXT;", []);

        Ok(())
    }

    /// Fills the favorites folder on the very first launch only.
    /// `user_version` keeps the seed from reappearing after the user removes the defaults.
    pub fn seed_defaults_once(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version == 0 {
            for (title, url) in DEFAULT_FAVORITES {
                self.add_bookmark(title, url, Some(FAVORITES_FOLDER))?;
            }
            self.conn.execute_batch("PRAGMA user_version = 1;")?;
        }

        Ok(())
    }

    fn now_ts() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    pub fn record_history_visit(&self, title: &str, url: &str) -> Result<()> {
        let now = Self::now_ts();
        self.conn.execute(
            r#"
            INSERT INTO history (title, url, visit_time, visit_count)
            VALUES (?1, ?2, ?3, 1)
            ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                visit_time = excluded.visit_time,
                visit_count = history.visit_count + 1
            "#,
            params![title, url, now],
        )?;
        Ok(())
    }

    pub fn get_recent_history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, url, visit_time, visit_count FROM history ORDER BY visit_time DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                visit_time: row.get(3)?,
                visit_count: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn clear_history(&self) -> Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        self.conn
            .execute("DELETE FROM omnibox_fts WHERE item_type = 'history'", [])?;
        Ok(())
    }

    pub fn search_history(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let q = query.trim();
        if q.is_empty() {
            return self.get_recent_history(limit);
        }
        let pattern = format!("%{q}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, title, url, visit_time, visit_count FROM history
             WHERE title LIKE ?1 OR url LIKE ?1
             ORDER BY visit_time DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                visit_time: row.get(3)?,
                visit_count: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn create_tab_group(
        &self,
        id: &str,
        name: &str,
        icon: Option<&str>,
        color: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tab_groups (id, name, icon, color, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, icon, color, Self::now_ts()],
        )?;
        Ok(())
    }

    pub fn rename_tab_group(&self, id: &str, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tab_groups SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn get_tab_groups(&self) -> Result<Vec<TabGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, icon, color, created_at FROM tab_groups ORDER BY created_at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TabGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                icon: row.get(2)?,
                color: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete_tab_group(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM tab_groups WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Replaces the stored tab set of a group. Each tuple is `(title, url, is_pinned)`.
    pub fn save_group_tabs(&self, group_id: &str, tabs: &[(String, String, bool)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM tab_group_tabs WHERE group_id = ?1",
            params![group_id],
        )?;
        for (position, (title, url, pinned)) in tabs.iter().enumerate() {
            tx.execute(
                "INSERT INTO tab_group_tabs (group_id, title, url, position, is_pinned) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![group_id, title, url, position as i64, *pinned as i64],
            )?;
        }
        tx.commit()
    }

    pub fn get_group_tabs(&self, group_id: &str) -> Result<Vec<GroupTabEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, group_id, title, url, position, is_pinned FROM tab_group_tabs WHERE group_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt.query_map(params![group_id], |row| {
            Ok(GroupTabEntry {
                id: row.get(0)?,
                group_id: row.get(1)?,
                title: row.get(2)?,
                url: row.get(3)?,
                position: row.get(4)?,
                is_pinned: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect()
    }

    #[cfg(test)]
    pub fn add_tab_to_group(
        &self,
        group_id: &str,
        title: &str,
        url: &str,
        is_pinned: bool,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tab_group_tabs (group_id, title, url, position, is_pinned)
             VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), -1) + 1 FROM tab_group_tabs WHERE group_id = ?1), ?4)",
            params![group_id, title, url, is_pinned as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    #[cfg(test)]
    pub fn delete_tab_from_group(&self, tab_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM tab_group_tabs WHERE id = ?1", params![tab_id])?;
        Ok(())
    }

    pub fn add_bookmark(&self, title: &str, url: &str, folder: Option<&str>) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO bookmarks (title, url, folder, created_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                folder = excluded.folder
            "#,
            params![title, url, folder, Self::now_ts()],
        )?;
        // last_insert_rowid() is not updated by the DO UPDATE branch, so look the row up.
        self.conn.query_row(
            "SELECT id FROM bookmarks WHERE url = ?1",
            params![url],
            |row| row.get(0),
        )
    }

    pub fn get_bookmarks(&self, folder: Option<&str>) -> Result<Vec<Bookmark>> {
        let map_row = |row: &rusqlite::Row| {
            Ok(Bookmark {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                favicon_url: row.get(3)?,
                folder: row.get(4)?,
                created_at: row.get(5)?,
            })
        };

        match folder {
            Some(name) => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, title, url, favicon_url, folder, created_at FROM bookmarks WHERE folder = ?1 ORDER BY id ASC",
                )?;
                let rows = stmt.query_map(params![name], map_row)?;
                rows.collect()
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, title, url, favicon_url, folder, created_at FROM bookmarks ORDER BY id ASC",
                )?;
                let rows = stmt.query_map([], map_row)?;
                rows.collect()
            }
        }
    }

    pub fn is_bookmarked(&self, url: &str) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM bookmarks WHERE url = ?1",
                params![url],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    pub fn delete_bookmark(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_bookmark_by_url(&self, url: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM bookmarks WHERE url = ?1", params![url])?;
        Ok(())
    }

    /// Full-text search across bookmarks and history for Omnibox autocomplete.
    /// Prioritizes bookmarks over history and deduplicates URLs.
    pub fn search_omnibox(&self, query: &str, limit: usize) -> Result<Vec<OmniboxResult>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            // Empty query: return top bookmarks and recent history
            let mut stmt = self.conn.prepare(
                r#"
                SELECT title, url, 'bookmark' as item_type, created_at as sort_key FROM bookmarks
                UNION ALL
                SELECT title, url, 'history' as item_type, visit_time as sort_key FROM history
                WHERE url NOT IN (SELECT url FROM bookmarks)
                ORDER BY sort_key DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt.query_map(params![limit as i64], |row| {
                let type_str: String = row.get(2)?;
                let item_type = if type_str == "bookmark" {
                    OmniboxItemType::Bookmark
                } else {
                    OmniboxItemType::History
                };
                Ok(OmniboxResult {
                    title: row.get(0)?,
                    url: row.get(1)?,
                    item_type,
                })
            })?;
            return rows.collect();
        }

        // 1. Try fast FTS5 search with prefix matching
        let fts_query = Self::sanitize_fts5_query(trimmed);
        if !fts_query.is_empty() {
            let sql = r#"
                SELECT title, url, item_type
                FROM (
                    SELECT title, url, item_type, rank,
                           ROW_NUMBER() OVER (
                               PARTITION BY url
                               ORDER BY (CASE WHEN item_type = 'bookmark' THEN 0 ELSE 1 END), rank
                           ) as rn
                    FROM omnibox_fts
                    WHERE omnibox_fts MATCH ?1
                )
                WHERE rn = 1
                ORDER BY (CASE WHEN item_type = 'bookmark' THEN 0 ELSE 1 END), rank
                LIMIT ?2
            "#;
            if let Ok(mut stmt) = self.conn.prepare(sql) {
                if let Ok(rows) = stmt.query_map(params![fts_query, limit as i64], |row| {
                    let type_str: String = row.get(2)?;
                    let item_type = if type_str == "bookmark" {
                        OmniboxItemType::Bookmark
                    } else {
                        OmniboxItemType::History
                    };
                    Ok(OmniboxResult {
                        title: row.get(0)?,
                        url: row.get(1)?,
                        item_type,
                    })
                }) {
                    let collected: Result<Vec<OmniboxResult>> = rows.collect();
                    if let Ok(results) = collected {
                        if !results.is_empty() {
                            return Ok(results);
                        }
                    }
                }
            }
        }

        // 2. Fallback to SQL LIKE substring search
        let pattern = format!("%{trimmed}%");
        let mut stmt = self.conn.prepare(
            r#"
            SELECT title, url, 'bookmark' as item_type
            FROM bookmarks
            WHERE title LIKE ?1 OR url LIKE ?1
            UNION
            SELECT title, url, 'history' as item_type
            FROM history
            WHERE (title LIKE ?1 OR url LIKE ?1)
              AND url NOT IN (SELECT url FROM bookmarks WHERE title LIKE ?1 OR url LIKE ?1)
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            let type_str: String = row.get(2)?;
            let item_type = if type_str == "bookmark" {
                OmniboxItemType::Bookmark
            } else {
                OmniboxItemType::History
            };
            Ok(OmniboxResult {
                title: row.get(0)?,
                url: row.get(1)?,
                item_type,
            })
        })?;
        rows.collect()
    }

    fn sanitize_fts5_query(input: &str) -> String {
        let words: Vec<String> = input
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty() && *s != "AND" && *s != "OR" && *s != "NOT")
            .map(|s| format!("\"{s}\"*"))
            .collect();
        words.join(" ")
    }

    pub fn add_reading_item(
        &self,
        title: &str,
        url: &str,
        domain: &str,
        preview: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO reading_list (title, url, domain, preview_text, is_read, added_at)
            VALUES (?1, ?2, ?3, ?4, 0, ?5)
            ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                preview_text = excluded.preview_text
            "#,
            params![title, url, domain, preview, Self::now_ts()],
        )?;
        self.conn.query_row(
            "SELECT id FROM reading_list WHERE url = ?1",
            params![url],
            |row| row.get(0),
        )
    }

    pub fn get_reading_list(&self, unread_only: bool) -> Result<Vec<ReadingItem>> {
        let sql = if unread_only {
            "SELECT id, title, url, domain, preview_text, is_read, added_at, offline_path FROM reading_list WHERE is_read = 0 ORDER BY added_at DESC, id DESC"
        } else {
            "SELECT id, title, url, domain, preview_text, is_read, added_at, offline_path FROM reading_list ORDER BY added_at DESC, id DESC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(ReadingItem {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                domain: row.get(3)?,
                preview_text: row.get(4)?,
                is_read: row.get::<_, i64>(5)? != 0,
                added_at: row.get(6)?,
                offline_path: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn set_reading_item_offline_path(&self, id: i64, path: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE reading_list SET offline_path = ?2 WHERE id = ?1",
            params![id, path],
        )?;
        Ok(())
    }

    pub fn mark_reading_item_read(&self, id: i64, is_read: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE reading_list SET is_read = ?2 WHERE id = ?1",
            params![id, is_read as i64],
        )?;
        Ok(())
    }

    pub fn delete_reading_item(&self, id: i64) -> Result<Option<String>> {
        let offline_path: Option<String> = self
            .conn
            .query_row(
                "SELECT offline_path FROM reading_list WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        self.conn
            .execute("DELETE FROM reading_list WHERE id = ?1", params![id])?;
        Ok(offline_path)
    }

    pub fn record_tracker_blocked(&self, domain: &str, rule: &str) -> Result<()> {
        self.record_tracker_blocked_n(domain, rule, 1)
    }

    /// Adds `count` blocked requests for `domain` in one statement (used for batched writes).
    pub fn record_tracker_blocked_n(&self, domain: &str, rule: &str, count: u64) -> Result<()> {
        let now = Self::now_ts();
        self.conn.execute(
            r#"
            INSERT INTO tracker_stats (domain, tracker_rule, blocked_count, last_blocked_at)
            VALUES (?1, ?2, ?4, ?3)
            ON CONFLICT(domain) DO UPDATE SET
                blocked_count = tracker_stats.blocked_count + ?4,
                last_blocked_at = excluded.last_blocked_at
            "#,
            params![domain, rule, now, count as i64],
        )?;
        Ok(())
    }

    /// Runs `f` inside a single transaction (one fsync instead of one per statement).
    pub fn in_transaction<T>(&self, f: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        match f(self) {
            Ok(value) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn get_privacy_summary(&self, days: u32) -> Result<PrivacySummary> {
        let since_ts = Self::now_ts() - (days as i64 * 86400);

        let total_blocked: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(blocked_count), 0) FROM tracker_stats WHERE last_blocked_at >= ?1",
            params![since_ts],
            |row| row.get(0),
        )?;

        let unique_count: i64 = self.conn.query_row(
            "SELECT COUNT(domain) FROM tracker_stats WHERE last_blocked_at >= ?1",
            params![since_ts],
            |row| row.get(0),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT domain, blocked_count FROM tracker_stats WHERE last_blocked_at >= ?1 ORDER BY blocked_count DESC LIMIT 5",
        )?;
        let top_blocked = stmt
            .query_map(params![since_ts], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(PrivacySummary {
            total_trackers_blocked: total_blocked as u64,
            unique_trackers_count: unique_count as usize,
            days_period: days,
            top_blocked_domains: top_blocked,
        })
    }

    pub fn get_all_tracker_stats(&self) -> Result<Vec<TrackerStat>> {
        let mut stmt = self.conn.prepare(
            "SELECT domain, tracker_rule, blocked_count, last_blocked_at FROM tracker_stats ORDER BY blocked_count DESC, last_blocked_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TrackerStat {
                domain: row.get(0)?,
                tracker_rule: row.get(1)?,
                blocked_count: row.get::<_, i64>(2)? as u64,
                last_blocked_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    pub fn clear_tracker_stats(&self) -> Result<()> {
        self.conn.execute("DELETE FROM tracker_stats", [])?;
        self.conn.execute("DELETE FROM fingerprint_stats", [])?;
        let _ = self.set_setting("stats_cleared_by_user", "true");
        Ok(())
    }

    #[cfg(test)]
    pub fn record_fingerprint_defended(&self, vector: &str) -> Result<()> {
        self.record_fingerprint_defended_n(vector, 1)
    }

    pub fn record_fingerprint_defended_n(&self, vector: &str, count: u64) -> Result<()> {
        let now = Self::now_ts();
        self.conn.execute(
            r#"
            INSERT INTO fingerprint_stats (vector, defended_count, last_defended_at)
            VALUES (?1, ?3, ?2)
            ON CONFLICT(vector) DO UPDATE SET
                defended_count = fingerprint_stats.defended_count + ?3,
                last_defended_at = excluded.last_defended_at
            "#,
            params![vector, now, count as i64],
        )?;
        Ok(())
    }

    pub fn get_fingerprint_stats(&self) -> Result<Vec<FingerprintStat>> {
        let mut stmt = self.conn.prepare(
            "SELECT vector, defended_count, last_defended_at FROM fingerprint_stats ORDER BY defended_count DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FingerprintStat {
                vector: row.get(0)?,
                defended_count: row.get::<_, i64>(1)? as u64,
                last_defended_at: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn get_all_settings(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM app_settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (k, v) = r?;
            map.insert(k, v);
        }
        Ok(map)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO app_settings (key, value)
            VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_auto_translate_languages(
        &self,
    ) -> Result<std::collections::HashMap<String, String>> {
        if let Some(json_str) = self.get_setting("translate_auto_languages")? {
            if let Ok(map) =
                serde_json::from_str::<std::collections::HashMap<String, String>>(&json_str)
            {
                return Ok(map);
            }
        }
        Ok(std::collections::HashMap::new())
    }

    pub fn is_auto_translate_enabled(&self, source_lang: &str) -> Option<String> {
        let global_enabled = self
            .get_setting("translate_auto_enabled")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);

        let target_lang = self
            .get_setting("translate_auto_target_lang")
            .ok()
            .flatten()
            .unwrap_or_else(|| "ru".to_string());

        if global_enabled && source_lang != target_lang {
            return Some(target_lang);
        }

        let map = self.get_auto_translate_languages().ok()?;
        if let Some(target) = map.get(source_lang) {
            if source_lang != target {
                return Some(target.clone());
            }
        }
        if let Some(target) = map.get("auto") {
            if source_lang != target {
                return Some(target.clone());
            }
        }

        None
    }

    pub fn is_any_auto_translate_enabled(&self) -> bool {
        if let Ok(Some(v)) = self.get_setting("translate_auto_enabled") {
            if v == "true" {
                return true;
            }
        }
        if let Ok(map) = self.get_auto_translate_languages() {
            if !map.is_empty() {
                return true;
            }
        }
        false
    }

    pub fn set_auto_translate_language(
        &self,
        source_lang: &str,
        target_lang: Option<&str>,
    ) -> Result<()> {
        let mut map = self.get_auto_translate_languages().unwrap_or_default();
        if let Some(tgt) = target_lang {
            map.insert(source_lang.to_string(), tgt.to_string());
        } else {
            map.remove(source_lang);
        }
        let json_str = serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string());
        self.set_setting("translate_auto_languages", &json_str)?;
        Ok(())
    }

    pub fn get_site_permission(&self, origin: &str, perm_type: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT state FROM site_permissions WHERE origin = ?1 AND permission_type = ?2",
                params![origin, perm_type],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn set_site_permission(&self, origin: &str, perm_type: &str, state: &str) -> Result<()> {
        let now = Self::now_ts();
        self.conn.execute(
            r#"
            INSERT INTO site_permissions (origin, permission_type, state, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(origin, permission_type) DO UPDATE SET
                state = excluded.state,
                updated_at = excluded.updated_at
            "#,
            params![origin, perm_type, state, now],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn delete_site_permission(&self, origin: &str, perm_type: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM site_permissions WHERE origin = ?1 AND permission_type = ?2",
            params![origin, perm_type],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn get_all_site_permissions(&self) -> Result<Vec<SitePermissionEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT origin, permission_type, state, updated_at FROM site_permissions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SitePermissionEntry {
                origin: row.get(0)?,
                permission_type: row.get(1)?,
                state: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;
        let mut entries = Vec::new();
        for r in rows {
            entries.push(r?);
        }
        Ok(entries)
    }

    pub fn clear_site_permissions(&self) -> Result<()> {
        self.conn.execute("DELETE FROM site_permissions", [])?;
        Ok(())
    }

    pub fn is_protection_exception(&self, domain: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(1) FROM protection_exceptions WHERE domain = ?1",
            params![domain],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn set_protection_exception(&self, domain: &str, is_exception: bool) -> Result<()> {
        if is_exception {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            self.conn.execute(
                "INSERT OR REPLACE INTO protection_exceptions (domain, created_at) VALUES (?1, ?2)",
                params![domain, now],
            )?;
        } else {
            self.conn.execute(
                "DELETE FROM protection_exceptions WHERE domain = ?1",
                params![domain],
            )?;
        }
        Ok(())
    }

    pub fn set_domain_zoom(&self, domain: &str, zoom: f64) -> Result<()> {
        let domain_lower = domain.trim().to_lowercase();
        self.conn.execute(
            "INSERT INTO domain_zoom (domain, zoom) VALUES (?1, ?2)
             ON CONFLICT(domain) DO UPDATE SET zoom = excluded.zoom",
            params![domain_lower, zoom],
        )?;
        Ok(())
    }

    pub fn get_domain_zoom(&self, domain: &str) -> Result<Option<f64>> {
        let domain_lower = domain.trim().to_lowercase();
        let mut stmt = self
            .conn
            .prepare("SELECT zoom FROM domain_zoom WHERE domain = ?1")?;
        let mut rows = stmt.query(params![domain_lower])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    #[cfg(test)]
    pub fn clear_domain_zoom(&self) -> Result<()> {
        self.conn.execute("DELETE FROM domain_zoom", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_zoom_persistence_lifecycle() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        assert_eq!(db.get_domain_zoom("example.com")?, None);

        db.set_domain_zoom("example.com", 1.25)?;
        assert_eq!(db.get_domain_zoom("example.com")?, Some(1.25));

        // Case-insensitivity check
        assert_eq!(db.get_domain_zoom("EXAMPLE.COM")?, Some(1.25));

        // Update zoom
        db.set_domain_zoom("example.com", 1.50)?;
        assert_eq!(db.get_domain_zoom("example.com")?, Some(1.50));

        // Different domain
        db.set_domain_zoom("rust-lang.org", 0.9)?;
        assert_eq!(db.get_domain_zoom("rust-lang.org")?, Some(0.9));
        assert_eq!(db.get_domain_zoom("example.com")?, Some(1.50));

        db.clear_domain_zoom()?;
        assert_eq!(db.get_domain_zoom("example.com")?, None);
        assert_eq!(db.get_domain_zoom("rust-lang.org")?, None);
        Ok(())
    }

    #[test]
    fn app_settings_storage_and_update() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        assert_eq!(db.get_setting("theme")?, None);

        db.set_setting("theme", "light")?;
        assert_eq!(db.get_setting("theme")?, Some("light".to_string()));

        db.set_setting("theme", "dark")?;
        assert_eq!(db.get_setting("theme")?, Some("dark".to_string()));
        Ok(())
    }

    #[test]
    fn history_visits_are_merged_by_url() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        db.record_history_visit("Example Site", "https://example.com")?;
        db.record_history_visit("Example Site 2", "https://example.com")?;

        let history = db.get_recent_history(10)?;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].visit_count, 2);
        assert_eq!(history[0].title, "Example Site 2");
        Ok(())
    }

    #[test]
    fn tracker_summary_counts_domains() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        db.record_tracker_blocked("google-analytics.com", "EasyPrivacy")?;
        db.record_tracker_blocked("facebook.net", "EasyPrivacy")?;
        db.record_tracker_blocked("facebook.net", "EasyPrivacy")?;

        let privacy = db.get_privacy_summary(7)?;
        assert_eq!(privacy.total_trackers_blocked, 3);
        assert_eq!(privacy.unique_trackers_count, 2);
        assert_eq!(privacy.top_blocked_domains[0].0, "facebook.net");

        let all_stats = db.get_all_tracker_stats()?;
        assert_eq!(all_stats.len(), 2);

        db.clear_tracker_stats()?;
        assert!(db.get_all_tracker_stats()?.is_empty());
        assert_eq!(db.get_privacy_summary(7)?.total_trackers_blocked, 0);
        Ok(())
    }

    #[test]
    fn fingerprint_stats_lifecycle() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        db.seed_defaults_once()?;
        // Verify default seeding does NOT insert fake trackers or fake fingerprint data
        assert!(db.get_all_tracker_stats()?.is_empty());
        assert!(db.get_fingerprint_stats()?.is_empty());

        db.record_fingerprint_defended("canvas")?;
        db.record_fingerprint_defended_n("canvas", 4)?;
        db.record_fingerprint_defended_n("webgl", 3)?;

        let stats = db.get_fingerprint_stats()?;
        assert_eq!(stats.len(), 2);
        let canvas = stats.iter().find(|s| s.vector == "canvas").unwrap();
        assert_eq!(canvas.defended_count, 5);
        let webgl = stats.iter().find(|s| s.vector == "webgl").unwrap();
        assert_eq!(webgl.defended_count, 3);

        db.clear_tracker_stats()?;
        assert!(db.get_fingerprint_stats()?.is_empty());
        Ok(())
    }

    #[test]
    fn bookmarks_crud_and_folder_filter() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        let apple = db.add_bookmark("Apple", "https://apple.com", Some("Избранное"))?;
        db.add_bookmark("Rust", "https://rust-lang.org", None)?;

        assert_eq!(db.get_bookmarks(None)?.len(), 2);
        let favorites = db.get_bookmarks(Some("Избранное"))?;
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].id, apple);

        let same = db.add_bookmark("Apple Inc.", "https://apple.com", Some("Избранное"))?;
        assert_eq!(same, apple);
        assert_eq!(db.get_bookmarks(None)?.len(), 2);

        assert!(db.is_bookmarked("https://apple.com")?);
        db.delete_bookmark(apple)?;
        assert!(!db.is_bookmarked("https://apple.com")?);
        db.delete_bookmark_by_url("https://rust-lang.org")?;
        assert!(db.get_bookmarks(None)?.is_empty());
        Ok(())
    }

    #[test]
    fn reading_list_unread_filter_and_cleanup() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        let first = db.add_reading_item("A", "https://a.test/post", "a.test", Some("preview"))?;
        db.add_reading_item("B", "https://b.test/post", "b.test", None)?;
        assert_eq!(db.get_reading_list(false)?.len(), 2);

        db.mark_reading_item_read(first, true)?;
        let unread = db.get_reading_list(true)?;
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].domain, "b.test");

        db.mark_reading_item_read(first, false)?;
        assert_eq!(db.get_reading_list(true)?.len(), 2);

        let removed_path = db.delete_reading_item(first)?;
        assert_eq!(removed_path, None);
        assert_eq!(db.get_reading_list(false)?.len(), 1);

        let remaining = &db.get_reading_list(false)?[0];
        db.set_reading_item_offline_path(remaining.id, Some("/tmp/offline.mht"))?;
        let with_offline = &db.get_reading_list(false)?[0];
        assert_eq!(
            with_offline.offline_path.as_deref(),
            Some("/tmp/offline.mht")
        );

        let deleted = db.delete_reading_item(remaining.id)?;
        assert_eq!(deleted.as_deref(), Some("/tmp/offline.mht"));
        assert_eq!(db.get_reading_list(false)?.len(), 0);
        Ok(())
    }

    #[test]
    fn tab_groups_store_ordered_tabs_and_cascade_on_delete() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        db.create_tab_group("work", "Работа", None, Some("blue"))?;
        db.create_tab_group("home", "Личное", Some("go-home-symbolic"), None)?;

        let tabs = vec![
            ("Docs".to_string(), "https://docs.rs".to_string(), true),
            ("Crates".to_string(), "https://crates.io".to_string(), false),
        ];
        db.save_group_tabs("work", &tabs)?;

        let stored = db.get_group_tabs("work")?;
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].url, "https://docs.rs");
        assert!(stored[0].is_pinned);
        assert_eq!(stored[1].position, 1);

        db.save_group_tabs("work", &tabs[1..])?;
        assert_eq!(db.get_group_tabs("work")?.len(), 1);

        let new_tab_id = db.add_tab_to_group("work", "GitHub", "https://github.com", false)?;
        let updated = db.get_group_tabs("work")?;
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[1].title, "GitHub");
        assert_eq!(updated[1].url, "https://github.com");

        db.delete_tab_from_group(new_tab_id)?;
        assert_eq!(db.get_group_tabs("work")?.len(), 1);

        db.rename_tab_group("work", "Проекты")?;
        let groups = db.get_tab_groups()?;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "Проекты");

        db.delete_tab_group("work")?;
        assert!(db.get_group_tabs("work")?.is_empty());
        assert_eq!(db.get_tab_groups()?.len(), 1);
        Ok(())
    }

    #[test]
    fn default_favorites_are_seeded_only_once() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        db.seed_defaults_once()?;
        let favorites = db.get_bookmarks(Some(FAVORITES_FOLDER))?;
        assert_eq!(favorites.len(), DEFAULT_FAVORITES.len());

        for bookmark in favorites {
            db.delete_bookmark(bookmark.id)?;
        }
        db.seed_defaults_once()?;
        assert!(db.get_bookmarks(Some(FAVORITES_FOLDER))?.is_empty());
        Ok(())
    }

    #[test]
    fn omnibox_fts_search_finds_bookmarks_and_history() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        db.add_bookmark(
            "Rust Programming Language",
            "https://www.rust-lang.org",
            None,
        )?;
        db.add_bookmark(
            "GitHub Repository",
            "https://github.com/rust-lang/rust",
            None,
        )?;
        db.record_history_visit("Arch Linux Documentation", "https://wiki.archlinux.org")?;
        db.record_history_visit("Hacker News", "https://news.ycombinator.com")?;

        // 1. Search for "rust" should find both bookmark entries
        let results = db.search_omnibox("rust", 10)?;
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|r| r.item_type == OmniboxItemType::Bookmark));

        // 2. Search for "arch" should find history entry
        let results = db.search_omnibox("arch", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Arch Linux Documentation");
        assert_eq!(results[0].item_type, OmniboxItemType::History);

        // 3. Search for "ycombinator" should find by partial URL
        let results = db.search_omnibox("ycombinator", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Hacker News");

        // 4. Empty query returns recent items
        let all = db.search_omnibox("", 10)?;
        assert_eq!(all.len(), 4);

        Ok(())
    }

    #[test]
    fn omnibox_fts_sanitization_and_deduplication() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        // Same URL in both history and bookmark: bookmark should be prioritized
        db.record_history_visit("Old History Title", "https://example.com/item")?;
        db.add_bookmark("Starred Item Title", "https://example.com/item", None)?;

        let results = db.search_omnibox("item", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Starred Item Title");
        assert_eq!(results[0].item_type, OmniboxItemType::Bookmark);

        // Special characters should not crash FTS5
        let safe = db.search_omnibox("item: \"*() AND OR NOT", 10)?;
        assert_eq!(safe.len(), 1);

        Ok(())
    }

    #[test]
    fn history_clear_and_search() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        db.record_history_visit("First Page", "https://first.org")?;
        db.record_history_visit("Second Page", "https://second.org")?;
        db.record_history_visit("Third Page", "https://third.org")?;

        let recent = db.get_recent_history(10)?;
        assert_eq!(recent.len(), 3);

        let search_res = db.search_history("second", 10)?;
        assert_eq!(search_res.len(), 1);
        assert_eq!(search_res[0].url, "https://second.org");

        db.clear_history()?;
        let after_clear = db.get_recent_history(10)?;
        assert!(after_clear.is_empty());

        let fts_after_clear = db.search_omnibox("first", 10)?;
        assert!(fts_after_clear.is_empty());

        Ok(())
    }

    #[test]
    fn site_permissions_lifecycle() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        let origin = "https://example.com";
        let perm = "camera";

        // 1. Initial state is None
        assert_eq!(db.get_site_permission(origin, perm)?, None);

        // 2. Set to allow
        db.set_site_permission(origin, perm, "allow")?;
        assert_eq!(
            db.get_site_permission(origin, perm)?,
            Some("allow".to_string())
        );

        // 3. Update to deny
        db.set_site_permission(origin, perm, "deny")?;
        assert_eq!(
            db.get_site_permission(origin, perm)?,
            Some("deny".to_string())
        );

        // 4. Add another permission
        db.set_site_permission(origin, "microphone", "allow")?;
        let all = db.get_all_site_permissions()?;
        assert_eq!(all.len(), 2);

        // 5. Delete specific permission
        db.delete_site_permission(origin, perm)?;
        assert_eq!(db.get_site_permission(origin, perm)?, None);
        assert_eq!(
            db.get_site_permission(origin, "microphone")?,
            Some("allow".to_string())
        );

        // 6. Clear all permissions
        db.clear_site_permissions()?;
        assert!(db.get_all_site_permissions()?.is_empty());

        Ok(())
    }

    #[test]
    fn protection_exceptions_lifecycle() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        let domain = "deepseek.com";

        assert!(!db.is_protection_exception(domain)?);

        db.set_protection_exception(domain, true)?;
        assert!(db.is_protection_exception(domain)?);

        db.set_protection_exception(domain, false)?;
        assert!(!db.is_protection_exception(domain)?);

        Ok(())
    }

    #[test]
    fn auto_translate_languages_lifecycle() -> Result<()> {
        let db = BrowserDatabase::in_memory()?;
        assert!(db.get_auto_translate_languages()?.is_empty());
        assert_eq!(db.is_auto_translate_enabled("en"), None);

        db.set_auto_translate_language("en", Some("ru"))?;
        assert_eq!(db.is_auto_translate_enabled("en"), Some("ru".to_string()));
        assert_eq!(db.is_auto_translate_enabled("de"), None);

        db.set_auto_translate_language("de", Some("ru"))?;
        let all = db.get_auto_translate_languages()?;
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("en").map(String::as_str), Some("ru"));
        assert_eq!(all.get("de").map(String::as_str), Some("ru"));

        db.set_auto_translate_language("en", None)?;
        assert_eq!(db.is_auto_translate_enabled("en"), None);
        assert_eq!(db.is_auto_translate_enabled("de"), Some("ru".to_string()));

        // Test global enable
        db.set_auto_translate_language("de", None)?;
        assert!(!db.is_any_auto_translate_enabled());

        db.set_setting("translate_auto_enabled", "true")?;
        assert!(db.is_any_auto_translate_enabled());
        assert_eq!(db.is_auto_translate_enabled("en"), Some("ru".to_string()));
        assert_eq!(db.is_auto_translate_enabled("fr"), Some("ru".to_string()));
        assert_eq!(db.is_auto_translate_enabled("ru"), None);

        Ok(())
    }
}
