use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use webkit6::prelude::*;
use webkit6::{
    CacheModel, HardwareAccelerationPolicy, Settings, UserContentFilter, UserContentFilterStore,
    UserContentInjectedFrames, UserContentManager, UserScript, UserScriptInjectionTime, WebContext,
    WebView,
};

use super::network::NetworkManager;
use super::profile::BrowserProfile;
use super::scheme_handler;

pub type TrackerCallback = Rc<dyn Fn(&str, &str)>;
pub type FingerprintCallback = Rc<dyn Fn(&str, u64)>;
pub type SettingsChangeCallback = Rc<dyn Fn(&str, &str)>;
pub type TranslateIpcCallback = Rc<dyn Fn(&str)>;
pub type ReaderModeCallback = Rc<dyn Fn(bool, &str)>;
pub type ActionCallback = Rc<dyn Fn(&str)>;
pub type MediaIpcCallback = Rc<dyn Fn(&str, &str, &str, &str)>;
pub type EphemeralSessionCallback = Rc<dyn Fn(&webkit6::NetworkSession)>;

pub struct BrowserEngine {
    web_context: WebContext,
    network_manager: NetworkManager,
    ephemeral_network_manager: RefCell<Option<NetworkManager>>,
    web_settings: Settings,
    internal_settings: Settings,
    profile: Rc<RefCell<BrowserProfile>>,
    work_user_content_manager: UserContentManager,
    hardened_user_content_manager: UserContentManager,
    content_filter: Rc<RefCell<Option<UserContentFilter>>>,
    tracker_callback: Rc<RefCell<Option<TrackerCallback>>>,
    fingerprint_callback: Rc<RefCell<Option<FingerprintCallback>>>,
    settings_callback: Rc<RefCell<Option<SettingsChangeCallback>>>,
    translate_callback: Rc<RefCell<Option<TranslateIpcCallback>>>,
    translator_host_callback: Rc<RefCell<Option<TranslateIpcCallback>>>,
    reader_mode_callback: Rc<RefCell<Option<ReaderModeCallback>>>,
    action_callback: Rc<RefCell<Option<ActionCallback>>>,
    media_callback: Rc<RefCell<Option<MediaIpcCallback>>>,
    ephemeral_session_callback: Rc<RefCell<Option<EphemeralSessionCallback>>>,
    ipc_token: String,
}

