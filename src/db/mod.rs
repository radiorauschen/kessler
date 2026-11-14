pub mod loudness;
pub mod models;

use std::path::Path;

use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use models::*;

const SEARCH_TEXT_VERSION: &str = "3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlbumOrder {
    Alphabetical,
    Added,
}

#[derive(Debug, Clone)]
pub struct ScanState {
    pub modified: String,
    pub artist_key: String,
    pub album_key: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    #[allow(dead_code)]
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS tracks (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                path            TEXT NOT NULL UNIQUE,
                title           TEXT NOT NULL DEFAULT '',
                artist          TEXT NOT NULL DEFAULT '',
                album_artist    TEXT NOT NULL DEFAULT '',
                album           TEXT NOT NULL DEFAULT '',
                genre           TEXT NOT NULL DEFAULT '',
                year            INTEGER,
                track_number    INTEGER,
                disc_number     INTEGER,
                duration_secs   REAL NOT NULL DEFAULT 0.0,
                bitrate         INTEGER,
                sample_rate     INTEGER,
                file_size       INTEGER NOT NULL DEFAULT 0,
                format          TEXT NOT NULL DEFAULT '',
                rating          INTEGER NOT NULL DEFAULT 0,
                play_count      INTEGER NOT NULL DEFAULT 0,
                last_played     TEXT,
                date_added      TEXT NOT NULL,
                date_modified   TEXT NOT NULL,
                file_hash       TEXT,
                has_artwork     INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_tracks_album_artist ON tracks(album_artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);
            CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);

            CREATE TABLE IF NOT EXISTS playlists (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                name            TEXT NOT NULL,
                is_smart        INTEGER NOT NULL DEFAULT 0,
                smart_rules     TEXT,
                date_created    TEXT NOT NULL,
                date_modified   TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS playlist_tracks (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                playlist_id     INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                track_id        INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                position        INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_playlist_tracks_playlist
                ON playlist_tracks(playlist_id);

            CREATE TABLE IF NOT EXISTS meta (
                key             TEXT PRIMARY KEY,
                value           TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS play_history (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id        INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                played_at       TEXT NOT NULL,
                duration_played REAL NOT NULL DEFAULT 0.0
            );
            ",
        )?;
        self.add_search_column()?;
        self.add_loudness_table()?;
        self.date_tracks_by_their_files()?;
        self.one_row_per_file()?;
        Ok(())
    }

    fn one_row_per_file(&self) -> Result<()> {
        const VERSION: &str = "1";
        let stamped: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'one_row_per_file'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if stamped.as_deref() == Some(VERSION) {
            return Ok(());
        }

        let rows: Vec<(i64, String)> = self
            .conn
            .prepare("SELECT id, path FROM tracks ORDER BY id")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut seen = crate::library::DiskNames::new();
        let mut every_file_answered = true;
        for (id, path) in rows {
            let at = std::path::Path::new(&path);
            if !at.exists() {
                every_file_answered = false;
                continue;
            }
            let real = crate::library::as_the_disk_spells_it(
                &crate::library::one_name_for(at),
                &mut seen,
            );
            let real = real.to_string_lossy().to_string();
            if real == path {
                continue;
            }
            let keeper: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM tracks WHERE path = ?1 AND id != ?2",
                    params![real, id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(keeper) = keeper else {
                self.conn
                    .execute("UPDATE tracks SET path = ?1 WHERE id = ?2", params![real, id])?;
                continue;
            };
            self.conn.execute(
                "UPDATE tracks SET
                    play_count  = MAX(play_count, (SELECT play_count FROM tracks WHERE id = ?2)),
                    rating      = MAX(rating,     (SELECT rating     FROM tracks WHERE id = ?2)),
                    last_played = MAX(
                        IFNULL(last_played, ''),
                        IFNULL((SELECT last_played FROM tracks WHERE id = ?2), '')),
                    date_added  = MIN(date_added, (SELECT date_added FROM tracks WHERE id = ?2))
                 WHERE id = ?1",
                params![keeper, id],
            )?;
            self.conn.execute(
                "UPDATE play_history SET track_id = ?1 WHERE track_id = ?2",
                params![keeper, id],
            )?;
            self.conn.execute(
                "UPDATE OR IGNORE playlist_tracks SET track_id = ?1 WHERE track_id = ?2",
                params![keeper, id],
            )?;
            self.conn
                .execute("DELETE FROM tracks WHERE id = ?1", params![id])?;
        }

        self.conn.execute(
            "UPDATE tracks SET last_played = NULL WHERE last_played = ''",
            [],
        )?;
        if every_file_answered {
            self.conn.execute(
                "INSERT INTO meta (key, value) VALUES ('one_row_per_file', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![VERSION],
            )?;
        }
        Ok(())
    }

    fn date_tracks_by_their_files(&self) -> Result<()> {
        let done: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'date_added_from_files'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if done.is_some() {
            return Ok(());
        }

        self.conn.execute_batch(
            "UPDATE tracks
                SET date_added = MIN(date_added, date_modified)
              WHERE date_modified != '' AND date_modified < date_added;
             INSERT INTO meta (key, value) VALUES ('date_added_from_files', '1')
                ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
        )?;
        Ok(())
    }

    fn add_search_column(&self) -> Result<()> {
        for column in ["search_text", "artist_key", "album_key"] {
            let present: bool = self
                .conn
                .prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1")?
                .exists(params![column])?;
            if !present {
                self.conn.execute_batch(&format!(
                    "ALTER TABLE tracks ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"
                ))?;
            }
        }
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_tracks_artist_key ON tracks(artist_key);
             CREATE INDEX IF NOT EXISTS idx_tracks_album_key ON tracks(album_key);",
        )?;

        let stamped: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'search_text_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if stamped.as_deref() == Some(SEARCH_TEXT_VERSION) {
            return Ok(());
        }

        self.rebuild_search_text()?;
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('search_text_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SEARCH_TEXT_VERSION],
        )?;
        Ok(())
    }

    fn rebuild_search_text(&self) -> Result<()> {
        let rows: Vec<(i64, String, String, String, String)> = self
            .conn
            .prepare("SELECT id, title, artist, album_artist, album FROM tracks")?
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut stmt = self.conn.prepare(
            "UPDATE tracks
             SET search_text = ?1, artist_key = ?2, album_key = ?3
             WHERE id = ?4",
        )?;
        for (id, title, artist, album_artist, album) in rows {
            let display = if album_artist.is_empty() {
                &artist
            } else {
                &album_artist
            };
            stmt.execute(params![
                search_text(&title, &artist, &album_artist, &album),
                sort_key(display),
                sort_key(&album),
                id
            ])?;
        }
        Ok(())
    }

