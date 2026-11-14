use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::db::loudness::LoudRow;
use crate::library::loudness::{self, Loudness};
use crate::player::leveling::{Factors, Level};

use super::{log, text, GuiApp};

pub enum News {
    Track { id: i64, loud: Loudness, modified: String },
    Record { ids: Vec<i64>, gain: f64, peak: f64 },
    Failed { path: PathBuf, why: String },
}

pub struct Measuring {
    pub news: mpsc::Receiver<News>,
    pub stop: Arc<AtomicBool>,
    pub playing: Arc<AtomicBool>,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub refreshed: Instant,
}

const REFRESH: Duration = Duration::from_secs(5);

fn due(rows: Vec<LoudRow>) -> Vec<Vec<LoudRow>> {
    let mut records: Vec<Vec<LoudRow>> = Vec::new();
    for row in rows {
        match records.last_mut() {
            Some(last) if last[0].record == row.record => last.push(row),
            _ => records.push(vec![row]),
        }
    }
    records.retain(|r| r.iter().any(|t| !t.fresh));
    records
}

fn readers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get() / 2).clamp(1, 4)
}

const WAITING: Duration = Duration::from_millis(500);

fn start(records: Vec<Vec<LoudRow>>, playing: Arc<AtomicBool>) -> (mpsc::Receiver<News>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let records = Arc::new(std::sync::Mutex::new(records.into_iter()));
    for reader in 0..readers() {
        let tx = tx.clone();
        let stopped = Arc::clone(&stop);
        let playing = Arc::clone(&playing);
        let records = Arc::clone(&records);
        std::thread::spawn(move || loop {
            while reader > 0 && playing.load(Ordering::Relaxed) {
                if stopped.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(WAITING);
            }
            let Some(record) = records.lock().ok().and_then(|mut r| r.next()) else { return };
            let mut members: Vec<(f64, f64, f64)> = Vec::new();
            let mut ids = Vec::new();
            for row in &record {
                if stopped.load(Ordering::Relaxed) {
                    return;
                }
                ids.push(row.id);
                if row.fresh {
                    if let Some(k) = row.known {
                        members.push((k.lufs, k.peak, row.seconds));
                    }
                    continue;
                }
                let found = loudness::from_tags(&row.path)
                    .map(Ok)
                    .unwrap_or_else(|| loudness::measure_file(&row.path, Some(&playing)));
                let news = match found {
                    Ok(loud) => {
                        members.push((loud.lufs, loud.peak, row.seconds));
                        News::Track { id: row.id, loud, modified: row.modified.clone() }
                    }
                    Err(e) => News::Failed { path: row.path.clone(), why: e.to_string() },
                };
                if tx.send(news).is_err() {
                    return;
                }
            }
            if let Some((gain, peak)) = loudness::album(&members) {
                if tx.send(News::Record { ids, gain, peak }).is_err() {
                    return;
                }
            }
        });
    }
    (rx, stop)
}

fn factors_of(k: &crate::db::loudness::Known) -> Factors {
    let track = loudness::factor(k.track_gain, k.peak);
    let album = k
        .album_gain
        .map(|g| loudness::factor(g, k.album_peak.unwrap_or(k.peak)))
        .unwrap_or(track);
    Factors { track, album }
}

impl GuiApp {
    pub(super) fn apply_leveling(&mut self) {
        let level = Level::from_name(&self.config.player.leveling);
        self.player.leveling().set_level(level);
        self.fill_level_table();
    }

    pub fn cycle_leveling(&mut self) {
        let now = Level::from_name(&self.config.player.leveling);
        let at = Level::ALL.iter().position(|l| *l == now).unwrap_or(0);
        let next = Level::ALL[(at + 1) % Level::ALL.len()];
        self.config.player.leveling = next.name().to_string();
        self.save_config();
        self.player.leveling().set_level(next);
        if next == Level::Off {
            self.stop_measuring();
        } else {
            self.level_check = true;
        }
    }

