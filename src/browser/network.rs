use std::cell::Cell;
use std::path::{Path, PathBuf};

use gtk4::gio;
use webkit6::{
    CookieAcceptPolicy, MemoryPressureSettings, NetworkSession, TLSErrorsPolicy, WebsiteDataTypes,
};

/// Target maximum disk cache size: 500 MB (in bytes).
pub const MAX_DISK_CACHE_BYTES: u64 = 500 * 1024 * 1024;

/// Standard Accept header for Safari on macOS (eliminates libsoup/WebKitGTK fingerprinting)
pub const HARDENED_ACCEPT_HEADER: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
/// Standard Accept-Encoding header
pub const HARDENED_ACCEPT_ENCODING_HEADER: &str = "gzip, deflate, br";
/// Standard User-Agent header for Hardened profile
pub const HARDENED_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15";

#[derive(Clone)]
pub struct NetworkManager {
    session: NetworkSession,
    https_only: Cell<bool>,
}

impl NetworkManager {
    /// Creates and initializes the shared NetworkSession with persistent disk cache,
    /// ITP enabled, strict TLS policy, and memory limits.
    pub fn new() -> Self {
        let mut cache_dir = glib::user_cache_dir();
        cache_dir.push("takeweb");
        cache_dir.push("http-cache");

        let mut data_dir = glib::user_data_dir();
        data_dir.push("takeweb");
        data_dir.push("network-data");

        if let Err(err) = std::fs::create_dir_all(&cache_dir) {
            log::warn!(
                "Could not create http-cache directory {}: {err}",
                cache_dir.display()
            );
        }
        if let Err(err) = std::fs::create_dir_all(&data_dir) {
            log::warn!(
                "Could not create network data directory {}: {err}",
                data_dir.display()
            );
        }

        let cache_str = cache_dir.to_str();
        let data_str = data_dir.to_str();

        // 1. Persistent NetworkSession using ~/.cache/takeweb/http-cache/
        let session = NetworkSession::new(data_str, cache_str);

        // 2. Intelligent Tracking Prevention (ITP)
        session.set_itp_enabled(true);

        // 3. Enable favicon database
        if let Some(dm) = session.website_data_manager() {
            dm.set_favicons_enabled(true);
        }

        // 4. Strict TLS policy for HTTPS-Only mode
        session.set_tls_errors_policy(TLSErrorsPolicy::Fail);

        // 5. Memory pressure configuration:
        // Dynamically configured according to physical RAM and memory pressure:
        // <=4GB: 768MB, <=8GB: 1024MB, >8GB: 1536MB with progressive cleanup thresholds.
        let (limit_mb, cons_threshold, strict_threshold, kill_threshold) =
            crate::browser::performance::detect_memory_pressure_plan();
        let mut pressure = MemoryPressureSettings::new();
        pressure.set_memory_limit(limit_mb);
        // Set in descending order (kill -> strict -> conservative) so that
        // conservative < strict < kill invariant holds at every step against defaults.
        pressure.set_kill_threshold(kill_threshold);
        pressure.set_strict_threshold(strict_threshold);
        pressure.set_conservative_threshold(cons_threshold);
        pressure.set_poll_interval(30.0);
        NetworkSession::set_memory_pressure_settings(&mut pressure);

        // 6. Asynchronously prune disk cache off the UI startup critical path
        let prune_path = cache_dir.clone();
        std::thread::spawn(move || {
            Self::prune_disk_cache_dir(&prune_path, MAX_DISK_CACHE_BYTES);
        });

        log::info!(
            "NetworkSession initialized: cache_dir={}, itp=true, https_only=true, memory_limit={}MB (cons={:.2}, strict={:.2}, kill={:.2})",
            cache_dir.display(),
            limit_mb,
            cons_threshold,
            strict_threshold,
            kill_threshold,
        );

        Self {
            session,
            https_only: Cell::new(true),
        }
    }

    /// Creates an isolated in-memory ephemeral NetworkSession for Private Browsing.
    pub fn new_ephemeral() -> Self {
        let session = NetworkSession::new_ephemeral();
        session.set_itp_enabled(true);
        if let Some(dm) = session.website_data_manager() {
            dm.set_favicons_enabled(false);
        }
        session.set_tls_errors_policy(TLSErrorsPolicy::Fail);
        Self {
            session,
            https_only: Cell::new(true),
        }
    }