    pub fn upsert_track(&self, track: &Track) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tracks (
                path, title, artist, album_artist, album, genre, year,
                track_number, disc_number, duration_secs, bitrate, sample_rate,
                file_size, format, rating, play_count, last_played,
                date_added, date_modified, file_hash, has_artwork, search_text,
                artist_key, album_key
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17,
                ?18, ?19, ?20, ?21, ?22, ?23, ?24
            )
            ON CONFLICT(path) DO UPDATE SET
                title = excluded.title,
                artist = excluded.artist,
                album_artist = excluded.album_artist,
                album = excluded.album,
                genre = excluded.genre,
                year = excluded.year,
                track_number = excluded.track_number,
                disc_number = excluded.disc_number,
                duration_secs = excluded.duration_secs,
                bitrate = excluded.bitrate,
                sample_rate = excluded.sample_rate,
                file_size = excluded.file_size,
                format = excluded.format,
                date_modified = excluded.date_modified,
                file_hash = excluded.file_hash,
                has_artwork = excluded.has_artwork,
                search_text = excluded.search_text,
                artist_key = excluded.artist_key,
                album_key = excluded.album_key",
            params![
                track.path.to_string_lossy().to_string(),
                track.title,
                track.artist,
                track.album_artist,
                track.album,
                track.genre,
                track.year,
                track.track_number,
                track.disc_number,
                track.duration_secs,
                track.bitrate,
                track.sample_rate,
                track.file_size as i64,
                track.format,
                track.rating,
                track.play_count,
                track.last_played.map(|dt| dt.to_string()),
                track.date_added.to_string(),
                track.date_modified.to_string(),
                track.file_hash,
                track.has_artwork as i32,
                search_text(
                    &track.title,
                    &track.artist,
                    &track.album_artist,
                    &track.album,
                ),
                sort_key(grouping_artist(track)),
                sort_key(&track.album),
            ],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    pub fn get_track_by_path(&self, path: &str) -> Result<Option<Track>> {
        let track = self
            .conn
            .query_row(
                "SELECT * FROM tracks WHERE path = ?1",
                params![path],
                row_to_track,
            )
            .optional()?;
        Ok(track)
    }

    pub fn get_track(&self, id: i64) -> Result<Option<Track>> {
        let track = self
            .conn
            .query_row("SELECT * FROM tracks WHERE id = ?1", params![id], |row| {
                row_to_track(row)
            })
            .optional()?;
        Ok(track)
    }

