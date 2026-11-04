use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use glib::clone;
use gtk4::gio::ThemedIcon;
use gtk4::prelude::*;
use libadwaita as adw;
use webkit6::prelude::*;
use webkit6::{WebProcessTerminationReason, WebView};

use crate::browser::BrowserEngine;

static TAB_COUNTER: AtomicU64 = AtomicU64::new(1);

const START_URL: &str = "take://start";
const SETTINGS_URL: &str = "take://settings";
const TRACKERS_URL: &str = "take://trackers";
const PRIVACY_URL: &str = "take://privacy";

pub struct BrowserTab {
    pub id: u64,
    pub web_view: WebView,
    pub page: adw::TabPage,
    /// WebKit reports no URI until the provisional load starts, so remember what was asked for.
    requested_url: RefCell<String>,
    pub is_crashed: Cell<Option<WebProcessTerminationReason>>,
    pub is_private: bool,
    pub detected_lang: RefCell<Option<String>>,
    pub auto_translated_for_uri: RefCell<Option<String>>,
    pub blocked_trackers: Rc<RefCell<Vec<String>>>,
}

impl BrowserTab {
    /// Adds a tab at the end of the strip (or right after `parent`, which also makes
    /// the parent the page selected when this one is closed).
    pub fn new(
        engine: &BrowserEngine,
        tab_view: &adw::TabView,
        parent: Option<&adw::TabPage>,
    ) -> Self {
        Self::attach(engine.create_web_view(), tab_view, parent, false)
    }

    /// Creates an isolated private browsing tab bound to the ephemeral RAM session.
    pub fn new_private(
        engine: &BrowserEngine,
        tab_view: &adw::TabView,
        parent: Option<&adw::TabPage>,
    ) -> Self {
        Self::attach(engine.create_private_web_view(), tab_view, parent, true)
    }

    /// Wraps a view that WebKit asked us to create for `window.open()` or `target=_blank`.
    pub fn from_web_view(
        web_view: WebView,
        tab_view: &adw::TabView,
        parent: Option<&adw::TabPage>,
    ) -> Self {
        Self::attach(web_view, tab_view, parent, false)
    }

    /// Wraps a private view that WebKit asked us to create for `window.open()`.
    pub fn from_private_web_view(
        web_view: WebView,
        tab_view: &adw::TabView,
        parent: Option<&adw::TabPage>,
    ) -> Self {
        Self::attach(web_view, tab_view, parent, true)
    }