    /// Accessor for the underlying WebKit NetworkSession.
    pub fn session(&self) -> &NetworkSession {
        &self.session
    }

    /// Toggles Intelligent Tracking Prevention (ITP).
    pub fn set_itp_enabled(&self, enabled: bool) {
        self.session.set_itp_enabled(enabled);
        log::info!("ITP set to: {enabled}");
    }

    /// Toggles HTTPS-Only mode enforcement.
    pub fn set_https_only(&self, enabled: bool) {
        self.https_only.set(enabled);
        log::info!("HTTPS-Only mode set to: {enabled}");
    }

    /// Updates WebKit cookie accept policy.
    pub fn set_cookie_policy(&self, policy_str: &str) {
        if let Some(cm) = self.session.cookie_manager() {
            let policy = match policy_str {
                "always" => CookieAcceptPolicy::Always,
                "never" => CookieAcceptPolicy::Never,
                _ => CookieAcceptPolicy::NoThirdParty,
            };
            cm.set_accept_policy(policy);
            log::info!("Cookie accept policy updated to: {policy_str}");
        }
    }

    /// Checks if a URL should be upgraded from `http://` to `https://`.
    /// Localhost and loopback IPs are spared for developer testing.
    pub fn enforce_https_upgrade(url: &str) -> Option<String> {
        let trimmed = url.trim();
        if let Some(rest) = trimmed.strip_prefix("http://") {
            if !rest.starts_with("localhost")
                && !rest.starts_with("127.0.0.1")
                && !rest.starts_with("[::1]")
            {
                return Some(format!("https://{rest}"));
            }
        }
        None
    }

