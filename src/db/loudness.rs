use rusqlite::params;

use super::Database;
use crate::error::Result;
use crate::library::loudness::Loudness;

#[derive(Debug, Clone)]
pub struct LoudRow {
    pub id: i64,
    pub path: std::path::PathBuf,
    pub record: String,
    pub seconds: f64,
    pub modified: String,
    pub known: Option<Known>,
    pub fresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Known {
    pub lufs: f64,
    pub peak: f64,
    pub track_gain: f64,
    pub album_gain: Option<f64>,
    pub album_peak: Option<f64>,
    pub album_from_tags: bool,
    pub from_tags: bool,
}

impl Database {
    pub(super) fn add_loudness_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS loudness (
                track_id        INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
                lufs            REAL NOT NULL,
                peak            REAL NOT NULL,
                track_gain      REAL NOT NULL,
                album_gain      REAL,
                album_peak      REAL,
                album_from_tags INTEGER NOT NULL DEFAULT 0,
                from_tags       INTEGER NOT NULL DEFAULT 0,
                file_date       TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn loudness_rows(&self) -> Result<Vec<LoudRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.path, t.artist_key, t.album_key, t.album, t.duration_secs,
                    t.date_modified, l.lufs, l.peak, l.track_gain, l.album_gain,
                    l.album_peak, l.album_from_tags, l.from_tags, l.file_date
             FROM tracks t LEFT JOIN loudness l ON l.track_id = t.id
             ORDER BY t.artist_key, t.album_key, t.disc_number, t.track_number",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let album: String = row.get(4)?;
            let record = if album.trim().is_empty() {
                format!("\u{1}{id}")
            } else {
                format!("{}\u{1}{}", row.get::<_, String>(2)?, row.get::<_, String>(3)?)
            };
            let modified: String = row.get(6)?;
            let file_date: Option<String> = row.get(14)?;
            let fresh = file_date.as_deref() == Some(modified.as_str());
            let known = match row.get::<_, Option<f64>>(7)? {
                Some(lufs) => Some(Known {
                    lufs,
                    peak: row.get(8)?,
                    track_gain: row.get(9)?,
                    album_gain: row.get(10)?,
                    album_peak: row.get(11)?,
                    album_from_tags: row.get::<_, i32>(12)? != 0,
                    from_tags: row.get::<_, i32>(13)? != 0,
                }),
                _ => None,
            };
            Ok(LoudRow {
                id,
                path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                record,
                seconds: row.get(5)?,
                modified,
                known,
                fresh,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn set_loudness(&self, id: i64, loud: &Loudness, file_date: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO loudness (track_id, lufs, peak, track_gain, album_gain, album_peak,
                                   album_from_tags, from_tags, file_date)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(track_id) DO UPDATE SET
                lufs = excluded.lufs, peak = excluded.peak, track_gain = excluded.track_gain,
                album_gain = excluded.album_gain, album_peak = excluded.album_peak,
                album_from_tags = excluded.album_from_tags, from_tags = excluded.from_tags,
                file_date = excluded.file_date",
            params![
                id,
                loud.lufs,
                loud.peak,
                loud.track_gain,
                loud.album_gain,
                loud.album_peak,
                (loud.from_tags && loud.album_gain.is_some()) as i32,
                loud.from_tags as i32,
                file_date,
            ],
        )?;
        Ok(())
    }

    pub fn set_album_loudness(&self, ids: &[i64], gain: f64, peak: f64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE loudness SET album_gain = ?2, album_peak = ?3
                 WHERE track_id = ?1 AND album_from_tags = 0",
                params![id, gain, peak],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::Track;

    fn track(path: &str, album: &str) -> Track {
        let now = chrono::NaiveDateTime::parse_from_str("2026-09-01 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        Track {
            id: 0,
            path: path.into(),
            title: "t".into(),
            artist: "a".into(),
            album_artist: String::new(),
            album: album.into(),
            genre: String::new(),
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 100.0,
            bitrate: None,
            sample_rate: None,
            file_size: 1,
            format: "mp3".into(),
            rating: 0,
            play_count: 0,
            last_played: None,
            date_added: now,
            date_modified: now,
            file_hash: None,
            has_artwork: false,
        }
    }

    #[test]
    fn loudness_is_kept_for_the_file_as_it_was() {
        let db = Database::open_memory().unwrap();
        let a = db.upsert_track(&track("/m/a.mp3", "rec")).unwrap();
        let b = db.upsert_track(&track("/m/b.mp3", "rec")).unwrap();
        let rows = db.loudness_rows().unwrap();
        assert!(rows.iter().all(|r| r.known.is_none()));
        assert_eq!(rows[0].record, rows[1].record);
        let date = rows[0].modified.clone();

        let measured = Loudness { lufs: -12.0, peak: 0.9, track_gain: -6.0, album_gain: None, album_peak: None, from_tags: false };
        let tagged = Loudness { album_gain: Some(-4.0), album_peak: Some(1.0), from_tags: true, ..measured };
        db.set_loudness(a, &measured, &date).unwrap();
        db.set_loudness(b, &tagged, "some other day").unwrap();
        db.set_album_loudness(&[a, b], -5.0, 0.95).unwrap();

        let rows = db.loudness_rows().unwrap();
        let row = |id: i64| rows.iter().find(|r| r.id == id).unwrap().clone();
        assert_eq!(row(a).known.unwrap().album_gain, Some(-5.0));
        assert!(row(a).fresh);
        assert!(row(b).known.is_some() && !row(b).fresh);
        db.set_loudness(b, &tagged, &date).unwrap();
        db.set_album_loudness(&[a, b], -5.0, 0.95).unwrap();
        let rows = db.loudness_rows().unwrap();
        assert_eq!(rows.iter().find(|r| r.id == b).unwrap().known.unwrap().album_gain, Some(-4.0));
    }
}