impl BrowserEngine {
    pub fn new(profile: BrowserProfile, test_mode: bool) -> Self {
        let web_context = WebContext::default().unwrap_or_default();

        let is_low_mem = crate::browser::performance::detect_meminfo_ratio()
            .map(crate::browser::performance::low_memory)
            .unwrap_or(false);

        // Under normal memory: WebBrowser cache model enables full in-memory caching.
        // Under low memory: DocumentBrowser model avoids holding large page trees in RAM.
        if is_low_mem {
            web_context.set_cache_model(CacheModel::DocumentBrowser);
        } else {
            web_context.set_cache_model(CacheModel::WebBrowser);
        }

        // Internal pages are served from memory over `take://`; no file:// access needed.
        scheme_handler::register(&web_context);

        // Set WebKit preferred languages according to configured UI language
        let preferred = crate::i18n::current_language().webkit_preferred_languages();
        let spell = crate::i18n::current_language().spell_checking_languages();
        web_context.set_preferred_languages(preferred);
        web_context.set_spell_checking_languages(spell);

        // WebKit Bubblewrap sandbox path configuration for fonts and system resources
        Self::configure_sandbox_paths(&web_context);

        // Persistent NetworkSession with 500 MB disk cache in ~/.cache/takeweb/http-cache/,
        // ITP enabled, and HTTPS-Only / strict TLS policy.
        let network_manager = NetworkManager::new();

        let ipc_token = glib::uuid_string_random().to_string();

        let work_user_content_manager = UserContentManager::new();
        Self::setup_internal_handlers(&work_user_content_manager);

        let hardened_user_content_manager = UserContentManager::new();
        Self::setup_internal_handlers(&hardened_user_content_manager);

        // Inject cryptographic session token strictly into internal take:// origins
        let token_js = format!(
            "window.__TAKE_IPC_TOKEN__ = Object.freeze('{}');",
            ipc_token
        );
        let token_script = UserScript::new(
            &token_js,
            UserContentInjectedFrames::AllFrames,
            UserScriptInjectionTime::Start,
            &["take://*"],
            &[],
        );
        work_user_content_manager.add_script(&token_script);
        hardened_user_content_manager.add_script(&token_script);

        // Inject local Bergamot WASM page translator & floating selection popup into web pages
        let translator_script = UserScript::new(
            crate::browser::translator::PAGE_TRANSLATOR_SCRIPT,
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::End,
            &[],
            &["take://*"],
        );
        work_user_content_manager.add_script(&translator_script);
        hardened_user_content_manager.add_script(&translator_script);

        // Inject Reader Mode (DOM article extractor, dark/sepia themes, clean typography)
        let reader_script = UserScript::new(
            crate::browser::reader::READER_MODE_SCRIPT,
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::End,
            &[],
            &["take://*"],
        );
        work_user_content_manager.add_script(&reader_script);
        hardened_user_content_manager.add_script(&reader_script);

        // Inject MPRIS media playback listener for audio/video media session events
        let media_script = UserScript::new(
            crate::browser::mpris::MEDIA_LISTENER_SCRIPT,
            UserContentInjectedFrames::AllFrames,
            UserScriptInjectionTime::End,
            &[],
            &[],
        );
        work_user_content_manager.add_script(&media_script);
        hardened_user_content_manager.add_script(&media_script);

        // Inject stealth Anti-Fingerprinting shield into Hardened profile
        let anti_fp_script = crate::privacy::anti_fingerprint::create_user_script();
        hardened_user_content_manager.add_script(&anti_fp_script);

        if test_mode {
            log::info!("engine: test_mode is ON, injecting console interceptor");
            Self::setup_console_interceptor(&work_user_content_manager);
            Self::setup_console_interceptor(&hardened_user_content_manager);
        } else {
            log::info!("engine: test_mode is OFF, skipping console interceptor");
        }

        let web_settings = Settings::new();
        let internal_settings = Settings::new();

        // Hardware acceleration & modern web engine performance
        for s in [&web_settings, &internal_settings] {
            s.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
            s.set_enable_2d_canvas_acceleration(true);
            s.set_enable_webgl(true);
            s.set_enable_javascript(true);
            s.set_enable_developer_extras(true);
            s.set_enable_smooth_scrolling(true);
            s.set_enable_back_forward_navigation_gestures(true);
            s.set_enable_page_cache(!is_low_mem);
            s.set_enable_site_specific_quirks(true);
            s.set_enable_encrypted_media(true);
            s.set_enable_media_capabilities(true);
            s.set_enable_write_console_messages_to_stdout(test_mode);
            s.set_allow_file_access_from_file_urls(false);
            s.set_allow_universal_access_from_file_urls(false);
            s.set_enable_media(true);
            s.set_enable_mediasource(true);
            s.set_enable_webaudio(true);
            s.set_enable_fullscreen(true);
            s.set_media_playback_requires_user_gesture(false);
            s.set_enable_webrtc(true);
        }

        if profile == BrowserProfile::Hardened {
            web_settings.set_user_agent(Some(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15",
            ));
            web_settings.set_enable_mock_capture_devices(false);
        }

        let content_filter = Rc::new(RefCell::new(None));
        Self::init_content_filter(
            &content_filter,
            &[&work_user_content_manager, &hardened_user_content_manager],
        );

        Self {
            web_context,
            network_manager,
            ephemeral_network_manager: RefCell::new(None),
            web_settings,
            internal_settings,
            profile: Rc::new(RefCell::new(profile)),
            work_user_content_manager,
            hardened_user_content_manager,
            content_filter,
            tracker_callback: Rc::new(RefCell::new(None)),
            fingerprint_callback: Rc::new(RefCell::new(None)),
            settings_callback: Rc::new(RefCell::new(None)),
            translate_callback: Rc::new(RefCell::new(None)),
            translator_host_callback: Rc::new(RefCell::new(None)),
            reader_mode_callback: Rc::new(RefCell::new(None)),
            action_callback: Rc::new(RefCell::new(None)),
            media_callback: Rc::new(RefCell::new(None)),
            ephemeral_session_callback: Rc::new(RefCell::new(None)),
            ipc_token,
        }
    }

    /// Whitelists essential system directories in the WebKit Bubblewrap sandbox (fonts, icons, fontconfig).
    /// WebKitGTK requires these paths to exist before any web process is spawned.
    fn configure_sandbox_paths(web_context: &WebContext) {
        use std::path::PathBuf;

        let mut candidate_paths: Vec<PathBuf> = vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
            PathBuf::from("/etc/fonts"),
            PathBuf::from("/var/cache/fontconfig"),
            PathBuf::from("/usr/share/icons"),
            PathBuf::from("/usr/share/pixmaps"),
        ];

        let user_data = glib::user_data_dir();
        candidate_paths.push(user_data.join("fonts"));
        candidate_paths.push(user_data.join("icons"));

        let user_cache = glib::user_cache_dir();
        candidate_paths.push(user_cache.join("fontconfig"));

        let home = glib::home_dir();
        candidate_paths.push(home.join(".fonts"));
        candidate_paths.push(home.join(".local").join("share").join("fonts"));