    /// Calculates recursive directory size in bytes.
    pub fn get_dir_size(dir: &Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = entry.metadata() {
                        total += meta.len();
                    }
                } else if path.is_dir() {
                    total += Self::get_dir_size(&path);
                }
            }
        }
        total
    }

    /// Recursively prunes oldest cache files if total size exceeds `max_bytes`.
    pub fn prune_disk_cache_dir(dir: &Path, max_bytes: u64) {
        let current_size = Self::get_dir_size(dir);
        if current_size <= max_bytes {
            return;
        }

        log::info!(
            "Disk cache size ({} bytes) exceeds limit ({} bytes), pruning oldest cache files...",
            current_size,
            max_bytes
        );

        let mut files = Vec::new();
        Self::collect_files(dir, &mut files);

        // Sort oldest first
        files.sort_by_key(|(_, _, modified)| *modified);

        let mut to_free = current_size.saturating_sub(max_bytes);
        for (path, size, _) in files {
            if to_free == 0 {
                break;
            }
            if let Err(err) = std::fs::remove_file(&path) {
                log::warn!("Could not remove cache file {}: {err}", path.display());
            } else {
                to_free = to_free.saturating_sub(size);
            }
        }
    }

    fn collect_files(dir: &Path, files: &mut Vec<(PathBuf, u64, std::time::SystemTime)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = entry.metadata() {
                        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                        files.push((path, meta.len(), mtime));
                    }
                } else if path.is_dir() {
                    Self::collect_files(&path, files);
                }
            }
        }
    }

    /// Clears the WebKit disk and memory caches via WebsiteDataManager.
    pub fn clear_cache(&self) {
        if let Some(dm) = self.session.website_data_manager() {
            dm.clear(
                WebsiteDataTypes::DISK_CACHE | WebsiteDataTypes::MEMORY_CACHE,
                glib::TimeSpan::from_seconds(0),
                None::<&gio::Cancellable>,
                |res| {
                    if let Err(err) = res {
                        log::warn!("Failed to clear WebKit cache: {err}");
                    } else {
                        log::info!("WebKit HTTP cache cleared successfully");
                    }
                },
            );
        }
    }

    /// Standardizes outgoing request headers to match standard Safari on macOS
    /// when running under the Hardened profile.
    pub fn sanitize_request_headers(request: &webkit6::URIRequest) {
        if let Some(headers) = request.http_headers() {
            let should_replace_accept = match headers.one("Accept") {
                Some(acc) => acc.contains("text/html") || acc.contains("q=0.01"),
                None => true,
            };
            if should_replace_accept {
                headers.replace("Accept", HARDENED_ACCEPT_HEADER);
            }
            let accept_lang = crate::i18n::current_language().accept_language_header();
            headers.replace("Accept-Language", accept_lang);
            headers.replace("Accept-Encoding", HARDENED_ACCEPT_ENCODING_HEADER);
            headers.replace("User-Agent", HARDENED_USER_AGENT);

            // Remove Chromium / Linux Client Hints headers
            headers.remove("Sec-CH-UA");
            headers.remove("Sec-CH-UA-Mobile");
            headers.remove("Sec-CH-UA-Platform");
            headers.remove("Sec-CH-UA-Model");
            headers.remove("Sec-CH-UA-Platform-Version");
        }
    }
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enforce_https_upgrade() {
        assert_eq!(
            NetworkManager::enforce_https_upgrade("http://example.com"),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            NetworkManager::enforce_https_upgrade("http://apple.com/mac"),
            Some("https://apple.com/mac".to_string())
        );
        // Spares localhost and loopbacks
        assert_eq!(
            NetworkManager::enforce_https_upgrade("http://localhost:3000"),
            None
        );
        assert_eq!(
            NetworkManager::enforce_https_upgrade("http://127.0.0.1:8080"),
            None
        );
        assert_eq!(
            NetworkManager::enforce_https_upgrade("http://[::1]:8080"),
            None
        );
        // Already https or custom schemes
        assert_eq!(
            NetworkManager::enforce_https_upgrade("https://example.com"),
            None
        );
        assert_eq!(NetworkManager::enforce_https_upgrade("take://start"), None);
    }

    #[test]
    fn test_cache_dir_pruning() {
        let temp_dir = std::env::temp_dir().join("take_cache_test");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let file1 = temp_dir.join("cache_1.bin");
        let file2 = temp_dir.join("cache_2.bin");

        let _ = std::fs::write(&file1, vec![0u8; 1000]);
        // Slight pause to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(15));
        let _ = std::fs::write(&file2, vec![0u8; 1000]);

        assert_eq!(NetworkManager::get_dir_size(&temp_dir), 2000);

        // Prune to 1500 bytes (should delete file1)
        NetworkManager::prune_disk_cache_dir(&temp_dir, 1500);
        assert!(NetworkManager::get_dir_size(&temp_dir) <= 1500);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sanitize_request_headers_hardened() {
        let req = webkit6::URIRequest::new("https://example.com");
        if let Some(headers) = req.http_headers() {
            headers.append("Accept", "text/html, */*; q=0.01");
            headers.append("Accept-Language", "ru-RU");
            headers.append("Sec-CH-UA", "\"Chromium\";v=\"120\"");
        }
        NetworkManager::sanitize_request_headers(&req);
        if let Some(headers) = req.http_headers() {
            assert_eq!(
                headers.one("Accept").as_deref(),
                Some(HARDENED_ACCEPT_HEADER)
            );
            assert_eq!(
                headers.one("Accept-Language").as_deref(),
                Some(crate::i18n::current_language().accept_language_header())
            );
            assert_eq!(
                headers.one("Accept-Encoding").as_deref(),
                Some(HARDENED_ACCEPT_ENCODING_HEADER)
            );
            assert_eq!(
                headers.one("User-Agent").as_deref(),
                Some(HARDENED_USER_AGENT)
            );
            assert_eq!(headers.one("Sec-CH-UA"), None);
        }
    }

    #[test]
    fn test_sanitize_request_headers_preserves_subresources() {
        let req = webkit6::URIRequest::new("https://example.com/image.png");
        if let Some(headers) = req.http_headers() {
            headers.append("Accept", "image/webp,image/*;q=0.8");
        }
        NetworkManager::sanitize_request_headers(&req);
        if let Some(headers) = req.http_headers() {
            assert_eq!(
                headers.one("Accept").as_deref(),
                Some("image/webp,image/*;q=0.8")
            );
        }
    }
}
