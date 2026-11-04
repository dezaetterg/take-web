//! Background SQLite worker.
//!
//! The UI thread must never wait on disk. `AsyncDb` owns a dedicated thread with
//! its own WAL connection and exposes:
//!
//! * fire-and-forget writes (`record_history_visit`, `set_setting`, ...),
//! * **batched** tracker counters (coalesced per domain, flushed every
//!   [`FLUSH_INTERVAL`] in a single transaction instead of one fsync per request),
//! * `query`, which runs a read on the worker and hands the result back on the
//!   GLib main loop.
//!
//! Every command that reads flushes pending writes first, so queries always observe
//! everything queued before them.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::database::BrowserDatabase;

pub const FLUSH_INTERVAL: Duration = Duration::from_millis(1500);

type Job = Box<dyn FnOnce(&BrowserDatabase) + Send + 'static>;
type MainCallback = Box<dyn FnOnce(Box<dyn Any + Send>)>;

thread_local! {
    // Completion callbacks are not `Send`; they never leave the main thread. Only an id
    // travels to the worker and back, so dropping an unexecuted job is always safe.
    static CALLBACKS: RefCell<HashMap<u64, MainCallback>> = RefCell::new(HashMap::new());
    static NEXT_CALLBACK_ID: Cell<u64> = const { Cell::new(1) };
}

fn run_callback(id: u64, value: Box<dyn Any + Send>) {
    if let Some(callback) = CALLBACKS.with(|c| c.borrow_mut().remove(&id)) {
        callback(value);
    }
}

enum Command {
    HistoryVisit {
        title: String,
        url: String,
    },
    TrackerBlocked {
        domain: String,
        rule: String,
    },
    FingerprintDefended {
        vector: String,
        count: u64,
    },
    Setting {
        key: String,
        value: String,
    },
    SitePermission {
        origin: String,
        perm_type: String,
        state: String,
    },
    ClearTrackers,
    Job(Job),
    #[cfg(test)]
    Flush(Sender<()>),
    Shutdown,
}

pub struct AsyncDb {
    tx: Option<Sender<Command>>,
    handle: Option<JoinHandle<()>>,
}

impl AsyncDb {
    /// Opens a second connection to `path` on a worker thread. Returns a disabled
    /// handle (all calls become no-ops) when the database is not file backed.
    pub fn open(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            log::info!("AsyncDb: in-memory store, background writer disabled");
            return Self {
                tx: None,
                handle: None,
            };
        };

        let path = path.to_path_buf();
        let (tx, rx) = mpsc::channel::<Command>();
        let spawned = thread::Builder::new()
            .name("take-db".into())
            .spawn(move || {
                let db = match BrowserDatabase::open(&path) {
                    Ok(db) => db,
                    Err(err) => {
                        log::error!("AsyncDb: cannot open {}: {err}", path.display());
                        return;
                    }
                };
                worker_loop(&db, rx);
            });