    /// Wraps an existing page and web view when transferred between windows (drag & drop / tear-off).
    pub fn from_transferred(web_view: WebView, page: adw::TabPage, is_private: bool) -> Self {
        let id = TAB_COUNTER.fetch_add(1, Ordering::SeqCst);
        page.set_live_thumbnail(false);
        Self {
            id,
            web_view,
            page,
            requested_url: RefCell::new(String::new()),
            is_crashed: Cell::new(None),
            is_private,
            detected_lang: RefCell::new(None),
            auto_translated_for_uri: RefCell::new(None),
            blocked_trackers: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn attach(
        web_view: WebView,
        tab_view: &adw::TabView,
        parent: Option<&adw::TabPage>,
        is_private: bool,
    ) -> Self {
        let id = TAB_COUNTER.fetch_add(1, Ordering::SeqCst);

        web_view.add_css_class("take-web-view");
        if is_private {
            web_view.add_css_class("take-private-web-view");
        }
        web_view.set_background_color(&gtk4::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0));
        web_view.set_hexpand(true);
        web_view.set_vexpand(true);

        let page = tab_view.add_page(&web_view, parent);
        if is_private {
            page.set_title(crate::i18n::tr("tab.private_tab_title"));
            page.set_icon(Some(&ThemedIcon::new("view-conceal-symbolic")));
        } else {
            page.set_title(crate::i18n::tr("tab.new_tab_title"));
            page.set_icon(Some(&ThemedIcon::new("web-browser-symbolic")));
        }
        page.set_live_thumbnail(false);

        web_view.connect_title_notify(clone!(
            #[weak]
            page,
            move |view| Self::sync_title(&page, view)
        ));

        web_view.connect_is_loading_notify(clone!(
            #[weak]
            page,
            move |view| {
                let loading = if Self::is_start_page(view) || Self::is_settings_page(view) || Self::is_trackers_page(view) {
                    false
                } else {
                    view.is_loading()
                };
                page.set_loading(loading);
                Self::sync_icon(&page, view, is_private);
                if !loading {
                    page.invalidate_thumbnail();
                    let is_dark = libadwaita::StyleManager::default().is_dark();
                    let theme_str = if is_dark { "dark" } else { "light" };
                    let js = if is_private {
                        format!("if (document.documentElement) {{ document.documentElement.setAttribute('data-theme', '{theme_str}'); document.documentElement.setAttribute('data-private', 'true'); if (document.body) document.body.classList.add('private-session'); }}")
                    } else {
                        format!("if (document.documentElement) document.documentElement.setAttribute('data-theme', '{theme_str}');")
                    };
                    view.evaluate_javascript(&js, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
                }
            }
        ));

        web_view.connect_favicon_notify(clone!(
            #[weak]
            page,
            move |view| Self::sync_icon(&page, view, is_private)
        ));

        web_view.connect_uri_notify(clone!(
            #[weak]
            page,
            move |view| {
                Self::sync_title(&page, view);
                Self::sync_icon(&page, view, is_private);
            }
        ));

        web_view.connect_is_playing_audio_notify(clone!(
            #[weak]
            page,
            move |view| Self::sync_audio_indicator(&page, view)
        ));

        web_view.connect_is_muted_notify(clone!(
            #[weak]
            page,
            move |view| Self::sync_audio_indicator(&page, view)
        ));

        web_view.connect_load_failed(|view, _event, failing_uri, error| {
            let domain = error.domain().as_str();
            let code = error.code();
            // Don't show error page for user cancellations or download handoffs
            if (domain == "WebKitNetworkError" && code == 302)
                || (domain == "g-io-error-quark" && code == 19)
                || (domain == "WebKitPolicyError" && code == 102)
                || (domain == "WebKitPluginError" && code == 204)
                || error.message().to_lowercase().contains("cancelled")
            {
                return false;
            }

            log::warn!("Load failed for '{failing_uri}': {error} ({domain}:{code})");
            let html = crate::browser::error_page::render_error_page(failing_uri, error);
            view.load_alternate_html(&html, failing_uri, None);
            true
        });

        Self {
            id,
            web_view,
            page,
            requested_url: RefCell::new(String::new()),
            is_crashed: Cell::new(None),
            is_private,
            detected_lang: RefCell::new(None),
            auto_translated_for_uri: RefCell::new(None),
            blocked_trackers: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn mark_crashed(&self, reason: WebProcessTerminationReason) {
        self.is_crashed.set(Some(reason));
    }

    pub fn clear_crashed(&self) {
        self.is_crashed.set(None);
    }

    pub fn crashed_reason(&self) -> Option<WebProcessTerminationReason> {
        self.is_crashed.get()
    }

    fn is_start_page(view: &WebView) -> bool {
        view.uri()
            .map(|uri| BrowserEngine::logical_url(&uri) == START_URL)
            .unwrap_or(false)
    }

    fn is_settings_page(view: &WebView) -> bool {
        view.uri()
            .map(|uri| BrowserEngine::logical_url(&uri) == SETTINGS_URL)
            .unwrap_or(false)
    }

    fn is_trackers_page(view: &WebView) -> bool {
        view.uri()
            .map(|uri| {
                let logical = BrowserEngine::logical_url(&uri);
                logical == TRACKERS_URL || logical == PRIVACY_URL
            })
            .unwrap_or(false)
    }

    fn sync_title(page: &adw::TabPage, view: &WebView) {
        if Self::is_start_page(view) {
            page.set_title(crate::i18n::tr("tab.startpage_title"));
            return;
        }
        if Self::is_settings_page(view) {
            page.set_title(crate::i18n::tr("tab.settings_title"));
            return;
        }
        if Self::is_trackers_page(view) {
            page.set_title(crate::i18n::tr("tab.trackers_title"));
            return;
        }
        if let Some(title) = view.title() {
            if !title.is_empty() {
                page.set_title(&title);
            }
        }
    }

    fn sync_icon(page: &adw::TabPage, view: &WebView, is_private: bool) {
        if let Some(texture) = view.favicon() {
            page.set_icon(Some(&texture));
            if !is_private {
                if let Some(uri) = view.uri() {
                    let domain = crate::browser::favicon::FaviconManager::extract_domain(&uri);
                    crate::browser::favicon::FaviconManager::global()
                        .save_texture(&domain, &texture);
                }
            }
        } else if Self::is_start_page(view) {
            page.set_icon(Some(&ThemedIcon::new("go-home-symbolic")));
        } else if Self::is_settings_page(view) {
            page.set_icon(Some(&ThemedIcon::new("preferences-system-symbolic")));
        } else if Self::is_trackers_page(view) {
            page.set_icon(Some(&ThemedIcon::new("security-high-symbolic")));
        } else {
            page.set_icon(Some(&ThemedIcon::new("web-browser-symbolic")));
        }
    }

    fn sync_audio_indicator(page: &adw::TabPage, view: &WebView) {
        // Internal take:// pages never produce sound
        if Self::is_start_page(view) || Self::is_settings_page(view) || Self::is_trackers_page(view)
        {
            page.set_indicator_icon(None::<&ThemedIcon>);
            page.set_indicator_activatable(false);
            return;
        }

        let is_playing = view.is_playing_audio();
        let is_muted = view.is_muted();

        let icon_name = if is_playing {
            if is_muted {
                Some("audio-volume-muted-symbolic")
            } else {
                Some("audio-volume-high-symbolic")
            }
        } else {
            None
        };

        match icon_name {
            Some(name) => {
                page.set_indicator_icon(Some(&ThemedIcon::new(name)));
                page.set_indicator_activatable(true);
            }
            None => {
                page.set_indicator_icon(None::<&ThemedIcon>);
                page.set_indicator_activatable(false);
            }
        }
    }

    pub fn toggle_mute(&self) {
        self.web_view.set_is_muted(!self.web_view.is_muted());
    }

    /// The address as the user knows it: bundled pages appear as take:// URLs.
    pub fn logical_url(&self) -> String {
        match self.web_view.uri() {
            Some(uri) => BrowserEngine::logical_url(&uri),
            None => self.requested_url.borrow().clone(),
        }
    }

    pub fn load_url(&self, target_url: &str) {
        *self.requested_url.borrow_mut() = target_url.to_string();

        // Internal pages are served by the in-memory `take://` scheme handler.
        let internal = crate::browser::scheme_handler::canonical_load_url(target_url);
        if internal.is_some() {
            let logical = BrowserEngine::logical_url(target_url);
            match logical.as_str() {
                START_URL => {
                    self.page.set_title(crate::i18n::tr("tab.startpage_title"));
                    self.page
                        .set_icon(Some(&ThemedIcon::new("go-home-symbolic")));
                    self.page.set_loading(false);
                }
                SETTINGS_URL => {
                    self.page.set_title(crate::i18n::tr("tab.settings_title"));
                    self.page
                        .set_icon(Some(&ThemedIcon::new("preferences-system-symbolic")));
                    self.page.set_loading(false);
                }
                TRACKERS_URL | PRIVACY_URL => {
                    self.page.set_title(crate::i18n::tr("tab.trackers_title"));
                    self.page
                        .set_icon(Some(&ThemedIcon::new("security-high-symbolic")));
                    self.page.set_loading(false);
                }
                _ => {}
            }
        }

        match internal {
            Some(mut url) => {
                if self.is_private && (url == "take://start/" || url == "take://start") {
                    url = "take://start/?private=1".to_string();
                }
                self.web_view.load_uri(&url);
            }
            None => {
                let effective_url =
                    crate::browser::NetworkManager::enforce_https_upgrade(target_url)
                        .unwrap_or_else(|| target_url.to_string());
                self.web_view.load_uri(&effective_url);
            }
        }
    }
}