    pub fn fill_level_table(&mut self) {
        let Ok(rows) = self.db.loudness_rows() else { return };
        self.loudness_count = (
            rows.iter().filter(|r| r.known.is_some() && r.fresh).count(),
            rows.len(),
        );
        let table: HashMap<PathBuf, Factors> = rows
            .iter()
            .filter_map(|r| r.known.as_ref().map(|k| (r.path.clone(), factors_of(k))))
            .collect();
        self.player.leveling().set_all(table);
    }

    pub fn start_measuring(&mut self) {
        if self.measuring.is_some() {
            return;
        }
        let Ok(rows) = self.db.loudness_rows() else { return };
        let records = due(rows);
        let total: usize = records.iter().flatten().filter(|r| !r.fresh).count();
        if total == 0 {
            return;
        }
        let playing = Arc::new(AtomicBool::new(false));
        let (news, stop) = start(records, Arc::clone(&playing));
        self.measuring = Some(Measuring {
            news,
            stop,
            playing,
            total,
            done: 0,
            failed: 0,
            refreshed: Instant::now(),
        });
        self.note(
            log::Kind::Library,
            tr!("lautstärke: {} titel zu messen", text::count(total as u64)),
        );
    }

    pub fn stop_measuring(&mut self) {
        if let Some(run) = self.measuring.take() {
            run.stop.store(true, Ordering::Relaxed);
            self.fill_level_table();
            self.note(
                log::Kind::Library,
                tr!(
                    "lautstärke: angehalten nach {} von {}",
                    text::count(run.done as u64),
                    text::count(run.total as u64)
                ),
            );
        }
    }

    pub(super) fn poll_measuring(&mut self) {
        if self.config.ui.music
            && std::mem::take(&mut self.level_check)
            && Level::from_name(&self.config.player.leveling) != Level::Off
        {
            self.start_measuring();
        }
        let playing = self.player.state() == crate::player::PlaybackState::Playing;
        let Some(run) = self.measuring.as_mut() else { return };
        run.playing.store(playing, Ordering::Relaxed);
        let mut ended = false;
        let mut failures: Vec<(PathBuf, String)> = Vec::new();
        loop {
            match run.news.try_recv() {
                Ok(News::Track { id, loud, modified }) => {
                    run.done += 1;
                    if let Err(e) = self.db.set_loudness(id, &loud, &modified) {
                        failures.push((PathBuf::new(), e.to_string()));
                    }
                }
                Ok(News::Record { ids, gain, peak }) => {
                    let _ = self.db.set_album_loudness(&ids, gain, peak);
                }
                Ok(News::Failed { path, why }) => {
                    run.done += 1;
                    run.failed += 1;
                    failures.push((path, why));
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        let due = run.refreshed.elapsed() >= REFRESH;
        if due {
            run.refreshed = Instant::now();
        }
        let (done, total, failed) = (run.done, run.total, run.failed);
        for (path, why) in failures {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            tracing::debug!("lautstärke: {name}: {why}");
        }
        if ended {
            self.measuring = None;
            self.fill_level_table();
            let said = if failed > 0 {
                tr!(
                    "lautstärke: {} titel gemessen, {} nicht lesbar",
                    text::count((done - failed) as u64),
                    text::count(failed as u64)
                )
            } else {
                tr!("lautstärke: {} titel gemessen", text::count(total as u64))
            };
            self.note(log::Kind::Library, said);
        } else if due {
            self.fill_level_table();
        }
    }

    pub fn measuring_standing(&self) -> Option<String> {
        let run = self.measuring.as_ref()?;
        Some(tr!(
            "lautstärke {} / {}",
            text::count(run.done as u64),
            text::count(run.total as u64)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, record: &str, fresh: bool) -> LoudRow {
        LoudRow {
            id,
            path: PathBuf::from(format!("/{id}.mp3")),
            record: record.into(),
            seconds: 60.0,
            modified: String::new(),
            known: None,
            fresh,
        }
    }

    #[test]
    fn records_are_due_whole() {
        let due = due(vec![row(1, "a", true), row(2, "a", false), row(3, "b", true), row(4, "c", false)]);
        let ids: Vec<Vec<i64>> = due.iter().map(|r| r.iter().map(|t| t.id).collect()).collect();
        assert_eq!(ids, vec![vec![1, 2], vec![4]]);
    }
}