    pub fn remove_tracks_under(&self, folder: &Path) -> Result<u32> {
        let mut prefix = folder.to_string_lossy().to_string();
        if !prefix.ends_with(std::path::MAIN_SEPARATOR) {
            prefix.push(std::path::MAIN_SEPARATOR);
        }
        let pattern = format!(
            "{}%",
            prefix
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let removed = self.conn.execute(
            "DELETE FROM tracks WHERE path LIKE ?1 ESCAPE '\\'",
            params![pattern],
        )?;
        Ok(removed as u32)
    }

    pub fn remove_missing_tracks(&self) -> Result<u32> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM tracks")?;
        let missing: Vec<i64> = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let path: String = row.get(1)?;
                Ok((id, path))
            })?
            .filter_map(|r| r.ok())
            .filter(|(_, path)| !Path::new(path).exists())
            .map(|(id, _)| id)
            .collect();

        let count = missing.len() as u32;
        for id in &missing {
            self.conn
                .execute("DELETE FROM tracks WHERE id = ?1", params![id])?;
        }
        Ok(count)
    }

    pub fn move_track_path(&self, id: i64, path: &Path) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET path = ?1 WHERE id = ?2",
            params![path.to_string_lossy(), id],
        )?;
        Ok(())
    }

    pub fn remove_missing_under(&self, folder: &Path) -> Result<u32> {
        let mut prefix = folder.to_string_lossy().to_string();
        if !prefix.ends_with(std::path::MAIN_SEPARATOR) {
            prefix.push(std::path::MAIN_SEPARATOR);
        }
        let mut stmt = self.conn.prepare("SELECT id, path FROM tracks")?;
        let missing: Vec<i64> = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .filter(|(_, path)| path.starts_with(&prefix) && !Path::new(path).exists())
            .map(|(id, _)| id)
            .collect();
        for id in &missing {
            self.conn
                .execute("DELETE FROM tracks WHERE id = ?1", params![id])?;
        }
        Ok(missing.len() as u32)
    }

    pub fn scan_state(&self, path: &str) -> Result<Option<ScanState>> {
        let result = self
            .conn
            .query_row(
                "SELECT date_modified, artist_key, album_key FROM tracks WHERE path = ?1",
                params![path],
                |row| {
                    Ok(ScanState {
                        modified: row.get(0)?,
                        artist_key: row.get(1)?,
                        album_key: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    pub fn get_artists(&self, filter: Option<&str>) -> Result<Vec<Artist>> {
        self.artists_ordered(filter, false)
    }

    pub fn get_artists_recent(&self, filter: Option<&str>) -> Result<Vec<Artist>> {
        self.artists_ordered(filter, true)
    }

    fn artists_ordered(&self, filter: Option<&str>, recent: bool) -> Result<Vec<Artist>> {
        let patterns = filter_patterns(filter);
        let sql = format!(
            "SELECT
                (SELECT CASE WHEN t2.album_artist != '' THEN t2.album_artist ELSE t2.artist END
                 FROM tracks t2
                 WHERE t2.artist_key = t.artist_key
                 GROUP BY 1
                 ORDER BY COUNT(*) DESC, 1 ASC
                 LIMIT 1) AS display_artist,
                COUNT(DISTINCT t.album_key) AS album_count,
                COUNT(*) AS track_count
             FROM tracks t
             {}
             GROUP BY t.artist_key
             ORDER BY {}",
            filter_where(&patterns, 1),
            if recent {
                "MAX(t.date_added) DESC, display_artist COLLATE NOCASE ASC"
            } else {
                "(t.artist_key = '') ASC, display_artist COLLATE NOCASE ASC"
            }
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let artists = stmt
            .query_map(rusqlite::params_from_iter(patterns.iter()), |row| {
                Ok(Artist {
                    name: row.get(0)?,
                    album_count: row.get(1)?,
                    track_count: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok());
        let mut artists: Vec<Artist> = artists.collect();
        if !recent {
            artists.sort_by_cached_key(|a| (shelf_key(&a.name), a.name.to_lowercase()));
        }
        Ok(artists)
    }

    pub fn get_albums(&self, artist: Option<&str>, filter: Option<&str>) -> Result<Vec<Album>> {
        self.albums_ordered(artist, filter, AlbumOrder::Alphabetical)
    }

    pub fn get_albums_recent(
        &self,
        artist: Option<&str>,
        filter: Option<&str>,
    ) -> Result<Vec<Album>> {
        self.albums_ordered(artist, filter, AlbumOrder::Added)
    }

    fn albums_ordered(
        &self,
        artist: Option<&str>,
        filter: Option<&str>,
        order: AlbumOrder,
    ) -> Result<Vec<Album>> {
        let mut bound: Vec<String> = Vec::new();
        let artist_clause = match artist {
            Some(a) => {
                bound.push(sort_key(a));
                "WHERE t.artist_key = ?1".to_string()
            }
            None => String::new(),
        };

        let patterns = filter_patterns(filter);
        let filter_clause = filter_conditions(&patterns, bound.len() + 1);
        bound.extend(patterns);

        let where_clause = match (artist_clause.is_empty(), filter_clause.is_empty()) {
            (true, true) => String::new(),
            (true, false) => format!("WHERE {filter_clause}"),
            (false, true) => artist_clause,
            (false, false) => format!("{artist_clause} AND {filter_clause}"),
        };

        let sql = format!(
            "SELECT
                (SELECT t2.album FROM tracks t2
                 WHERE t2.album_key = t.album_key AND t2.artist_key = t.artist_key
                 GROUP BY 1 ORDER BY COUNT(*) DESC, 1 ASC LIMIT 1) AS album_name,
                (SELECT CASE WHEN t2.album_artist != '' THEN t2.album_artist ELSE t2.artist END
                 FROM tracks t2
                 WHERE t2.artist_key = t.artist_key
                 GROUP BY 1 ORDER BY COUNT(*) DESC, 1 ASC LIMIT 1) AS aa,
                MIN(t.year), COUNT(*), SUM(t.duration_secs), MAX(t.has_artwork),
                MAX(t.date_added) AS added
             FROM tracks t
             {where_clause}
             GROUP BY t.album_key, t.artist_key
             ORDER BY {}",
            match order {
                AlbumOrder::Alphabetical if artist.is_some() =>
                    "(t.album_key = '') ASC, MIN(t.year) ASC, album_name COLLATE NOCASE ASC",
                AlbumOrder::Alphabetical =>
                    "(t.album_key = '') ASC, album_name COLLATE NOCASE ASC",
                AlbumOrder::Added =>
                    "MAX(t.date_added) DESC, album_name COLLATE NOCASE ASC",
            }
        );

        let mut stmt = self.conn.prepare(&sql)?;

        let rows = stmt.query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            Ok(Album {
                name: row.get(0)?,
                artist: row.get(1)?,
                year: row.get(2)?,
                track_count: row.get(3)?,
                total_duration_secs: row.get(4)?,
                has_artwork: row.get::<_, i32>(5)? != 0,
                added: row.get(6)?,
            })
        })?;

        let albums = rows.filter_map(|r| r.ok()).collect();
        Ok(albums)
    }

    pub fn get_tracks(
        &self,
        artist: Option<&str>,
        album: Option<&str>,
        sort: SortField,
        order: SortOrder,
        filter: Option<&str>,
    ) -> Result<Vec<Track>> {
        let mut conditions = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        if let Some(a) = artist {
            conditions.push("artist_key = ?".to_string());
            param_values.push(sort_key(a));
        }
        if let Some(al) = album {
            conditions.push("album_key = ?".to_string());
            param_values.push(sort_key(al));
        }
        for pattern in filter_patterns(filter) {
            conditions.push("search_text LIKE ? ESCAPE '\\'".to_string());
            param_values.push(pattern);
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            let numbered: Vec<String> = conditions
                .iter()
                .enumerate()
                .map(|(i, c)| c.replacen('?', &format!("?{}", i + 1), 1))
                .collect();
            format!("WHERE {}", numbered.join(" AND "))
        };

        let rest = if matches!(sort, SortField::Artist) {
            order.sql()
        } else {
            "ASC"
        };
        let sql = format!(
            "SELECT * FROM tracks {} ORDER BY {} {}, \
             artist_key COLLATE NOCASE {rest}, \
             (SELECT MIN(r.year) FROM tracks r \
               WHERE r.artist_key = tracks.artist_key AND r.album_key = tracks.album_key) {rest}, \
             album_key COLLATE NOCASE {rest}, disc_number {rest}, \
             track_number {rest}, title COLLATE NOCASE {rest}",
            where_clause,
            sort.sql_column(),
            order.sql()
        );

        let mut stmt = self.conn.prepare(&sql)?;

        let mut tracks: Vec<Track> = stmt
            .query_map(rusqlite::params_from_iter(param_values.iter()), row_to_track)?
            .filter_map(|r| r.ok())
            .collect();
        if matches!(sort, SortField::Artist) {
            let rank = |t: &Track| shelf_key(grouping_artist(t));
            match order {
                SortOrder::Descending => tracks.sort_by_cached_key(|t| std::cmp::Reverse(rank(t))),
                SortOrder::Ascending => tracks.sort_by_cached_key(rank),
            }
        }

        Ok(tracks)
    }

    #[cfg(test)]
    pub fn search_tracks(&self, query: &str) -> Result<Vec<Track>> {
        let patterns = filter_patterns(Some(query));
        if patterns.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT * FROM tracks
             {}
             ORDER BY artist COLLATE NOCASE, album COLLATE NOCASE, disc_number, track_number
             LIMIT 200",
            filter_where(&patterns, 1)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let tracks = stmt
            .query_map(rusqlite::params_from_iter(patterns.iter()), row_to_track)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tracks)
    }

    pub fn track_count(&self) -> Result<u32> {
        self.track_count_matching(None)
    }

    pub fn library_totals(&self) -> Result<(f64, u64)> {
        let totals = self.conn.query_row(
            "SELECT COALESCE(SUM(duration_secs), 0.0), COALESCE(SUM(file_size), 0) FROM tracks",
            [],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, i64>(1)?.max(0) as u64)),
        )?;
        Ok(totals)
    }

    pub fn compilation_album_keys(&self, artists: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT album_key FROM tracks
             WHERE album_artist = '' AND album != ''
             GROUP BY album_key HAVING COUNT(DISTINCT artist_key) >= ?1
             ORDER BY album_key",
        )?;
        let keys = stmt
            .query_map(params![artists as i64], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(keys)
    }

    pub fn track_count_matching(&self, filter: Option<&str>) -> Result<u32> {
        let patterns = filter_patterns(filter);
        let sql = format!(
            "SELECT COUNT(*) FROM tracks {}",
            filter_where(&patterns, 1)
        );
        let count: u32 = self.conn.query_row(
            &sql,
            rusqlite::params_from_iter(patterns.iter()),
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn heard_before(&self) -> Result<f64> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(SUM(play_count * duration_secs), 0.0) FROM tracks", [], |r| r.get::<_, f64>(0))?)
    }

    pub fn record_play(&self, track_id: i64) -> Result<()> {
        if track_id < 0 {
            return Ok(());
        }
        let now = chrono::Local::now().naive_local().to_string();
        self.conn.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played = ?1 WHERE id = ?2",
            params![now, track_id],
        )?;
        self.conn.execute(
            "INSERT INTO play_history (track_id, played_at, duration_played) VALUES (?1, ?2, 0)",
            params![track_id, now],
        )?;
        Ok(())
    }

    pub fn add_plays(&self, track_id: i64, plays: u32, last: chrono::NaiveDateTime, seconds: f64) -> Result<()> {
        let last_text = last.to_string();
        self.conn.execute(
            "UPDATE tracks SET play_count = play_count + ?1,
                 last_played = CASE WHEN last_played IS NULL OR last_played < ?2 THEN ?2 ELSE last_played END
             WHERE id = ?3",
            params![plays, last_text, track_id],
        )?;
        let step = chrono::Duration::milliseconds((seconds.max(0.0) * 1000.0) as i64);
        for k in 0..plays {
            let at = last - step * k as i32;
            self.conn.execute(
                "INSERT INTO play_history (track_id, played_at, duration_played) VALUES (?1, ?2, 0)",
                params![track_id, at.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn set_rating(&self, track_id: i64, stars: u8) -> Result<()> {
        self.conn
            .execute("UPDATE tracks SET rating = ?1 WHERE id = ?2", params![stars.min(5), track_id])?;
        Ok(())
    }

    pub fn get_recent_tracks(&self, filter: Option<&str>, limit: usize) -> Result<Vec<Track>> {
        let patterns = filter_patterns(filter);
        let mut where_clause = String::new();
        for (i, _) in patterns.iter().enumerate() {
            where_clause.push_str(if i == 0 { "WHERE " } else { " AND " });
            where_clause.push_str(&format!("search_text LIKE ?{} ESCAPE '\\'", i + 1));
        }

        let sql = format!(
            "SELECT * FROM tracks {where_clause} \
             ORDER BY date_added DESC, id DESC LIMIT {}",
            limit.max(1)
        );

        let params: Vec<&dyn rusqlite::ToSql> =
            patterns.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let tracks = stmt
            .query_map(params.as_slice(), row_to_track)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tracks)
    }

    pub fn get_playlists(&self) -> Result<Vec<Playlist>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM playlists ORDER BY name COLLATE NOCASE ASC")?;
        let playlists = stmt
            .query_map([], |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    is_smart: row.get(2)?,
                    smart_rules: row.get(3)?,
                    date_created: parse_datetime_or_default(&row.get::<_, String>(4)?),
                    date_modified: parse_datetime_or_default(&row.get::<_, String>(5)?),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(playlists)
    }

    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        let now = chrono::Local::now().naive_local().to_string();
        self.conn.execute(
            "INSERT INTO playlists (name, is_smart, date_created, date_modified) VALUES (?1, 0, ?2, ?2)",
            params![name, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    #[cfg(test)]
    pub fn add_to_playlist(&self, playlist_id: i64, track_id: i64) -> Result<()> {
        let max_pos: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
                params![playlist_id],
                |row| row.get(0),
            )?;
        self.conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, max_pos + 1],
        )?;
        Ok(())
    }

    pub fn playlists_with_counts(&self) -> Result<Vec<(Playlist, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.is_smart, p.smart_rules, p.date_created, \
                    p.date_modified, COUNT(pt.id) \
             FROM playlists p \
             LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id \
             GROUP BY p.id \
             ORDER BY p.name COLLATE NOCASE ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    Playlist {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        is_smart: row.get(2)?,
                        smart_rules: row.get(3)?,
                        date_created: parse_datetime_or_default(&row.get::<_, String>(4)?),
                        date_modified: parse_datetime_or_default(&row.get::<_, String>(5)?),
                    },
                    row.get::<_, i64>(6)? as usize,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn rename_playlist(&self, playlist_id: i64, name: &str) -> Result<()> {
        let now = chrono::Local::now().naive_local().to_string();
        self.conn.execute(
            "UPDATE playlists SET name = ?2, date_modified = ?3 WHERE id = ?1",
            params![playlist_id, name, now],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, playlist_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
        Ok(())
    }

    pub fn set_playlist_tracks(&mut self, playlist_id: i64, track_ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) \
                 VALUES (?1, ?2, ?3)",
            )?;
            for (position, track_id) in track_ids.iter().enumerate() {
                insert.execute(params![playlist_id, track_id, position as i64])?;
            }
        }
        let now = chrono::Local::now().naive_local().to_string();
        tx.execute(
            "UPDATE playlists SET date_modified = ?2 WHERE id = ?1",
            params![playlist_id, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn append_to_playlist(&mut self, playlist_id: i64, track_ids: &[i64]) -> Result<()> {
        let mut at: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )?;
        let tx = self.conn.transaction()?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) \
                 VALUES (?1, ?2, ?3)",
            )?;
            for track_id in track_ids {
                at += 1;
                insert.execute(params![playlist_id, track_id, at])?;
            }
        }
        let now = chrono::Local::now().naive_local().to_string();
        tx.execute(
            "UPDATE playlists SET date_modified = ?2 WHERE id = ?1",
            params![playlist_id, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.* FROM tracks t
             JOIN playlist_tracks pt ON t.id = pt.track_id
             WHERE pt.playlist_id = ?1
             ORDER BY pt.position ASC",
        )?;
        let tracks = stmt
            .query_map(params![playlist_id], row_to_track)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tracks)
    }
}

fn fold(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let mut out = String::with_capacity(text.len());
    for c in text
        .to_lowercase()
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
    {
        match c {
            'ß' => out.push_str("ss"),
            'ø' => out.push('o'),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            'đ' | 'ð' => out.push('d'),
            'ł' => out.push('l'),
            'þ' => out.push_str("th"),
            _ => out.push(c),
        }
    }
    out
}

pub fn grouping_artist(track: &Track) -> &str {
    if track.album_artist.is_empty() {
        &track.artist
    } else {
        &track.album_artist
    }
}

pub fn sort_key(text: &str) -> String {
    fold(text)
        .chars()
        .filter(|c| !matches!(c, '\u{200B}'..='\u{200D}' | '\u{FEFF}' | '\u{00AD}'))
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn shelf_key(text: &str) -> (u8, String) {
    let folded = sort_key(text);
    let mut words = String::new();
    let mut digits = String::new();
    let flush = |digits: &mut String, words: &mut String| {
        if !digits.is_empty() {
            words.push_str(&format!("{:0>12}", digits.trim_start_matches('0')));
            digits.clear();
        }
    };
    for c in folded.chars() {
        if c.is_numeric() {
            digits.push(c);
            continue;
        }
        flush(&mut digits, &mut words);
        if c.is_alphanumeric() {
            words.push(c);
        } else if !words.is_empty() && !words.ends_with(' ') {
            words.push(' ');
        }
    }
    flush(&mut digits, &mut words);
    let key = words.trim_end().to_string();
    let class = match key.chars().next() {
        Some(c) if c.is_alphabetic() => 0,
        Some(_) => 1,
        None => 2,
    };
    (class, key)
}

fn search_text(title: &str, artist: &str, album_artist: &str, album: &str) -> String {
    fold(&format!("{title}\u{1}{artist}\u{1}{album_artist}\u{1}{album}"))
}

fn filter_patterns(query: Option<&str>) -> Vec<String> {
    let Some(query) = query else {
        return Vec::new();
    };
    query
        .split_whitespace()
        .map(|word| {
            let escaped = fold(word)
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            format!("%{escaped}%")
        })
        .collect()
}

fn filter_conditions(patterns: &[String], start: usize) -> String {
    patterns
        .iter()
        .enumerate()
        .map(|(i, _)| format!("search_text LIKE ?{} ESCAPE '\\'", start + i))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn filter_where(patterns: &[String], start: usize) -> String {
    if patterns.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", filter_conditions(patterns, start))
    }
}

fn row_to_track(row: &rusqlite::Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        path: std::path::PathBuf::from(row.get::<_, String>(1)?),
        title: row.get(2)?,
        artist: row.get(3)?,
        album_artist: row.get(4)?,
        album: row.get(5)?,
        genre: row.get(6)?,
        year: row.get(7)?,
        track_number: row.get(8)?,
        disc_number: row.get(9)?,
        duration_secs: row.get(10)?,
        bitrate: row.get(11)?,
        sample_rate: row.get(12)?,
        file_size: row.get::<_, i64>(13)? as u64,
        format: row.get(14)?,
        rating: row.get(15)?,
        play_count: row.get(16)?,
        last_played: row
            .get::<_, Option<String>>(17)?
            .and_then(|s| parse_datetime_opt(&s)),
        date_added: parse_datetime_or_default(&row.get::<_, String>(18)?),
        date_modified: parse_datetime_or_default(&row.get::<_, String>(19)?),
        file_hash: row.get(20)?,
        has_artwork: row.get::<_, i32>(21)? != 0,
    })
}

fn parse_datetime_opt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").ok()
}

fn parse_datetime_or_default(s: &str) -> NaiveDateTime {
    parse_datetime_opt(s).unwrap_or_else(|| chrono::Local::now().naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::SortOrder;

    fn make_test_track(title: &str, artist: &str, album: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from(format!("/music/{}/{}.mp3", artist, title)),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: album.into(),
            genre: "Electronic".into(),
            year: Some(2024),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 240.0,
            bitrate: Some(320),
            sample_rate: Some(44100),
            file_size: 8_000_000,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: chrono::Local::now().naive_local(),
            date_modified: chrono::Local::now().naive_local(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn test_upsert_and_query() {
        let db = Database::open_memory().unwrap();

        let track = make_test_track("Xtal", "Aphex Twin", "SAW 85-92");
        db.upsert_track(&track).unwrap();

        let artists = db.get_artists(None).unwrap();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].name, "Aphex Twin");

        let tracks = db
            .get_tracks(
                Some("Aphex Twin"),
                None,
                SortField::Title,
                SortOrder::Ascending,
                None,
            )
            .unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Xtal");
    }

    #[test]
    fn test_search() {
        let db = Database::open_memory().unwrap();
        db.upsert_track(&make_test_track("Xtal", "Aphex Twin", "SAW 85-92"))
            .unwrap();
        db.upsert_track(&make_test_track("Ageispolis", "Aphex Twin", "SAW 85-92"))
            .unwrap();
        db.upsert_track(&make_test_track("Roygbiv", "Boards of Canada", "MHTRTC"))
            .unwrap();

        let results = db.search_tracks("aphex").unwrap();
        assert_eq!(results.len(), 2);

        let results = db.search_tracks("Roygbiv").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_playlists() {
        let db = Database::open_memory().unwrap();
        let track = make_test_track("Xtal", "Aphex Twin", "SAW 85-92");
        let track_id = db.upsert_track(&track).unwrap();

        let playlist_id = db.create_playlist("Favorites").unwrap();
        db.add_to_playlist(playlist_id, track_id).unwrap();

        let tracks = db.get_playlist_tracks(playlist_id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Xtal");
    }

    #[test]
    fn a_list_keeps_the_order_it_was_given() {
        let mut db = Database::open_memory().unwrap();
        let mut ids = Vec::new();
        for title in ["a", "b", "c"] {
            let track = make_test_track(title, "Eno", "On Land");
            ids.push(db.upsert_track(&track).unwrap());
        }

        let list = db.create_playlist("abend").unwrap();
        let wanted = vec![ids[2], ids[0], ids[1], ids[0]];
        db.set_playlist_tracks(list, &wanted).unwrap();

        let titles: Vec<String> = db
            .get_playlist_tracks(list)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, vec!["c", "a", "b", "a"]);

        let (_, count) = db
            .playlists_with_counts()
            .unwrap()
            .into_iter()
            .find(|(p, _)| p.id == list)
            .expect("the list is listed");
        assert_eq!(count, 4, "the count is of places, not of different tracks");

        db.append_to_playlist(list, &[ids[1]]).unwrap();
        assert_eq!(db.get_playlist_tracks(list).unwrap().len(), 5);

        db.set_playlist_tracks(list, &[ids[0]]).unwrap();
        assert_eq!(db.get_playlist_tracks(list).unwrap().len(), 1);

        db.rename_playlist(list, "später").unwrap();
        let named: Vec<String> = db
            .playlists_with_counts()
            .unwrap()
            .into_iter()
            .map(|(p, _)| p.name)
            .collect();
        assert_eq!(named, vec!["später"]);

        db.delete_playlist(list).unwrap();
        assert!(db.playlists_with_counts().unwrap().is_empty());
        assert_eq!(db.track_count().unwrap(), 3);
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;

    fn track(artist: &str, album: &str, year: i32, nr: u32, title: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from(format!("/m/{artist}-{album}-{nr}.mp3")),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: album.into(),
            genre: String::new(),
            year: Some(year),
            track_number: Some(nr),
            disc_number: Some(1),
            duration_secs: 100.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: NaiveDateTime::default(),
            date_modified: NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn a_track_number_sort_gathers_the_openers() {
        let db = Database::open_memory().unwrap();
        for t in [
            track("Eno", "On Land", 1982, 1, "lizard point"),
            track("Eno", "On Land", 1982, 2, "the lost day"),
            track("Aphex Twin", "SAW", 1992, 1, "xtal"),
            track("Aphex Twin", "SAW", 1992, 2, "tha"),
        ] {
            db.upsert_track(&t).unwrap();
        }

        let titles: Vec<String> = db
            .get_tracks(None, None, SortField::TrackNumber, SortOrder::Ascending, None)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();

        assert_eq!(titles, vec!["xtal", "lizard point", "tha", "the lost day"]);
    }

    #[test]
    fn a_title_alone_does_not_pick_out_one_record() {
        let db = Database::open_memory().unwrap();
        for t in [
            track("Slowdive", "Greatest Hits", 1994, 1, "alison"),
            track("Cher", "Greatest Hits", 1999, 1, "believe"),
            track("Slowdive", "", 0, 1, "eine lose datei"),
            track("Cher", "", 0, 1, "noch eine"),
        ] {
            db.upsert_track(&t).unwrap();
        }

        let titles = |artist: Option<&str>, album: Option<&str>| -> Vec<String> {
            db.get_tracks(artist, album, SortField::Title, SortOrder::Ascending, None)
                .unwrap()
                .into_iter()
                .map(|t| t.title)
                .collect()
        };

        assert_eq!(titles(None, Some("Greatest Hits")).len(), 2);
        assert_eq!(titles(None, Some("")).len(), 2);

        assert_eq!(
            titles(Some("Slowdive"), Some("Greatest Hits")),
            vec!["alison"]
        );
        assert_eq!(titles(Some("Cher"), Some("")), vec!["noch eine"]);
    }

    #[test]
    fn the_same_title_by_two_artists_is_two_albums() {
        let db = Database::open_memory().unwrap();
        for t in [
            track("Slowdive", "Greatest Hits", 1994, 1, "alison"),
            track("Cher", "Greatest Hits", 1999, 1, "believe"),
            track("Slowdive", "", 0, 1, "eine lose datei"),
            track("Cher", "", 0, 1, "noch eine"),
        ] {
            db.upsert_track(&t).unwrap();
        }

        let albums = db.get_albums(None, None).unwrap();
        assert_eq!(albums.len(), 4);
        let nameless: Vec<&str> = albums
            .iter()
            .filter(|a| a.name.is_empty())
            .map(|a| a.artist.as_str())
            .collect();
        assert_eq!(nameless.len(), 2);
        assert!(nameless.contains(&"Cher") && nameless.contains(&"Slowdive"));
    }

    #[test]
    fn one_album_keeps_its_own_track_order() {
        let db = Database::open_memory().unwrap();
        for t in [
            track("Eno", "On Land", 1982, 3, "tal coat"),
            track("Eno", "On Land", 1982, 1, "lizard point"),
            track("Eno", "On Land", 1982, 2, "the lost day"),
        ] {
            db.upsert_track(&t).unwrap();
        }

        let titles: Vec<String> = db
            .get_tracks(
                Some("Eno"),
                Some("On Land"),
                SortField::TrackNumber,
                SortOrder::Ascending,
                None,
            )
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();

        assert_eq!(titles, vec!["lizard point", "the lost day", "tal coat"]);
    }

    #[test]
    fn artists_run_as_on_the_ipod() {
        let db = Database::open_memory().unwrap();
        for (n, name) in ["6955", "(((Microwaves)))", "Zeni Geva", "", "ÄÄNIPÄÄ", "Acid Mothers Guru Guru", "40318"]
            .into_iter()
            .enumerate()
        {
            db.upsert_track(&track(name, "x", 2000, 1, &format!("t{n}"))).unwrap();
        }
        let wanted = ["ÄÄNIPÄÄ", "Acid Mothers Guru Guru", "(((Microwaves)))", "Zeni Geva", "6955", "40318", ""];
        let artists: Vec<String> = db.get_artists(None).unwrap().into_iter().map(|a| a.name).collect();
        assert_eq!(artists, wanted);
        let tracks: Vec<String> = db
            .get_tracks(None, None, SortField::Artist, SortOrder::Ascending, None)
            .unwrap()
            .into_iter()
            .map(|t| t.artist)
            .collect();
        assert_eq!(tracks, wanted);
    }

    #[test]
    fn the_nameless_go_last() {
        let db = Database::open_memory().unwrap();
        for t in [
            track("boards of canada", "geogaddi", 2002, 1, "ready lets go"),
            track("", "", 0, 1, "irgendwas"),
            track("aphex twin", "drukqs", 2001, 1, "jynweythek"),
            track("zoviet france", "shouting at the ground", 1988, 1, "shamany"),
        ] {
            db.upsert_track(&t).unwrap();
        }

        let artists: Vec<String> = db
            .get_artists(None)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(
            artists,
            ["aphex twin", "boards of canada", "zoviet france", ""],
            "the nameless one belongs at the end"
        );

        let albums: Vec<String> = db
            .get_albums(None, None)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(albums.last().map(|s| s.as_str()), Some(""));
        assert_eq!(albums.len(), 4);
    }

    #[test]
    fn albums_can_run_newest_first() {
        let db = Database::open_memory().unwrap();
        let day = |y: i32, m: u32, d: u32| {
            chrono::NaiveDate::from_ymd_opt(y, m, d)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
        };

        let mut old = track("Eno", "On Land", 1982, 1, "lizard point");
        old.date_added = day(2020, 1, 1);
        let mut completed = track("Eno", "On Land", 1982, 2, "the lost day");
        completed.date_added = day(2026, 9, 1);
        let mut middle = track("Aphex Twin", "SAW", 1992, 1, "xtal");
        middle.date_added = day(2023, 5, 5);
        let mut oldest = track("Coil", "Musick", 1998, 1, "constant shallowness");
        oldest.date_added = day(2019, 3, 3);

        for t in [&old, &completed, &middle, &oldest] {
            db.upsert_track(t).unwrap();
        }

        let names: Vec<String> = db
            .get_albums_recent(None, None)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, vec!["On Land", "SAW", "Musick"]);

        let names: Vec<String> = db
            .get_albums_recent(Some("Coil"), None)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, vec!["Musick"]);
    }

    #[test]
    fn an_older_library_is_redated_from_its_files() {
        let db = Database::open_memory().unwrap();
        let mut old = track("Eno", "On Land", 1982, 1, "lizard point");
        old.date_modified = chrono::NaiveDate::from_ymd_opt(2014, 6, 1)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        old.date_added = chrono::NaiveDate::from_ymd_opt(2026, 9, 11)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        db.upsert_track(&old).unwrap();

        db.conn
            .execute("DELETE FROM meta WHERE key = 'date_added_from_files'", [])
            .unwrap();
        db.date_tracks_by_their_files().unwrap();

        let added: String = db
            .conn
            .query_row("SELECT date_added FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert!(added.starts_with("2014-06-01"), "left as {added}");

        db.conn
            .execute("DELETE FROM meta WHERE key = 'date_added_from_files'", [])
            .unwrap();
        db.date_tracks_by_their_files().unwrap();
        let again: String = db
            .conn
            .query_row("SELECT date_added FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(added, again);
    }

    #[test]
    #[cfg(unix)]
    fn a_file_written_down_twice_is_folded_back_into_one() {
        let root = std::env::temp_dir().join("kessler-two-names");
        let _ = std::fs::remove_dir_all(&root);
        let real = root.join("Cave");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("01.mp3"), b"not really an mp3").unwrap();
        let other = root.join("CAVE");
        std::os::unix::fs::symlink(&real, &other).unwrap();

        let db = Database::open_memory().unwrap();
        let mut one = track("Cave", "Allways", 2018, 2, "san'yago");
        one.path = real.join("01.mp3");
        one.play_count = 2;
        one.date_added = chrono::NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        db.upsert_track(&one).unwrap();

        let mut twin = one.clone();
        twin.path = other.join("01.mp3");
        twin.play_count = 7;
        twin.date_added = chrono::NaiveDate::from_ymd_opt(2026, 9, 1)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        db.upsert_track(&twin).unwrap();
        let both = db
            .get_tracks(None, None, SortField::Title, SortOrder::Ascending, None)
            .unwrap();
        assert_eq!(both.len(), 2, "the bug itself did not reproduce");

        db.conn
            .execute("DELETE FROM meta WHERE key = 'one_row_per_file'", [])
            .unwrap();
        db.one_row_per_file().unwrap();

        let left = db
            .get_tracks(None, None, SortField::Title, SortOrder::Ascending, None)
            .unwrap();
        assert_eq!(left.len(), 1, "still two rows after mending");
        assert_eq!(left[0].path, real.join("01.mp3"), "kept the wrong name");
        assert_eq!(left[0].play_count, 7, "the plays of both were not kept");
        assert_eq!(
            left[0].date_added.date(),
            chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            "a record does not become new again by being found twice"
        );

        db.conn
            .execute("DELETE FROM meta WHERE key = 'one_row_per_file'", [])
            .unwrap();
        db.one_row_per_file().unwrap();
        assert_eq!(
            db.get_tracks(None, None, SortField::Title, SortOrder::Ascending, None)
                .unwrap()
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn one_artist_reads_as_their_records_in_turn() {
        let db = Database::open_memory().unwrap();
        for t in [
            track("Eno", "Apollo", 1983, 2, "signals"),
            track("Eno", "Apollo", 1983, 1, "under stars"),
            track("Eno", "On Land", 1982, 2, "the lost day"),
            track("Eno", "On Land", 1982, 1, "lizard point"),
            track("Eno", "on land", 1982, 3, "tal coat"),
        ] {
            db.upsert_track(&t).unwrap();
        }

        let titles: Vec<String> = db
            .get_tracks(Some("Eno"), None, SortField::Artist, SortOrder::Ascending, None)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();

        assert_eq!(
            titles,
            vec![
                "lizard point",
                "the lost day",
                "tal coat",
                "under stars",
                "signals",
            ]
        );
    }

    #[test]
    fn the_shelf_read_backwards_is_the_list_reversed() {
        let db = Database::open_memory().unwrap();
        for t in [
            track("Eno", "Apollo", 1983, 2, "signals"),
            track("Eno", "Apollo", 1983, 1, "under stars"),
            track("Eno", "On Land", 1982, 2, "the lost day"),
            track("Eno", "On Land", 1982, 1, "lizard point"),
            track("Eno", "on land", 1982, 3, "tal coat"),
        ] {
            db.upsert_track(&t).unwrap();
        }

        let read = |order| -> Vec<String> {
            db.get_tracks(Some("Eno"), None, SortField::Artist, order, None)
                .unwrap()
                .into_iter()
                .map(|t| t.title)
                .collect()
        };

        let mut backwards = read(SortOrder::Ascending);
        backwards.reverse();
        assert_eq!(read(SortOrder::Descending), backwards);
    }

    #[test]
    fn albums_added_together_stay_alphabetical() {
        let db = Database::open_memory().unwrap();
        for (artist, album) in [("C", "charlie"), ("A", "alpha"), ("B", "bravo")] {
            db.upsert_track(&track(artist, album, 2000, 1, "x")).unwrap();
        }
        let names: Vec<String> = db
            .get_albums_recent(None, None)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    fn track(artist: &str, album: &str, title: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from(format!("/m/{artist}-{album}-{title}.mp3")),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: album.into(),
            genre: String::new(),
            year: Some(1997),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 100.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: NaiveDateTime::default(),
            date_modified: NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn library() -> Database {
        let db = Database::open_memory().unwrap();
        for t in [
            track("Björk", "Homogenic", "Jóga"),
            track("Björk", "Homogenic", "Bachelorette"),
            track("Björk", "Vespertine", "Pagan Poetry"),
            track("Aphex Twin", "SAW 85-92", "Xtal"),
            track("坂本龍一", "async", "andata"),
        ] {
            db.upsert_track(&t).unwrap();
        }
        db
    }

    fn titles(db: &Database, query: &str) -> Vec<String> {
        db.get_tracks(
            None,
            None,
            SortField::Artist,
            SortOrder::Ascending,
            Some(query),
        )
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect()
    }

    #[test]
    fn matches_any_of_title_artist_and_album() {
        let db = library();
        assert_eq!(titles(&db, "xtal"), vec!["Xtal"]);
        assert_eq!(titles(&db, "vespertine"), vec!["Pagan Poetry"]);
        assert_eq!(titles(&db, "aphex").len(), 1);
    }

    #[test]
    fn ignores_case_beyond_ascii() {
        let db = library();
        assert_eq!(titles(&db, "björk").len(), 3);
        assert_eq!(titles(&db, "BJÖRK").len(), 3);
        assert_eq!(titles(&db, "jóga"), vec!["Jóga"]);
        assert_eq!(titles(&db, "坂本").len(), 1);
    }

    #[test]
    fn accents_are_optional_on_both_sides() {
        let db = library();
        assert_eq!(titles(&db, "bjork").len(), 3);
        assert_eq!(titles(&db, "joga"), vec!["Jóga"]);
        assert_eq!(titles(&db, "BJORK").len(), 3);
    }

    #[test]
    fn every_word_has_to_match() {
        let db = library();
        assert_eq!(titles(&db, "björk homogenic").len(), 2);
        assert_eq!(titles(&db, "björk async"), Vec::<String>::new());
        assert_eq!(titles(&db, "homogenic björk").len(), 2);
    }

    #[test]
    fn typed_wildcards_are_literal() {
        let db = library();
        assert!(titles(&db, "%").is_empty());
        assert!(titles(&db, "_").is_empty());
        assert!(titles(&db, "\\").is_empty());
    }

    #[test]
    fn whitespace_alone_filters_nothing() {
        let db = library();
        assert_eq!(titles(&db, "   ").len(), 5);
    }

    #[test]
    fn artists_and_albums_narrow_with_the_query() {
        let db = library();
        assert_eq!(db.get_artists(Some("björk")).unwrap().len(), 1);
        assert_eq!(db.get_albums(None, Some("björk")).unwrap().len(), 2);
        assert_eq!(db.get_albums(None, Some("homogenic")).unwrap().len(), 1);
        assert_eq!(
            db.get_albums(Some("Björk"), Some("vespertine"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn an_older_database_is_backfilled() {
        let db = library();
        db.conn
            .execute_batch(
                "ALTER TABLE tracks DROP COLUMN search_text;
                 DELETE FROM meta WHERE key = 'search_text_version';",
            )
            .unwrap();
        db.add_search_column().unwrap();
        assert_eq!(titles(&db, "björk").len(), 3);
        assert_eq!(titles(&db, "坂本").len(), 1);
    }
}

#[cfg(test)]
mod fold_tests {
    use super::*;

    #[test]
    fn accents_come_off_both_sides() {
        assert_eq!(fold("Björk"), "bjork");
        assert_eq!(fold("BJÖRK"), "bjork");
        assert_eq!(fold("Jóga"), "joga");
        assert_eq!(fold("Sigur Rós"), "sigur ros");
        assert_eq!(fold("Amélie"), "amelie");
        assert_eq!(fold("Motörhead"), "motorhead");
    }

    #[test]
    fn letters_that_do_not_decompose_are_spelled_out() {
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Tørn"), "torn");
        assert_eq!(fold("Æther"), "aether");
        assert_eq!(fold("Łódź"), "lodz");
    }

    #[test]
    fn leaves_unaccented_text_alone() {
        assert_eq!(fold("Aphex Twin"), "aphex twin");
        assert_eq!(fold("坂本龍一"), "坂本龍一");
        assert_eq!(fold(""), "");
    }
}

#[cfg(test)]
mod grouping_tests {
    use super::*;

    fn track(artist: &str, album: &str, title: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from(format!("/m/{artist}-{album}-{title}.mp3")),
            title: title.into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: album.into(),
            genre: String::new(),
            year: Some(1998),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 100.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: NaiveDateTime::default(),
            date_modified: NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn with(tracks: &[Track]) -> Database {
        let db = Database::open_memory().unwrap();
        for t in tracks {
            db.upsert_track(t).unwrap();
        }
        db
    }

    #[test]
    fn case_alone_does_not_make_two_artists() {
        let db = with(&[
            track("Boards Of Canada", "Geogaddi", "dandelion"),
            track("boards of canada", "Geogaddi", "1969"),
            track("BOARDS OF CANADA", "Geogaddi", "gyroscope"),
        ]);
        let artists = db.get_artists(None).unwrap();
        assert_eq!(artists.len(), 1, "{artists:?}");
        assert_eq!(artists[0].track_count, 3);
        assert_eq!(artists[0].album_count, 1);
    }

    #[test]
    fn invisible_differences_do_not_make_two_artists() {
        let db = with(&[
            track("Blind Guardian", "Nightfall", "a"),
            track("Blind\u{00A0}Guardian", "Nightfall", "b"),
            track("Blind Guardian\u{200B}", "Nightfall", "c"),
            track("Blind  Guardian ", "Nightfall", "d"),
        ]);
        let artists = db.get_artists(None).unwrap();
        assert_eq!(artists.len(), 1, "{artists:?}");
        assert_eq!(artists[0].track_count, 4);
    }

    #[test]
    fn selecting_a_merged_artist_finds_every_spelling() {
        let db = with(&[
            track("Boris", "Pink", "farewell"),
            track("boris", "Pink", "pink"),
            track("BORIS", "Pink", "woman on the screen"),
        ]);
        let artists = db.get_artists(None).unwrap();
        let name = artists[0].name.clone();

        let tracks = db
            .get_tracks(
                Some(&name),
                None,
                SortField::Title,
                SortOrder::Ascending,
                None,
            )
            .unwrap();
        assert_eq!(tracks.len(), 3, "spellings were dropped");

        let albums = db.get_albums(Some(&name), None).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].track_count, 3);
    }

    #[test]
    fn the_commonest_spelling_wins() {
        let db = with(&[
            track("boards of canada", "Geogaddi", "a"),
            track("Boards of Canada", "Geogaddi", "b"),
            track("Boards of Canada", "Geogaddi", "c"),
            track("BOARDS OF CANADA", "Geogaddi", "d"),
        ]);
        assert_eq!(db.get_artists(None).unwrap()[0].name, "Boards of Canada");
    }

    #[test]
    fn merging_stops_at_names_that_actually_differ() {
        let db = with(&[
            track("Boris", "Pink", "a"),
            track("Boris with Michio Kurihara", "Rainbow", "b"),
            track("Bong", "Mana", "c"),
        ]);
        assert_eq!(db.get_artists(None).unwrap().len(), 3);
    }

    #[test]
    fn two_greatest_hits_are_two_albums() {
        let db = with(&[
            track("Bob Marley", "Greatest Hits", "a"),
            track("Busta Rhymes", "greatest hits", "b"),
        ]);
        assert_eq!(db.get_albums(None, None).unwrap().len(), 2);
    }

    #[test]
    fn album_spellings_merge_too() {
        let db = with(&[
            track("Burzum", "Filosofem", "a"),
            track("Burzum", "filosofem", "b"),
            track("Burzum", "Filosofem ", "c"),
        ]);
        let albums = db.get_albums(None, None).unwrap();
        assert_eq!(albums.len(), 1, "{albums:?}");
        assert_eq!(albums[0].track_count, 3);
    }

    #[test]
    fn the_key_ignores_case_accents_and_invisible_characters() {
        assert_eq!(sort_key("Boards Of Canada"), "boards of canada");
        assert_eq!(sort_key("  Blind   Guardian  "), "blind guardian");
        assert_eq!(sort_key("Blind\u{00A0}Guardian"), "blind guardian");
        assert_eq!(sort_key("Bj\u{00F6}rk\u{200B}"), "bjork");
        assert_eq!(sort_key("Motörhead"), "motorhead");
        assert_ne!(sort_key("boris"), sort_key("bong"));
        assert_ne!(sort_key("blind guardian"), sort_key("blind blake"));
    }
}

#[cfg(test)]
mod folder_tests {
    use super::*;

    fn track_at(path: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from(path),
            title: "t".into(),
            artist: "a".into(),
            album_artist: String::new(),
            album: "b".into(),
            genre: String::new(),
            year: None,
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 1.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: NaiveDateTime::default(),
            date_modified: NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn a_folder_takes_only_what_is_inside_it() {
        let db = Database::open_memory().unwrap();
        for path in [
            "/music/live/a.mp3",
            "/music/live/deeper/b.mp3",
            "/music/live-bootlegs/c.mp3",
            "/music/livery/d.mp3",
            "/other/e.mp3",
        ] {
            db.upsert_track(&track_at(path)).unwrap();
        }

        let removed = db
            .remove_tracks_under(std::path::Path::new("/music/live"))
            .unwrap();
        assert_eq!(removed, 2, "took the wrong tracks");
        assert_eq!(db.track_count().unwrap(), 3);
        assert!(db.get_track_by_path("/music/live-bootlegs/c.mp3").unwrap().is_some());
        assert!(db.get_track_by_path("/music/livery/d.mp3").unwrap().is_some());
        assert!(db.get_track_by_path("/other/e.mp3").unwrap().is_some());
    }

    #[test]
    fn a_trailing_slash_makes_no_difference() {
        let db = Database::open_memory().unwrap();
        db.upsert_track(&track_at("/music/live/a.mp3")).unwrap();
        db.upsert_track(&track_at("/music/other/b.mp3")).unwrap();
        assert_eq!(
            db.remove_tracks_under(std::path::Path::new("/music/live/"))
                .unwrap(),
            1
        );
        assert_eq!(db.track_count().unwrap(), 1);
    }

    #[test]
    fn wildcards_in_a_folder_name_are_literal() {
        let db = Database::open_memory().unwrap();
        db.upsert_track(&track_at("/music/100%/a.mp3")).unwrap();
        db.upsert_track(&track_at("/music/1000/b.mp3")).unwrap();
        db.upsert_track(&track_at("/music/a_b/c.mp3")).unwrap();
        db.upsert_track(&track_at("/music/axb/d.mp3")).unwrap();

        assert_eq!(
            db.remove_tracks_under(std::path::Path::new("/music/100%"))
                .unwrap(),
            1
        );
        assert_eq!(
            db.remove_tracks_under(std::path::Path::new("/music/a_b"))
                .unwrap(),
            1
        );
        assert_eq!(db.track_count().unwrap(), 2);
    }

    #[test]
    fn an_empty_folder_removes_nothing() {
        let db = Database::open_memory().unwrap();
        db.upsert_track(&track_at("/music/a.mp3")).unwrap();
        assert_eq!(
            db.remove_tracks_under(std::path::Path::new("/elsewhere"))
                .unwrap(),
            0
        );
        assert_eq!(db.track_count().unwrap(), 1);
    }
}

#[cfg(test)]
mod recent_tests {
    use super::*;

    fn arrived(title: &str, at: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from(format!("/m/{title}.mp3")),
            title: title.into(),
            artist: "someone".into(),
            album_artist: "someone".into(),
            album: "somewhere".into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 1.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S").unwrap(),
            date_modified: NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    fn titles(tracks: Vec<Track>) -> Vec<String> {
        tracks.into_iter().map(|t| t.title).collect()
    }

    #[test]
    fn the_newest_come_first_and_the_list_stops_where_asked() {
        let db = Database::open_memory().unwrap();
        for (title, at) in [
            ("alt", "2020-01-01 10:00:00"),
            ("mittel", "2024-06-01 10:00:00"),
            ("neu", "2026-09-01 10:00:00"),
            ("älter", "2019-01-01 10:00:00"),
        ] {
            db.upsert_track(&arrived(title, at)).unwrap();
        }

        assert_eq!(
            titles(db.get_recent_tracks(None, 2).unwrap()),
            vec!["neu", "mittel"]
        );
        assert_eq!(
            titles(db.get_recent_tracks(None, 99).unwrap()),
            vec!["neu", "mittel", "alt", "älter"]
        );
    }

    #[test]
    fn files_that_arrived_in_the_same_second_keep_a_fixed_order() {
        let db = Database::open_memory().unwrap();
        for title in ["eins", "zwei", "drei", "vier", "fünf"] {
            db.upsert_track(&arrived(title, "2026-09-01 10:00:00"))
                .unwrap();
        }

        let once = titles(db.get_recent_tracks(None, 3).unwrap());
        let again = titles(db.get_recent_tracks(None, 3).unwrap());
        assert_eq!(once, again);
        assert_eq!(once, vec!["fünf", "vier", "drei"]);
    }

    #[test]
    fn a_search_still_narrows_it() {
        let db = Database::open_memory().unwrap();
        db.upsert_track(&arrived("regen", "2026-01-01 10:00:00"))
            .unwrap();
        db.upsert_track(&arrived("sonne", "2026-02-01 10:00:00"))
            .unwrap();

        assert_eq!(
            titles(db.get_recent_tracks(Some("regen"), 80).unwrap()),
            vec!["regen"]
        );
    }

    #[test]
    fn asking_for_none_still_gives_one() {
        let db = Database::open_memory().unwrap();
        db.upsert_track(&arrived("da", "2026-01-01 10:00:00")).unwrap();
        assert_eq!(db.get_recent_tracks(None, 0).unwrap().len(), 1);
    }
}

#[cfg(test)]
mod shelf_order_tests {
    use super::*;

    fn track(artist: &str, album: &str, year: i32, number: u32, title: &str) -> Track {
        Track {
            id: 0,
            path: std::path::PathBuf::from(format!("/m/{artist}/{album}/{number} {title}.mp3")),
            title: title.into(),
            artist: artist.into(),
            album_artist: artist.into(),
            album: album.into(),
            genre: String::new(),
            year: Some(year),
            track_number: Some(number),
            disc_number: Some(1),
            duration_secs: 1.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: NaiveDateTime::default(),
            date_modified: NaiveDateTime::default(),
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn a_whole_artist_comes_back_in_shelf_order() {
        let db = Database::open_memory().unwrap();
        for (album, year, number, title) in [
            ("später", 2005, 2, "d"),
            ("später", 2005, 1, "c"),
            ("früher", 1999, 2, "b"),
            ("früher", 1999, 1, "a"),
        ] {
            db.upsert_track(&track("wer", album, year, number, title))
                .unwrap();
        }
        db.upsert_track(&track("wer anders", "fremd", 2001, 1, "x"))
            .unwrap();

        let carried = db
            .get_tracks(
                Some("wer"),
                None,
                SortField::Artist,
                SortOrder::Ascending,
                None,
            )
            .unwrap();
        let titles: Vec<&str> = carried.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["a", "b", "c", "d"],
            "a dragged artist should arrive as records by year, each in order"
        );
    }
}