        let mut added_count = 0;
        for path in candidate_paths {
            if path.exists() {
                web_context.add_path_to_sandbox(&path, true);
                added_count += 1;
            }
        }
        log::info!(
            "engine: configured WebKit sandbox, whitelisted {added_count} system directories"
        );
    }

    fn init_content_filter(
        filter_slot: &Rc<RefCell<Option<UserContentFilter>>>,
        ucms: &[&UserContentManager],
    ) {
        let mut filter_dir = glib::user_cache_dir();
        filter_dir.push("takeweb");
        filter_dir.push("filters");
        if let Err(err) = std::fs::create_dir_all(&filter_dir) {
            log::warn!("could not create filter cache dir: {err}");
        }

        static EMBEDDED_DEFAULT_RULES: &[u8] =
            include_bytes!("../../resources/filters/default_rules.json");
        let rules_bytes = glib::Bytes::from_static(EMBEDDED_DEFAULT_RULES);

        let store = UserContentFilterStore::new(&filter_dir.to_string_lossy());
        let slot = filter_slot.clone();
        let ucm_clones: Vec<UserContentManager> = ucms.iter().map(|u| (*u).clone()).collect();
        store.save(
            "take_default_filters",
            &rules_bytes,
            None::<&gio::Cancellable>,
            move |result| match result {
                Ok(filter) => {
                    log::info!("Compiled and loaded WebKit content blocker rules");
                    for ucm in &ucm_clones {
                        ucm.add_filter(&filter);
                    }
                    *slot.borrow_mut() = Some(filter);
                }
                Err(err) => {
                    log::warn!("Could not compile WebKit content blocker rules: {err}");
                }
            },
        );
    }

    /// Dynamically updates WebKit cache model and page cache state when host memory pressure changes.
    pub fn update_memory_pressure_mode(&self, low_mem: bool) {
        if low_mem {
            self.web_context
                .set_cache_model(CacheModel::DocumentBrowser);
            self.web_settings.set_enable_page_cache(false);
        } else {
            self.web_context.set_cache_model(CacheModel::WebBrowser);
            self.web_settings.set_enable_page_cache(true);
        }
        log::info!("engine: cache_model updated (low_mem={low_mem})");
    }

    /// Returns the session-scoped secret IPC token required for first-party IPC communications.
    pub fn ipc_token(&self) -> &str {
        &self.ipc_token
    }

    /// Registers the message handlers used by first-party pages.
    pub fn setup_internal_handlers(_ucm: &UserContentManager) {
        // Obsolete unauthenticated handlers removed. All first-party IPC is now authenticated
        // through `settingsChange` with session token verification.
    }

    /// Registers the trackerBlocked message handler to observe and record blocked tracker attempts.
    pub fn setup_tracker_block_listener<F: Fn(&str, &str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.tracker_callback.borrow_mut() = Some(callback.clone());
        for ucm in [
            &self.work_user_content_manager,
            &self.hardened_user_content_manager,
        ] {
            ucm.register_script_message_handler("trackerBlocked", None);
            let cb = callback.clone();
            ucm.connect_script_message_received(Some("trackerBlocked"), move |_, val| {
                let raw = val.to_str();
                let json_str = raw.as_str();
                if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(url) = obj.get("url").and_then(|u| u.as_str()) {
                        if url.len() > 2048 {
                            log::warn!(
                                "SECURITY: trackerBlocked ignored URL exceeding 2048 characters"
                            );
                            return;
                        }
                        let rule = obj
                            .get("rule")
                            .and_then(|r| r.as_str())
                            .unwrap_or("Content Filter");
                        if rule.len() > 128 {
                            return;
                        }
                        if let Some((domain, classified_rule)) =
                            crate::privacy::tracker_stats::classify_tracker(url)
                        {
                            cb(&domain, &classified_rule);
                        } else if let Ok(parsed_url) = url::Url::parse(url) {
                            if let Some(host) = parsed_url.host_str() {
                                cb(host, rule);
                            }
                        } else if url.contains("gen_204") {
                            cb("google.com/gen_204", "Google Telemetry (gen_204)");
                        } else if url.contains("client_204") {
                            cb("google.com/client_204", "Google Telemetry (client_204)");
                        }
                    }
                }
            });
        }
    }

    /// Registers the fingerprintDefended message handler to receive client-side protection stats.
    pub fn setup_fingerprint_listener<F: Fn(&str, u64) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.fingerprint_callback.borrow_mut() = Some(callback.clone());
        for ucm in [
            &self.work_user_content_manager,
            &self.hardened_user_content_manager,
        ] {
            ucm.register_script_message_handler("fingerprintDefended", None);
            let cb = callback.clone();
            ucm.connect_script_message_received(Some("fingerprintDefended"), move |_, val| {
                let raw = val.to_str();
                let json_str = raw.as_str();
                if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(vector) = obj.get("vector").and_then(|v| v.as_str()) {
                        let is_valid = matches!(
                            vector,
                            "canvas"
                                | "webgl"
                                | "audio"
                                | "fonts"
                                | "screen"
                                | "navigator"
                                | "timezone"
                                | "touch"
                                | "webrtc"
                                | "timers"
                        );
                        if !is_valid {
                            log::warn!(
                                "SECURITY: fingerprintDefended rejected invalid vector {vector:?}"
                            );
                            return;
                        }
                        let count = obj
                            .get("count")
                            .and_then(|c| c.as_u64())
                            .unwrap_or(1)
                            .clamp(1, 100);
                        cb(vector, count);
                    }
                }
            });
        }
    }

    /// Registers the settingsChange message handler to observe and persist settings from first-party pages.
    /// Strictly requires a matching session token to prevent untrusted web pages from mutating browser configuration.
    pub fn setup_settings_change_listener<F: Fn(&str, &str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.settings_callback.borrow_mut() = Some(callback.clone());
        let expected_token = self.ipc_token.clone();
        for ucm in [
            &self.work_user_content_manager,
            &self.hardened_user_content_manager,
        ] {
            ucm.register_script_message_handler("settingsChange", None);
            let cb = callback.clone();
            let token_clone = expected_token.clone();
            ucm.connect_script_message_received(Some("settingsChange"), move |_, val| {
                let raw = val.to_str();
                let json_str = raw.as_str();
                if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let msg_token = obj.get("token").and_then(|t| t.as_str()).unwrap_or("");
                    if msg_token.is_empty() || msg_token != token_clone.as_str() {
                        log::warn!("SECURITY ALERT: Blocked unauthorized settingsChange IPC message (invalid or missing session token)!");
                        return;
                    }
                    if let (Some(k), Some(v)) = (
                        obj.get("key").and_then(|k| k.as_str()),
                        obj.get("value").and_then(|v| v.as_str()),
                    ) {
                        cb(k, v);
                    }
                }
            });
        }
    }

    /// Registers the `takeTranslate` IPC handler for the local Bergamot WASM translator.
    pub fn setup_translate_listener<F: Fn(&str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.translate_callback.borrow_mut() = Some(callback.clone());
        for ucm in [
            &self.work_user_content_manager,
            &self.hardened_user_content_manager,
        ] {
            ucm.register_script_message_handler("takeTranslate", None);
            let cb = callback.clone();
            ucm.connect_script_message_received(Some("takeTranslate"), move |_, val| {
                let raw = val.to_str();
                let json_str = raw.as_str();
                if json_str.len() <= 262_144 {
                    cb(json_str);
                }
            });
        }
    }

    /// Registers the `takeTranslatorHost` IPC handler for the internal Bergamot WASM host WebView.
    pub fn setup_translator_host_listener<F: Fn(&str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.translator_host_callback.borrow_mut() = Some(callback);
    }

    /// Registers the `takeReaderMode` IPC handler for Reader Mode status updates.
    pub fn setup_reader_mode_listener<F: Fn(bool, &str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.reader_mode_callback.borrow_mut() = Some(callback.clone());
        for ucm in [
            &self.work_user_content_manager,
            &self.hardened_user_content_manager,
        ] {
            ucm.register_script_message_handler("takeReaderMode", None);
            let cb = callback.clone();
            ucm.connect_script_message_received(Some("takeReaderMode"), move |_, val| {
                let raw = val.to_str();
                let json_str = raw.as_str();
                if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let active = obj.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
                    let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    cb(active, url);
                }
            });
        }
    }

    /// Registers the `takeAction` IPC handler for internal pages (e.g. adding a favorite from start page).
    pub fn setup_action_listener<F: Fn(&str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.action_callback.borrow_mut() = Some(callback.clone());
        for ucm in [
            &self.work_user_content_manager,
            &self.hardened_user_content_manager,
        ] {
            ucm.register_script_message_handler("takeAction", None);
            let cb = callback.clone();
            ucm.connect_script_message_received(Some("takeAction"), move |_, val| {
                let raw = val.to_str();
                cb(raw.as_str());
            });
        }
    }

    /// Registers the `takeMedia` IPC handler for WebKit audio/video playback and mediaSession metadata.
    pub fn setup_media_listener<F: Fn(&str, &str, &str, &str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.media_callback.borrow_mut() = Some(callback.clone());
        for ucm in [
            &self.work_user_content_manager,
            &self.hardened_user_content_manager,
        ] {
            ucm.register_script_message_handler("takeMedia", None);
            let cb = callback.clone();
            ucm.connect_script_message_received(Some("takeMedia"), move |_, val| {
                let raw = val.to_str();
                let json_str = raw.as_str();
                if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let event = obj.get("event").and_then(|v| v.as_str()).unwrap_or("");
                    let title = obj.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let artist = obj.get("artist").and_then(|v| v.as_str()).unwrap_or("");
                    let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    cb(event, title, artist, url);
                }
            });
        }
    }

    /// Creates a dedicated internal WebView running `take://translator/index.html`
    /// so Bergamot WASM executes in an isolated first-party context immune to website CSP.
    pub fn create_translator_view(&self) -> WebView {
        let ucm = UserContentManager::new();
        ucm.register_script_message_handler("takeTranslatorHost", None);
        let host_cb_slot = self.translator_host_callback.clone();
        ucm.connect_script_message_received(Some("takeTranslatorHost"), move |_, val| {
            let raw = val.to_str();
            let json_str = raw.as_str();
            if json_str.len() <= 262_144 {
                if let Some(ref cb) = *host_cb_slot.borrow() {
                    cb(json_str);
                }
            }
        });

        let view = WebView::builder()
            .web_context(&self.web_context)
            .network_session(self.network_manager.session())
            .settings(&self.internal_settings)
            .user_content_manager(&ucm)
            .build();
        view.load_uri("take://translator/index.html");
        view
    }

    /// Test-suite only: forwards `console.log` and test results to stdout.
    pub fn setup_console_interceptor(ucm: &UserContentManager) {
        ucm.register_script_message_handler("consoleMessage", None);
        ucm.register_script_message_handler("testRunner", None);

        // Inject script to forward console.log into the Rust WebKit bridge
        let script_source = r#"
        (function() {
            if (window.__take_console_hooked__) return;
            window.__take_console_hooked__ = true;
            const origLog = console.log;
            console.log = function(...args) {
                origLog.apply(console, args);
                try {
                    const msg = args.map(a => (typeof a === 'object' && a !== null) ? JSON.stringify(a) : String(a)).join(' ');
                    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.consoleMessage) {
                        window.webkit.messageHandlers.consoleMessage.postMessage(msg);
                    }
                } catch(e) {}
            };
        })();
        "#;

        let user_script = UserScript::new(
            script_source,
            UserContentInjectedFrames::AllFrames,
            UserScriptInjectionTime::Start,
            &[],
            &[],
        );
        ucm.add_script(&user_script);

        fn process_console_message(value: &webkit6::javascriptcore::Value) {
            let raw = value.to_str();
            let text = raw.as_str();
            if let Some(json_payload) = text.strip_prefix("TEST_RESULT: ") {
                log::info!("TEST_RESULT: {}", json_payload);
                println!("TEST_RESULT: {}", json_payload);
            }
        }

        ucm.connect_script_message_received(Some("consoleMessage"), move |_, val| {
            process_console_message(val);
        });

        ucm.connect_script_message_received(Some("testRunner"), move |_, val| {
            process_console_message(val);
        });
    }

    /// Sets a callback to be invoked whenever a new ephemeral NetworkSession is created.
    pub fn set_on_ephemeral_session_created<F: Fn(&webkit6::NetworkSession) + 'static>(
        &self,
        f: F,
    ) {
        *self.ephemeral_session_callback.borrow_mut() = Some(Rc::new(f));
    }

    /// Returns the shared ephemeral NetworkManager for private browsing, creating one if needed.
    pub fn get_or_create_ephemeral_network(&self) -> NetworkManager {
        let mut slot = self.ephemeral_network_manager.borrow_mut();
        if let Some(ref nm) = *slot {
            nm.clone()
        } else {
            let nm = NetworkManager::new_ephemeral();
            if let Some(ref cb) = *self.ephemeral_session_callback.borrow() {
                cb(nm.session());
            }
            *slot = Some(nm.clone());
            nm
        }
    }

    /// Wipes the ephemeral NetworkSession, purging in-RAM cookies, storage, and cache.
    pub fn wipe_ephemeral_network(&self) {
        if let Some(ref nm) = *self.ephemeral_network_manager.borrow() {
            if let Some(dm) = nm.session().website_data_manager() {
                dm.clear(
                    webkit6::WebsiteDataTypes::ALL,
                    glib::TimeSpan::from_seconds(0),
                    None::<&gio::Cancellable>,
                    |res| {
                        if let Err(err) = res {
                            log::warn!("Failed to clear ephemeral website data: {err}");
                        } else {
                            log::info!("Ephemeral website data purged from RAM");
                        }
                    },
                );
            }
        }
        *self.ephemeral_network_manager.borrow_mut() = None;
    }

    pub fn create_web_view(&self) -> WebView {
        self.create_web_view_related_to(None)
    }

    /// A related view shares the opener's web process, which window.open() requires
    /// for the opened page to keep `window.opener`.
    pub fn create_web_view_related_to(&self, related: Option<&WebView>) -> WebView {
        self.create_web_view_with_session(self.network_manager.session(), related)
    }

    /// Creates an isolated private browsing WebView bound to the ephemeral RAM-only session.
    pub fn create_private_web_view(&self) -> WebView {
        self.create_private_web_view_related_to(None)
    }

    /// Creates a related private browsing WebView for window.open() in private mode.
    pub fn create_private_web_view_related_to(&self, related: Option<&WebView>) -> WebView {
        let ephemeral_nm = self.get_or_create_ephemeral_network();
        self.create_web_view_with_session(ephemeral_nm.session(), related)
    }

    fn create_web_view_with_session(
        &self,
        session: &webkit6::NetworkSession,
        related: Option<&WebView>,
    ) -> WebView {
        let active_profile = *self.profile.borrow();
        let ucm = match active_profile {
            BrowserProfile::Work => &self.work_user_content_manager,
            BrowserProfile::Hardened => &self.hardened_user_content_manager,
        };

        let builder = WebView::builder()
            .web_context(&self.web_context)
            .network_session(session)
            .settings(&self.internal_settings)
            .user_content_manager(ucm);

        let view = match related {
            Some(opener) => builder.related_view(opener).build(),
            None => builder.build(),
        };

        // Network-level header sanitization for Hardened profile and WebKit Content Filter failure tracking
        let profile_ref = self.profile.clone();
        let tracker_cb_slot = self.tracker_callback.clone();
        let is_ephemeral = session.is_ephemeral();
        view.connect_resource_load_started(move |_view, resource, request| {
            if profile_ref.borrow().enables_anti_fingerprinting() {
                NetworkManager::sanitize_request_headers(request);
            }

            if profile_ref.borrow().enables_content_blocker() && !is_ephemeral {
                if let Some(uri_gstr) = request.uri() {
                    let uri_str = uri_gstr.to_string();
                    let cb_slot = tracker_cb_slot.clone();
                    if let Some((domain, rule)) =
                        crate::privacy::tracker_stats::classify_tracker(&uri_str)
                    {
                        if let Some(cb) = cb_slot.borrow().as_ref() {
                            cb(&domain, &rule);
                        }
                    } else {
                        resource.connect_failed(move |_res, error| {
                            let err_msg = error.message();
                            let err_lower = err_msg.to_lowercase();
                            if err_lower.contains("cancelled") || err_lower.contains("blocked") {
                                if let Ok(parsed) = url::Url::parse(&uri_str) {
                                    if let Some(host) = parsed.host_str() {
                                        if host.contains("track")
                                            || host.contains("analytics")
                                            || host.contains("telemetry")
                                            || host.contains("pixel")
                                            || host.contains("adservice")
                                            || host.contains("adsystem")
                                        {
                                            if let Some(cb) = cb_slot.borrow().as_ref() {
                                                cb(host, "Content Filter");
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            }
        });

        view
    }

    pub fn network_manager(&self) -> &NetworkManager {
        &self.network_manager
    }

    pub fn current_profile(&self) -> BrowserProfile {
        *self.profile.borrow()
    }

    pub fn content_filter(&self) -> Rc<RefCell<Option<UserContentFilter>>> {
        Rc::clone(&self.content_filter)
    }

    /// Checks if a URI represents an internal browser interface (take:// or about:).
    pub fn is_internal_uri(uri: &str) -> bool {
        let logical = Self::logical_url(uri);
        logical.starts_with("take://") || uri.starts_with("take://") || uri.starts_with("about:")
    }

    /// Dynamically binds either internal_settings (where JS is always enabled for browser UI)
    /// or web_settings (where user preferences like JS disabling apply) to the WebView.
    pub fn apply_settings_for_uri(&self, view: &WebView, uri: &str) {
        if Self::is_internal_uri(uri) {
            view.set_settings(&self.internal_settings);
        } else {
            view.set_settings(&self.web_settings);
        }
    }

    pub fn set_hardware_acceleration(&self, enabled: bool) {
        let policy = if enabled {
            HardwareAccelerationPolicy::Always
        } else {
            HardwareAccelerationPolicy::Never
        };
        self.web_settings.set_hardware_acceleration_policy(policy);
        self.web_settings.set_enable_2d_canvas_acceleration(enabled);
        self.web_settings.set_enable_webgl(enabled);

        self.internal_settings
            .set_hardware_acceleration_policy(policy);
        self.internal_settings
            .set_enable_2d_canvas_acceleration(enabled);
        self.internal_settings.set_enable_webgl(enabled);
        log::info!("Hardware acceleration set to: {enabled}");
    }

    pub fn set_smooth_scrolling(&self, enabled: bool) {
        self.web_settings.set_enable_smooth_scrolling(enabled);
        self.internal_settings.set_enable_smooth_scrolling(enabled);
        log::info!("Smooth scrolling set to: {enabled}");
    }

    pub fn set_javascript_enabled(&self, enabled: bool) {
        self.web_settings.set_enable_javascript(enabled);
        // Internal settings must ALWAYS keep JavaScript enabled for settings, start page, etc.
        self.internal_settings.set_enable_javascript(true);
        log::info!(
            "JavaScript execution for web set to: {enabled} (internal pages always enabled)"
        );
    }

    pub fn set_block_autoplay(&self, block: bool) {
        self.web_settings
            .set_media_playback_requires_user_gesture(block);
        log::info!("Block media autoplay set to: {block}");
    }

    pub fn set_webrtc_protection(&self, protect: bool) {
        self.web_settings.set_enable_webrtc(!protect);
        log::info!("WebRTC protection set to: {protect}");
    }

    pub fn set_adblock_enabled(&self, enabled: bool) {
        if let Some(filter) = self.content_filter.borrow().as_ref() {
            if enabled {
                self.work_user_content_manager.add_filter(filter);
                self.hardened_user_content_manager.add_filter(filter);
            } else {
                self.work_user_content_manager.remove_filter(filter);
                self.hardened_user_content_manager.remove_filter(filter);
            }
            log::info!("Adblock content filter set to: {enabled}");
        }
    }

    pub fn set_anti_fingerprint(&self, enabled: bool) {
        if enabled {
            self.web_settings.set_user_agent(Some(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15",
            ));
            self.web_settings.set_enable_mock_capture_devices(false);
        } else {
            self.web_settings.set_user_agent(None);
            self.web_settings.set_enable_mock_capture_devices(false);
        }
        log::info!("Anti-fingerprinting mode set to: {enabled}");
    }

    pub fn set_https_only(&self, enabled: bool) {
        self.network_manager.set_https_only(enabled);
    }

    pub fn resolve_url(input: &str) -> String {
        Self::resolve_url_with_engine(input, "duckduckgo")
    }

    pub fn resolve_url_with_engine(input: &str, engine: &str) -> String {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return "take://start".to_string();
        }

        if trimmed == "about:blank" {
            return trimmed.to_string();
        }

        if trimmed.starts_with("take://") && scheme_handler::canonical_load_url(trimmed).is_some() {
            // Fold aliases such as privacy -> trackers
            return trimmed.replacen("take://privacy", "take://trackers", 1);
        }

        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("file://")
        {
            let candidate = if let Some(upgraded) = NetworkManager::enforce_https_upgrade(trimmed) {
                upgraded
            } else {
                trimmed.to_string()
            };
            return crate::privacy::url_cleaner::UrlCleaner::strip_tracking(&candidate);
        }

        // Domain check (e.g. "github.com", "archlinux.org", "localhost:3000")
        if (trimmed.contains('.') && !trimmed.contains(' ')) || trimmed.starts_with("localhost") {
            let candidate = format!("https://{}", trimmed);
            return crate::privacy::url_cleaner::UrlCleaner::strip_tracking(&candidate);
        }

        let encoded = urlencoding::encode(trimmed);
        if engine.contains("%s") {
            return engine.replace("%s", &encoded);
        }
        if engine.starts_with("http://") || engine.starts_with("https://") {
            if engine.ends_with('=') || engine.ends_with('?') || engine.ends_with('/') {
                return format!("{}{}", engine, encoded);
            } else {
                return format!("{}?q={}", engine, encoded);
            }
        }

        let lang = crate::i18n::current_language();
        let (google_hl, startpage_lang, bing_lang, ddg_kl) = match lang {
            crate::i18n::UiLanguage::Ru => ("ru", "russian", "ru", "ru-ru"),
            crate::i18n::UiLanguage::En => ("en", "english", "en", "us-en"),
        };

        match engine.to_lowercase().as_str() {
            "google" => format!(
                "https://www.google.com/search?q={}&hl={}",
                encoded, google_hl
            ),
            "startpage" => format!(
                "https://www.startpage.com/do/dsearch?q={}&cat=web&language={}",
                encoded, startpage_lang
            ),
            "yandex" => format!("https://yandex.ru/search/?text={}", encoded),
            "bing" => format!(
                "https://www.bing.com/search?q={}&setlang={}",
                encoded, bing_lang
            ),
            "brave" => format!("https://search.brave.com/search?q={}", encoded),
            "ecosia" => format!("https://www.ecosia.org/search?q={}", encoded),
            "qwant" => format!("https://www.qwant.com/?q={}", encoded),
            _ => format!("https://duckduckgo.com/?q={}&kl={}", encoded, ddg_kl),
        }
    }

    /// Maps the real location of bundled pages back to their take:// address.
    pub fn logical_url(uri: &str) -> String {
        if let Some(logical) = scheme_handler::logical_from_uri(uri) {
            return if logical == "take://privacy" {
                "take://trackers".to_string()
            } else {
                logical
            };
        }
        uri.to_string()
    }
}

// Lightweight URL encoder to avoid extra dependencies
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for b in s.bytes() {
            match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(b as char);
                }
                b' ' => result.push('+'),
                _ => {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_resolution() {
        assert_eq!(BrowserEngine::resolve_url(""), "take://start");
        assert_eq!(
            BrowserEngine::resolve_url("take://settings"),
            "take://settings"
        );
        assert_eq!(
            BrowserEngine::resolve_url("take://trackers"),
            "take://trackers"
        );
        assert_eq!(
            BrowserEngine::resolve_url("take://privacy"),
            "take://trackers"
        );
        assert_eq!(
            BrowserEngine::resolve_url("https://example.com"),
            "https://example.com"
        );
        assert_eq!(
            BrowserEngine::resolve_url("http://example.com"),
            "https://example.com"
        );
        assert_eq!(
            BrowserEngine::resolve_url("http://localhost:3000"),
            "http://localhost:3000"
        );
        assert_eq!(
            BrowserEngine::resolve_url("example.com"),
            "https://example.com"
        );
        assert_eq!(
            BrowserEngine::resolve_url(
                "https://example.com/search?q=rust&utm_source=twitter&utm_medium=social"
            ),
            "https://example.com/search?q=rust"
        );
        assert_eq!(
            BrowserEngine::resolve_url("example.com/item?id=42&fbclid=IwAR123&yclid=987"),
            "https://example.com/item?id=42"
        );
        assert_eq!(
            BrowserEngine::resolve_url("arch linux rust"),
            "https://duckduckgo.com/?q=arch+linux+rust&kl=us-en"
        );
    }

    #[test]
    fn test_url_resolution_with_various_engines() {
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "startpage"),
            "https://www.startpage.com/do/dsearch?q=arch+linux&cat=web&language=english"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "google"),
            "https://www.google.com/search?q=arch+linux&hl=en"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "yandex"),
            "https://yandex.ru/search/?text=arch+linux"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "bing"),
            "https://www.bing.com/search?q=arch+linux&setlang=en"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "brave"),
            "https://search.brave.com/search?q=arch+linux"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "ecosia"),
            "https://www.ecosia.org/search?q=arch+linux"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "qwant"),
            "https://www.qwant.com/?q=arch+linux"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "duckduckgo"),
            "https://duckduckgo.com/?q=arch+linux&kl=us-en"
        );
        assert_eq!(
            BrowserEngine::resolve_url_with_engine("arch linux", "https://searx.be/search?q=%s"),
            "https://searx.be/search?q=arch+linux"
        );
    }

    #[test]
    fn test_logical_url_maps_take_scheme_and_never_spoofs_from_files() {
        assert_eq!(BrowserEngine::logical_url("take://start/"), "take://start");
        assert_eq!(
            BrowserEngine::logical_url("take://settings/#about"),
            "take://settings"
        );
        assert_eq!(
            BrowserEngine::logical_url("take://privacy/"),
            "take://trackers"
        );
        assert_eq!(
            BrowserEngine::logical_url("take://test-suite/"),
            "take://test-suite"
        );
        assert_eq!(
            BrowserEngine::logical_url("https://example.com/"),
            "https://example.com/"
        );
        // A local file must never be displayed as an internal page.
        assert_eq!(
            BrowserEngine::logical_url("file:///tmp/settings/index.html"),
            "file:///tmp/settings/index.html"
        );
    }

    #[test]
    fn test_is_internal_uri_and_settings_isolation() {
        assert!(BrowserEngine::is_internal_uri("take://start"));
        assert!(BrowserEngine::is_internal_uri("take://start/"));
        assert!(BrowserEngine::is_internal_uri("take://settings"));
        assert!(BrowserEngine::is_internal_uri("take://settings/#about"));
        assert!(BrowserEngine::is_internal_uri("take://trackers"));
        assert!(BrowserEngine::is_internal_uri("about:blank"));

        assert!(!BrowserEngine::is_internal_uri("https://example.com"));
        assert!(!BrowserEngine::is_internal_uri("http://localhost:3000"));
        assert!(!BrowserEngine::is_internal_uri(
            "https://google.com/search?q=test"
        ));
    }

    #[test]
    fn test_ipc_token_generation() {
        let token = glib::uuid_string_random().to_string();
        assert_eq!(token.len(), 36);
        assert_eq!(token.matches('-').count(), 4);
    }
}