        match spawned {
            Ok(handle) => Self {
                tx: Some(tx),
                handle: Some(handle),
            },
            Err(err) => {
                log::error!("AsyncDb: cannot spawn worker thread: {err}");
                Self {
                    tx: None,
                    handle: None,
                }
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.tx.is_some()
    }

    fn send(&self, command: Command) {
        if let Some(tx) = &self.tx {
            if tx.send(command).is_err() {
                log::warn!("AsyncDb: worker has stopped, dropping command");
            }
        }
    }

    pub fn record_history_visit(&self, title: &str, url: &str) {
        self.send(Command::HistoryVisit {
            title: title.to_owned(),
            url: url.to_owned(),
        });
    }

    pub fn record_tracker_blocked(&self, domain: &str, rule: &str) {
        self.send(Command::TrackerBlocked {
            domain: domain.to_owned(),
            rule: rule.to_owned(),
        });
    }

    pub fn record_fingerprint_defended(&self, vector: &str, count: u64) {
        self.send(Command::FingerprintDefended {
            vector: vector.to_owned(),
            count,
        });
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        self.send(Command::Setting {
            key: key.to_owned(),
            value: value.to_owned(),
        });
    }

    pub fn set_site_permission(&self, origin: &str, perm_type: &str, state: &str) {
        self.send(Command::SitePermission {
            origin: origin.to_owned(),
            perm_type: perm_type.to_owned(),
            state: state.to_owned(),
        });
    }

    /// Drops queued-but-unwritten counters and wipes the table, preserving ordering
    /// with later queries (a plain synchronous clear could be undone by a pending flush).
    pub fn clear_tracker_stats(&self) {
        self.send(Command::ClearTrackers);
    }

    /// Runs `work` on the DB thread and `done` with its result on the GLib main loop.
    /// Must be called from the main thread. `done` does not have to be `Send`.
    pub fn query<T, W, D>(&self, work: W, done: D)
    where
        T: Send + 'static,
        W: FnOnce(&BrowserDatabase) -> T + Send + 'static,
        D: FnOnce(T) + 'static,
    {
        if self.tx.is_none() {
            return;
        }
        let id = NEXT_CALLBACK_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        CALLBACKS.with(|c| {
            c.borrow_mut().insert(
                id,
                Box::new(move |value: Box<dyn Any + Send>| {
                    if let Ok(value) = value.downcast::<T>() {
                        done(*value);
                    }
                }),
            );
        });
        self.send(Command::Job(Box::new(move |db| {
            let value: Box<dyn Any + Send> = Box::new(work(db));
            glib::MainContext::default().invoke(move || run_callback(id, value));
        })));
    }

    /// Blocks until everything queued so far is on disk (used on shutdown/tests).
    #[cfg(test)]
    pub fn flush_blocking(&self) {
        if let Some(tx) = &self.tx {
            let (ack_tx, ack_rx) = mpsc::channel();
            if tx.send(Command::Flush(ack_tx)).is_ok() {
                let _ = ack_rx.recv_timeout(Duration::from_secs(5));
            }
        }
    }
}

impl Drop for AsyncDb {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Command::Shutdown);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn flush_privacy_stats(
    db: &BrowserDatabase,
    pending_trackers: &mut HashMap<(String, String), u64>,
    pending_fp: &mut HashMap<String, u64>,
) {
    if pending_trackers.is_empty() && pending_fp.is_empty() {
        return;
    }
    let trackers_batch = std::mem::take(pending_trackers);
    let fp_batch = std::mem::take(pending_fp);
    let tracker_count = trackers_batch.len();
    let fp_count = fp_batch.len();

    let result = db.in_transaction(|db| {
        for ((domain, rule), n) in &trackers_batch {
            db.record_tracker_blocked_n(domain, rule, *n)?;
        }
        for (vector, n) in &fp_batch {
            db.record_fingerprint_defended_n(vector, *n)?;
        }
        Ok(())
    });
    match result {
        Ok(()) => log::debug!("AsyncDb: flushed {tracker_count} trackers, {fp_count} fp counters"),
        Err(err) => log::warn!("AsyncDb: privacy flush failed: {err}"),
    }
}

fn worker_loop(db: &BrowserDatabase, rx: mpsc::Receiver<Command>) {
    let mut pending_trackers: HashMap<(String, String), u64> = HashMap::new();
    let mut pending_fp: HashMap<String, u64> = HashMap::new();

    loop {
        let is_empty = pending_trackers.is_empty() && pending_fp.is_empty();
        let command = if is_empty {
            match rx.recv() {
                Ok(command) => command,
                Err(_) => break,
            }
        } else {
            match rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(command) => command,
                Err(RecvTimeoutError::Timeout) => {
                    flush_privacy_stats(db, &mut pending_trackers, &mut pending_fp);
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        };

        match command {
            Command::TrackerBlocked { domain, rule } => {
                *pending_trackers.entry((domain, rule)).or_insert(0) += 1;
            }
            Command::FingerprintDefended { vector, count } => {
                *pending_fp.entry(vector).or_insert(0) += count;
            }
            Command::HistoryVisit { title, url } => {
                if let Err(err) = db.record_history_visit(&title, &url) {
                    log::warn!("AsyncDb: history write failed: {err}");
                }
            }
            Command::Setting { key, value } => {
                if let Err(err) = db.set_setting(&key, &value) {
                    log::warn!("AsyncDb: setting write failed: {err}");
                }
            }
            Command::SitePermission {
                origin,
                perm_type,
                state,
            } => {
                if let Err(err) = db.set_site_permission(&origin, &perm_type, &state) {
                    log::warn!("AsyncDb: site permission write failed: {err}");
                }
            }
            Command::ClearTrackers => {
                pending_trackers.clear();
                pending_fp.clear();
                if let Err(err) = db.clear_tracker_stats() {
                    log::warn!("AsyncDb: clearing tracker stats failed: {err}");
                }
            }
            Command::Job(job) => {
                flush_privacy_stats(db, &mut pending_trackers, &mut pending_fp);
                job(db);
            }
            #[cfg(test)]
            Command::Flush(ack) => {
                flush_privacy_stats(db, &mut pending_trackers, &mut pending_fp);
                let _ = ack.send(());
            }
            Command::Shutdown => break,
        }
    }

    flush_privacy_stats(db, &mut pending_trackers, &mut pending_fp);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("take-asyncdb-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("test.db")
    }

    #[test]
    fn tracker_hits_are_coalesced_and_persisted_on_flush() {
        let path = temp_db_path("trackers");
        let main_db = BrowserDatabase::open(&path).unwrap();
        let writer = AsyncDb::open(Some(&path));
        assert!(writer.is_enabled());

        for _ in 0..50 {
            writer.record_tracker_blocked("ads.example", "ads");
        }
        writer.record_tracker_blocked("pixel.example", "analytics");
        writer.flush_blocking();

        let stats = main_db.get_all_tracker_stats().unwrap();
        let ads = stats.iter().find(|s| s.domain == "ads.example").unwrap();
        assert_eq!(ads.blocked_count, 50);
        assert_eq!(stats.len(), 2);

        drop(writer);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn fingerprint_hits_are_coalesced_and_persisted_on_flush() {
        let path = temp_db_path("fp_flush");
        let main_db = BrowserDatabase::open(&path).unwrap();
        let writer = AsyncDb::open(Some(&path));
        assert!(writer.is_enabled());

        for _ in 0..10 {
            writer.record_fingerprint_defended("canvas", 2);
        }
        writer.record_fingerprint_defended("webgl", 5);
        writer.flush_blocking();

        let stats = main_db.get_fingerprint_stats().unwrap();
        let canvas = stats.iter().find(|s| s.vector == "canvas").unwrap();
        assert_eq!(canvas.defended_count, 20);
        let webgl = stats.iter().find(|s| s.vector == "webgl").unwrap();
        assert_eq!(webgl.defended_count, 5);

        drop(writer);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn history_and_settings_are_written_off_thread() {
        let path = temp_db_path("history");
        let main_db = BrowserDatabase::open(&path).unwrap();
        let writer = AsyncDb::open(Some(&path));

        writer.record_history_visit("Example", "https://example.com/");
        writer.set_setting("theme", "dark");
        writer.flush_blocking();

        assert_eq!(
            main_db.get_setting("theme").unwrap().as_deref(),
            Some("dark")
        );
        assert_eq!(main_db.search_history("example", 5).unwrap().len(), 1);

        drop(writer);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn pending_trackers_flush_on_drop() {
        let path = temp_db_path("drop");
        let main_db = BrowserDatabase::open(&path).unwrap();
        {
            let writer = AsyncDb::open(Some(&path));
            writer.record_tracker_blocked("late.example", "ads");
        }
        assert_eq!(main_db.get_all_tracker_stats().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn clear_discards_pending_counters() {
        let path = temp_db_path("clear");
        let main_db = BrowserDatabase::open(&path).unwrap();
        let writer = AsyncDb::open(Some(&path));
        writer.record_tracker_blocked("old.example", "ads");
        writer.clear_tracker_stats();
        writer.record_tracker_blocked("new.example", "ads");
        writer.flush_blocking();
        let stats = main_db.get_all_tracker_stats().unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].domain, "new.example");
        drop(writer);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn disabled_handle_is_a_noop() {
        let writer = AsyncDb::open(None);
        assert!(!writer.is_enabled());
        writer.record_history_visit("x", "https://x.test/");
        writer.flush_blocking();
    }
}
