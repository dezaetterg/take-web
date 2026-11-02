use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use gtk4::gio::prelude::*;
use webkit6::Download;

static DOWNLOAD_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadStatus {
    Downloading,
    Finished,
    Failed,
    Cancelled,
}

pub struct DownloadItem {
    pub id: u64,
    pub download: Download,
    pub filename: String,
    pub destination: PathBuf,
    pub progress: f64,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub status: DownloadStatus,
    pub error_message: Option<String>,
    pub is_private: bool,
}

type DownloadListener = Box<dyn Fn()>;
type ListenerList = Rc<RefCell<Vec<DownloadListener>>>;
type ItemList = Rc<RefCell<Vec<Rc<RefCell<DownloadItem>>>>>;

pub struct DownloadManager {
    items: ItemList,
    listeners: ListenerList,
    custom_download_dir: RefCell<Option<PathBuf>>,
    ask_destination: Cell<bool>,
    parent_window: RefCell<Option<gtk4::Window>>,
}

impl DownloadManager {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            items: Rc::new(RefCell::new(Vec::new())),
            listeners: Rc::new(RefCell::new(Vec::new())),
            custom_download_dir: RefCell::new(None),
            ask_destination: Cell::new(false),
            parent_window: RefCell::new(None),
        })
    }

    /// Associates the main application window with FileDialog popups.
    pub fn set_parent_window(&self, win: Option<gtk4::Window>) {
        *self.parent_window.borrow_mut() = win;
    }

    /// Sets custom download directory from settings.
    pub fn set_download_dir(&self, dir: Option<PathBuf>) {
        *self.custom_download_dir.borrow_mut() = dir;
    }

    /// Sets whether to ask for destination location.
    pub fn set_ask_destination(&self, ask: bool) {
        self.ask_destination.set(ask);
    }

    /// Returns whether to ask for destination location.
    pub fn ask_destination(&self) -> bool {
        self.ask_destination.get()
    }

    /// Clears any in-RAM downloads that originated from private (incognito) browsing.
    pub fn clear_private_downloads(&self) {
        self.items.borrow_mut().retain(|it| !it.borrow().is_private);
        self.notify_listeners();
    }

    /// Returns active download directory (custom or ~/Downloads fallback).
    pub fn active_download_dir(&self) -> PathBuf {
        self.custom_download_dir
            .borrow()
            .clone()
            .unwrap_or_else(Self::default_download_dir)
    }

    /// Returns the user's Downloads directory (or ~/Downloads fallback).
    pub fn default_download_dir() -> PathBuf {
        glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join("Downloads")
        })
    }

    /// Handles a new download initiated by WebKit.
    pub fn handle_download(self: &Rc<Self>, download: &Download, is_private: bool) {
        let id = DOWNLOAD_COUNTER.fetch_add(1, Ordering::SeqCst);

        let item = Rc::new(RefCell::new(DownloadItem {
            id,
            download: download.clone(),
            filename: "downloading...".to_string(),
            destination: PathBuf::new(),
            progress: 0.0,
            received_bytes: 0,
            total_bytes: 0,
            status: DownloadStatus::Downloading,
            error_message: None,
            is_private,
        }));

        self.items.borrow_mut().push(Rc::clone(&item));
        self.notify_listeners();

        // 1. Decide destination dynamically using WebKit's resolved suggested filename
        let item_weak = Rc::downgrade(&item);
        let mgr_weak = Rc::downgrade(self);
        download.connect_decide_destination(move |dl, suggested| {
            let mgr = match mgr_weak.upgrade() {
                Some(m) => m,
                None => return false,
            };

            let req_uri = dl.request().and_then(|r| r.uri()).map(|s| s.to_string());
            let mime = dl
                .response()
                .and_then(|r| r.mime_type())
                .map(|s| s.to_string());

            let final_filename =
                Self::resolve_suggested_filename(suggested, req_uri.as_deref(), mime.as_deref());

            let target_dir = mgr.active_download_dir();
            if let Err(err) = std::fs::create_dir_all(&target_dir) {
                log::warn!(
                    "Could not create download directory {}: {err}",
                    target_dir.display()
                );
            }

            if mgr.ask_destination.get() {
                let file_dialog = gtk4::FileDialog::new();
                file_dialog.set_title("Сохранить файл как...");
                file_dialog.set_initial_name(Some(&final_filename));
                let initial_folder = gtk4::gio::File::for_path(&target_dir);
                file_dialog.set_initial_folder(Some(&initial_folder));

                let parent_win = mgr.parent_window.borrow().clone();
                let dl_clone = dl.clone();
                let item_w = item_weak.clone();
                let mgr_w = mgr_weak.clone();
                let fname_clone = final_filename.clone();

                file_dialog.save(
                    parent_win.as_ref(),
                    None::<&gtk4::gio::Cancellable>,
                    move |result| match result {
                        Ok(file) => {
                            if let Some(path) = file.path() {
                                let dest_str = path.to_string_lossy().to_string();
                                let chosen_name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or(fname_clone);
                                if let Some(it) = item_w.upgrade() {
                                    let mut guard = it.borrow_mut();
                                    guard.filename = chosen_name;
                                    guard.destination = path;
                                }
                                if let Some(m) = mgr_w.upgrade() {
                                    m.notify_listeners();
                                }
                                log::info!("FileDialog user selected: {dest_str}");
                                dl_clone.set_destination(&dest_str);
                                dl_clone.set_allow_overwrite(true);
                            } else {
                                dl_clone.cancel();
                            }
                        }
                        Err(err) => {
                            log::info!("FileDialog cancelled or failed: {err}");
                            dl_clone.cancel();
                            if let Some(it) = item_w.upgrade() {
                                let mut guard = it.borrow_mut();
                                guard.status = DownloadStatus::Cancelled;
                            }
                            if let Some(m) = mgr_w.upgrade() {
                                m.notify_listeners();
                            }
                        }
                    },
                );

                return true;
            }

            let destination = Self::unique_destination(&target_dir, &final_filename);
            let dest_str = destination.to_string_lossy().to_string();

            if let Some(it) = item_weak.upgrade() {
                let mut guard = it.borrow_mut();
                guard.filename = final_filename;
                guard.destination = destination;
            }
            mgr.notify_listeners();

            log::info!("Download destination resolved for suggested '{suggested}': {dest_str}");
            dl.set_destination(&dest_str);
            dl.set_allow_overwrite(false);
            true
        });

        // 2. Progress updates
        let item_weak = Rc::downgrade(&item);
        let mgr_weak = Rc::downgrade(self);
        download.connect_estimated_progress_notify(move |dl| {
            if let Some(item) = item_weak.upgrade() {
                let mut guard = item.borrow_mut();
                guard.progress = dl.estimated_progress();
                if let Some(resp) = dl.response() {
                    let total = resp.content_length();
                    if total > 0 {
                        guard.total_bytes = total;
                    }
                }
            }
            if let Some(mgr) = mgr_weak.upgrade() {
                mgr.notify_listeners();
            }
        });

        // 3. Received data
        let item_weak = Rc::downgrade(&item);
        let mgr_weak = Rc::downgrade(self);
        download.connect_received_data(move |dl, len| {
            if let Some(item) = item_weak.upgrade() {
                let mut guard = item.borrow_mut();
                guard.received_bytes = dl.received_data_length();
                if guard.total_bytes == 0 {
                    guard.total_bytes = guard.received_bytes.max(len);
                }
            }
            if let Some(mgr) = mgr_weak.upgrade() {
                mgr.notify_listeners();
            }
        });

        // 4. Download finished
        let item_weak = Rc::downgrade(&item);
        let mgr_weak = Rc::downgrade(self);
        download.connect_finished(move |_dl| {
            if let Some(item) = item_weak.upgrade() {
                let mut guard = item.borrow_mut();
                guard.status = DownloadStatus::Finished;
                guard.progress = 1.0;
                log::info!(
                    "Download id={} finished: {}",
                    guard.id,
                    guard.destination.display()
                );
            }
            if let Some(mgr) = mgr_weak.upgrade() {
                mgr.notify_listeners();
            }
        });

        // 5. Download failed
        let item_weak = Rc::downgrade(&item);
        let mgr_weak = Rc::downgrade(self);
        download.connect_failed(move |_, err| {
            if let Some(item) = item_weak.upgrade() {
                let mut guard = item.borrow_mut();
                if guard.status != DownloadStatus::Cancelled {
                    let msg = err.message().to_string();
                    if msg.to_lowercase().contains("cancell") {
                        guard.status = DownloadStatus::Cancelled;
                    } else {
                        guard.status = DownloadStatus::Failed;
                        guard.error_message = Some(msg);
                    }
                }
                log::info!(
                    "Download id={} finished with status {:?}",
                    guard.id,
                    guard.status
                );
            }
            if let Some(mgr) = mgr_weak.upgrade() {
                mgr.notify_listeners();
            }
        });
    }

    /// Cancels an in-progress download.
    pub fn cancel_download(&self, id: u64) {
        let items = self.items.borrow();
        if let Some(item) = items.iter().find(|i| i.borrow().id == id) {
            let mut guard = item.borrow_mut();
            if guard.status == DownloadStatus::Downloading {
                guard.download.cancel();
                guard.status = DownloadStatus::Cancelled;
                log::info!("Download id={} was cancelled by user", id);
            }
        }
        self.notify_listeners();
    }

    /// Registers a listener for download updates (for UI reactive rendering).
    pub fn add_listener<F: Fn() + 'static>(&self, listener: F) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    fn notify_listeners(&self) {
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    /// List of current downloads.
    pub fn items(&self) -> Vec<Rc<RefCell<DownloadItem>>> {
        self.items.borrow().clone()
    }

    /// Checks if any downloads are currently active.
    pub fn has_active_downloads(&self) -> bool {
        self.items
            .borrow()
            .iter()
            .any(|item| item.borrow().status == DownloadStatus::Downloading)
    }

    /// Resolves an optimal and safe filename using WebKit's suggested name,
    /// falling back to the URL path or response MIME type if generic.
    pub fn resolve_suggested_filename(
        suggested: &str,
        uri: Option<&str>,
        mime_type: Option<&str>,
    ) -> String {
        let sanitized = Self::sanitize_filename(suggested);
        let base_name = if sanitized != "download.bin" && !sanitized.is_empty() {
            sanitized
        } else if let Some(raw_uri) = uri {
            url::Url::parse(raw_uri)
                .ok()
                .and_then(|u| {
                    u.path_segments()?
                        .next_back()
                        .map(Self::sanitize_filename)
                        .filter(|s| s != "download.bin" && !s.is_empty())
                })
                .unwrap_or_else(|| "download".to_string())
        } else {
            "download".to_string()
        };

        let mime_ext = mime_type.and_then(|m| {
            let ext = match m.trim().to_lowercase().as_str() {
                "application/pdf" => "pdf",
                "image/png" => "png",
                "image/jpeg" | "image/jpg" => "jpg",
                "image/webp" => "webp",
                "image/gif" => "gif",
                "image/svg+xml" => "svg",
                "text/html" => "html",
                "text/plain" => "txt",
                "application/json" => "json",
                "application/zip" => "zip",
                "application/gzip" | "application/x-gzip" => "tar.gz",
                "application/x-tar" => "tar",
                "video/mp4" => "mp4",
                "video/webm" => "webm",
                "audio/mpeg" | "audio/mp3" => "mp3",
                "audio/ogg" => "ogg",
                _ => "",
            };
            if !ext.is_empty() {
                Some(ext)
            } else {
                None
            }
        });

        // If the filename has no extension and we know the MIME extension, append it
        if !base_name.contains('.') {
            if let Some(ext) = mime_ext {
                return format!("{base_name}.{ext}");
            }
        }

        if base_name == "download" {
            "download.bin".to_string()
        } else {
            base_name
        }
    }

    /// Sanitizes an untrusted filename suggested by a remote server or Content-Disposition.
    /// Strips directory components, path traversal tokens (`..`), control characters,
    /// and leading periods. Falls back to "download.bin" if empty.
    pub fn sanitize_filename(suggested: &str) -> String {
        let base = std::path::Path::new(suggested)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(suggested);

        let filtered: String = base
            .chars()
            .filter(|c| !c.is_control() && *c != '/' && *c != '\\' && *c != '\0')
            .collect();

        let trimmed = filtered.trim().trim_start_matches('.');
        if trimmed.is_empty() || trimmed == ".." {
            "download.bin".to_string()
        } else if trimmed.len() > 200 {
            trimmed[..200].to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn unique_destination(dir: &Path, filename: &str) -> PathBuf {
        let safe_filename = Self::sanitize_filename(filename);
        let candidate = dir.join(&safe_filename);
        if !candidate.exists() {
            return candidate;
        }

        let (stem, ext) = match safe_filename.rsplit_once('.') {
            Some((s, e)) => (s, format!(".{e}")),
            None => (safe_filename.as_str(), String::new()),
        };

        for i in 1..1000 {
            let next = dir.join(format!("{stem} ({i}){ext}"));
            if !next.exists() {
                return next;
            }
        }
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(
            DownloadManager::sanitize_filename("valid_file.pdf"),
            "valid_file.pdf"
        );
        assert_eq!(
            DownloadManager::sanitize_filename("../../../etc/passwd"),
            "passwd"
        );
        assert_eq!(
            DownloadManager::sanitize_filename("/home/user/.bashrc"),
            "bashrc"
        );
        assert_eq!(DownloadManager::sanitize_filename(".."), "download.bin");
        assert_eq!(DownloadManager::sanitize_filename(""), "download.bin");
        assert_eq!(
            DownloadManager::sanitize_filename("test\0injection.bin"),
            "testinjection.bin"
        );
        assert_eq!(DownloadManager::sanitize_filename("...hidden"), "hidden");
    }

    #[test]
    fn test_resolve_suggested_filename() {
        // 1. Direct suggested filename from WebKit
        assert_eq!(
            DownloadManager::resolve_suggested_filename("presentation.pptx", None, None),
            "presentation.pptx"
        );
        // 2. Traversal in suggested filename sanitized
        assert_eq!(
            DownloadManager::resolve_suggested_filename("../../document.pdf", None, None),
            "document.pdf"
        );
        // 3. Fallback to URI path when suggested is generic
        assert_eq!(
            DownloadManager::resolve_suggested_filename(
                "download.bin",
                Some("https://example.com/assets/images/logo.png?v=2"),
                None
            ),
            "logo.png"
        );
        // 4. Fallback to MIME type when suggested and URI are generic
        assert_eq!(
            DownloadManager::resolve_suggested_filename(
                "",
                Some("https://example.com/api/export"),
                Some("application/pdf")
            ),
            "export.pdf"
        );
        assert_eq!(
            DownloadManager::resolve_suggested_filename("", None, Some("image/jpeg")),
            "download.jpg"
        );
        // 5. Total fallback
        assert_eq!(
            DownloadManager::resolve_suggested_filename("", None, None),
            "download.bin"
        );
    }

    #[test]
    fn test_unique_destination_non_existent() {
        let temp_dir = std::env::temp_dir().join("echo_download_test_unique");
        let dest = DownloadManager::unique_destination(&temp_dir, "archive.tar.gz");
        assert_eq!(dest, temp_dir.join("archive.tar.gz"));
    }

    #[test]
    fn test_unique_destination_traversal_prevented() {
        let temp_dir = std::env::temp_dir().join("echo_download_test_trav");
        let dest = DownloadManager::unique_destination(&temp_dir, "../../evil.sh");
        assert_eq!(dest, temp_dir.join("evil.sh"));
    }

    #[test]
    fn test_unique_destination_handles_collisions() {
        let temp_dir = std::env::temp_dir().join("echo_download_test_collision");
        let _ = std::fs::create_dir_all(&temp_dir);
        let first_file = temp_dir.join("document.pdf");
        let _ = std::fs::write(&first_file, b"test");

        let dest = DownloadManager::unique_destination(&temp_dir, "document.pdf");
        assert_eq!(dest, temp_dir.join("document (1).pdf"));

        let _ = std::fs::remove_file(&first_file);
        let _ = std::fs::remove_dir(&temp_dir);
    }
}
