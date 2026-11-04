use std::rc::Rc;

use gtk4::gio::{self, ActionEntry};
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Entry, Orientation};
use libadwaita as adw;
use libadwaita::prelude::*;
use webkit6::prelude::*;
use webkit6::{PrintOperation, SaveMode};

use super::{
    host_of, is_web_url, set_sidebar_visible, BrowserWindow, Inner, PREVIEW_SCRIPT, PRIVACY_URL,
    SETTINGS_URL, START_URL, TRACKERS_URL,
};
use crate::browser::session_manager::SessionManager;
use crate::storage::database::FAVORITES_FOLDER;
use crate::storage::{page_data, BrowserDatabase};
use crate::ui::app_menu::AppMenuAction;
use crate::ui::sidebar::{ActionHandler, SidebarAction};
use crate::ui::tab::BrowserTab;

impl Inner {
    pub(super) fn handle_app_menu_action(self: &Rc<Self>, action: AppMenuAction) {
        match action {
            AppMenuAction::NewTab => {
                self.open_user_tab();
            }
            AppMenuAction::NewWindow => {
                log::info!("AppMenu: NewWindow requested");
                if let Some(app) = self.window.application().and_downcast::<adw::Application>() {
                    let win = BrowserWindow::new(
                        &app,
                        self.is_test_suite,
                        &[],
                        self.is_private_mode.get(),
                    );
                    win.present();
                }
            }
            AppMenuAction::RestoreSession => {
                let to_restore = self.last_crashed_session.borrow().clone().or_else(|| {
                    self.session_path
                        .as_ref()
                        .and_then(|p| SessionManager::load_session(p).ok().flatten())
                });

                if let Some(session) = to_restore {
                    if !session.tabs.is_empty() {
                        self.restore_session(&session);
                        self.toast(crate::i18n::tr("app.session_restored"));
                        self.restore_banner.set_revealed(false);
                        self.restore_banner.set_visible(false);
                    } else {
                        self.toast(crate::i18n::tr("app.session_empty"));
                    }
                } else {
                    self.toast(crate::i18n::tr("app.session_not_found"));
                }
            }
            AppMenuAction::OpenBookmarks => {
                set_sidebar_visible(&self.split_view, true);
                self.toast(crate::i18n::tr("sidebar.bookmarks_opened"));
            }
            AppMenuAction::OpenHistory => {
                let h_opt = self.history_popover.borrow().clone();
                glib::idle_add_local_once(move || {
                    if let Some(popover) = h_opt {
                        popover.popup();
                    }
                });
            }
            AppMenuAction::OpenDownloads => {
                self.header.downloads_button.set_visible(true);
                let dl = self.downloads_popover.clone();
                glib::idle_add_local_once(move || {
                    dl.popup();
                });
            }
            AppMenuAction::OpenReadingList => {
                set_sidebar_visible(&self.split_view, true);
                self.toast(crate::i18n::tr("sidebar.reading_list_opened"));
            }
            AppMenuAction::ZoomIn => {
                self.zoom_in();
            }
            AppMenuAction::ZoomOut => {
                self.zoom_out();
            }
            AppMenuAction::ZoomReset => {
                self.zoom_reset();
            }
            AppMenuAction::Print => {
                self.print_current_tab();
            }
            AppMenuAction::ToggleFullscreen => {
                self.toggle_fullscreen();
            }
            AppMenuAction::FindInPage => {
                if let Some(tab) = self.current_tab() {
                    self.find_bar.show_for_view(&tab.web_view);
                }
            }
            AppMenuAction::TranslatePage => {
                if let Some(tab) = self.current_tab() {
                    let detected = tab.detected_lang.borrow().clone();
                    self.translate_popover
                        .set_detected_language(detected.as_deref());
                }
                self.translate_popover.popup();
            }
            AppMenuAction::ToggleReaderMode => {
                self.toggle_reader_mode();
            }
            AppMenuAction::ToggleSplitView => {
                self.toggle_split_view();
            }
            AppMenuAction::InstallPwa => {
                self.install_current_page_as_pwa();
            }
            AppMenuAction::ToggleTheme => {
                self.toggle_theme();
            }
            AppMenuAction::OpenSettings => {
                self.open_settings_page();
            }
            AppMenuAction::OpenPrivacyReport => {
                self.open_tab(TRACKERS_URL, None, true);
            }
            AppMenuAction::OpenAbout => {
                self.open_tab(&format!("{SETTINGS_URL}#about"), None, true);
            }
        }
    }

    pub(super) fn print_current_tab(self: &Rc<Self>) {
        if let Some(tab) = self.current_tab() {
            let print_op = PrintOperation::new(&tab.web_view);
            let weak = Rc::downgrade(self);
            print_op.connect_failed(move |_, err| {
                if let Some(this) = weak.upgrade() {
                    log::warn!("Print failed: {}", err.message());
                    this.toast(crate::i18n::tr("app.print_failed"));
                }
            });
            print_op.run_dialog(Some(&self.window));
        }
    }

    pub(super) fn enter_fullscreen(&self) {
        self.fullscreen_prev_sidebar
            .set(self.split_view.shows_sidebar());
        self.toolbar_view.set_reveal_top_bars(false);
        self.header.container.set_visible(false);
        self.tab_bar.set_visible(false);
        set_sidebar_visible(&self.split_view, false);
        self.window.fullscreen();
    }

    pub(super) fn leave_fullscreen(&self) {
        self.window.unfullscreen();
        self.header.container.set_visible(true);
        self.tab_bar.set_visible(true);
        self.toolbar_view.set_reveal_top_bars(true);
        set_sidebar_visible(&self.split_view, self.fullscreen_prev_sidebar.get());
    }

    pub(super) fn toggle_fullscreen(&self) {
        if self.window.is_fullscreen() {
            self.leave_fullscreen();
        } else {
            self.enter_fullscreen();
        }
    }

    /// Last theme applied to the UI (no database or StyleManager round-trip).
    pub(super) fn is_dark(&self) -> bool {
        self.theme_dark.get()
    }

    pub(super) fn apply_theme_setting(&self, theme: &str) {
        let style_mgr = adw::StyleManager::default();
        let is_dark = match theme {
            "light" => {
                style_mgr.set_color_scheme(adw::ColorScheme::ForceLight);
                false
            }
            "dark" => {
                style_mgr.set_color_scheme(adw::ColorScheme::ForceDark);
                true
            }
            _ => {
                style_mgr.set_color_scheme(adw::ColorScheme::Default);
                style_mgr.is_dark()
            }
        };
        let _ = self.db.set_setting("theme", theme);
        self.sync_theme_classes(is_dark);
    }

    pub(super) fn sync_theme_classes(&self, is_dark: bool) {
        self.theme_dark.set(is_dark);
        if is_dark {
            self.window.remove_css_class("light-mode");
            self.window.add_css_class("dark-mode");
            self.split_view.remove_css_class("light-mode");
            self.split_view.add_css_class("dark-mode");
            self.sidebar.container.remove_css_class("light-mode");
            self.sidebar.container.add_css_class("dark-mode");
            self.downloads_popover
                .popover()
                .remove_css_class("light-mode");
            if let Some(h) = self.history_popover.borrow().as_ref() {
                h.popover().remove_css_class("light-mode");
            }
            if let Some(m) = self.app_menu.borrow().as_ref() {
                m.popover.remove_css_class("light-mode");
            }
            self.omnibox_popup.popover().remove_css_class("light-mode");
            self.shield_popover.popover().remove_css_class("light-mode");
            self.translate_popover
                .popover()
                .remove_css_class("light-mode");
        } else {
            self.window.remove_css_class("dark-mode");
            self.window.add_css_class("light-mode");
            self.split_view.remove_css_class("dark-mode");
            self.split_view.add_css_class("light-mode");
            self.sidebar.container.remove_css_class("dark-mode");
            self.sidebar.container.add_css_class("light-mode");
            self.downloads_popover.popover().add_css_class("light-mode");
            if let Some(h) = self.history_popover.borrow().as_ref() {
                h.popover().add_css_class("light-mode");
            }
            if let Some(m) = self.app_menu.borrow().as_ref() {
                m.popover.add_css_class("light-mode");
            }
            self.omnibox_popup.popover().add_css_class("light-mode");
            self.shield_popover.popover().add_css_class("light-mode");
            self.translate_popover.popover().add_css_class("light-mode");
        }

        let theme_str = if is_dark { "dark" } else { "light" };
        let js = format!("if (document.documentElement) document.documentElement.setAttribute('data-theme', '{theme_str}');");
        for tab in self.tabs.borrow().iter() {
            tab.web_view
                .evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |_| {});
        }
    }

    pub(super) fn toggle_theme(self: &Rc<Self>) {
        let is_currently_dark = self.is_dark();
        let next_theme = if is_currently_dark { "light" } else { "dark" };
        self.apply_theme_setting(next_theme);
        if next_theme == "light" {
            self.toast(crate::i18n::tr("app.light_theme_enabled"));
        } else {
            self.toast(crate::i18n::tr("app.dark_theme_enabled"));
        }
    }

    pub(super) fn close_all_popovers(&self) {
        self.shield_popover.popdown();
        self.translate_popover.popdown();
        self.downloads_popover.popdown();
        if let Some(h) = self.history_popover.borrow().as_ref() {
            h.popdown();
        }
        if let Some(m) = self.app_menu.borrow().as_ref() {
            m.popover.popdown();
        }
        self.omnibox_popup.close();
        if let Some(p) = self.header.security_button.popover() {
            p.popdown();
        }
        if let Some(p) = self.header.translate_button.popover() {
            p.popdown();
        }
        if let Some(p) = self.header.burger_button.popover() {
            p.popdown();
        }
        if let Some(p) = self.header.downloads_button.popover() {
            p.popdown();
        }

        fn dismiss_child_popovers(root: &gtk4::Widget) {
            let mut child = root.first_child();
            while let Some(w) = child {
                if let Ok(pop) = w.clone().downcast::<gtk4::Popover>() {
                    pop.popdown();
                }
                dismiss_child_popovers(&w);
                child = w.next_sibling();
            }
        }

        for tab in self.tabs.borrow().iter() {
            dismiss_child_popovers(tab.web_view.upcast_ref());
        }
        if let Some(ref view) = *self.split_web_view.borrow() {
            dismiss_child_popovers(view.upcast_ref());
        }
    }

    pub(super) fn open_settings_page(self: &Rc<Self>) {
        for tab in self.tabs.borrow().iter() {
            let logical = tab.logical_url();
            if logical == SETTINGS_URL {
                self.tab_view().set_selected_page(&tab.page);
                return;
            }
        }
        self.open_tab(SETTINGS_URL, None, true);
    }

    pub(super) fn dispatch_payload(
        view: &webkit6::WebView,
        function: &str,
        payload: serde_json::Value,
    ) {
        let uri = view.uri().map(|u| u.to_string()).unwrap_or_default();
        if !crate::browser::BrowserEngine::is_internal_uri(&uri) {
            log::warn!("{function}: view navigated away from an internal page, payload dropped");
            return;
        }
        let script = format!("window.{function} && window.{function}({payload});");
        let name = function.to_owned();
        view.evaluate_javascript(
            &script,
            None,
            None,
            None::<&gio::Cancellable>,
            move |result| {
                if let Err(err) = result {
                    log::warn!("{name} failed: {err}");
                }
            },
        );
    }

    /// Builds the payload on the DB thread (or inline when there is no background
    /// writer, e.g. the in-memory test-suite store) and injects it into `view`.
    pub(super) fn push_page_data(
        &self,
        view: &webkit6::WebView,
        function: &'static str,
        build: fn(&BrowserDatabase) -> serde_json::Value,
    ) {
        if self.async_db.is_enabled() {
            let view = view.clone();
            self.async_db.query(build, move |payload| {
                Self::dispatch_payload(&view, function, payload)
            });
        } else {
            Self::dispatch_payload(view, function, build(&self.db));
        }
    }

    pub(super) fn push_start_page_data(&self, view: &webkit6::WebView) {
        self.push_page_data(view, "updateStartPageData", page_data::start_page_payload);
    }

    pub(super) fn push_trackers_page_data(&self, view: &webkit6::WebView) {
        self.push_page_data(
            view,
            "updateTrackersPageData",
            page_data::trackers_page_payload,
        );
    }

    pub(super) fn push_settings_page_data(&self, view: &webkit6::WebView) {
        let token = self.engine.borrow().ipc_token().to_string();
        if self.async_db.is_enabled() {
            let view = view.clone();
            self.async_db
                .query(page_data::settings_page_payload, move |mut payload| {
                    if let Some(map) = payload.as_object_mut() {
                        map.insert("ipc_token".to_string(), serde_json::Value::String(token));
                    }
                    Self::dispatch_payload(&view, "updateSettingsPageData", payload);
                });
        } else {
            let mut payload = page_data::settings_page_payload(&self.db);
            if let Some(map) = payload.as_object_mut() {
                map.insert("ipc_token".to_string(), serde_json::Value::String(token));
            }
            Self::dispatch_payload(view, "updateSettingsPageData", payload);
        }
    }

    pub(super) fn record_history_visit(&self, title: &str, url: &str) {
        if self.is_private_mode.get() {
            return;
        }
        if self.async_db.is_enabled() {
            self.async_db.record_history_visit(title, url);
        } else if let Err(err) = self.db.record_history_visit(title, url) {
            log::warn!("Failed to record history visit: {err}");
        }
    }

    pub(super) fn refresh_shield_popover(&self) {
        let (url, is_protected, tracker_list) = if let Some(tab) = self.current_tab() {
            let u = tab.logical_url();
            let domain = url::Url::parse(&u)
                .ok()
                .and_then(|parsed| {
                    parsed
                        .host_str()
                        .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
                })
                .unwrap_or_default();
            let is_exception = if !domain.is_empty() {
                self.db.is_protection_exception(&domain).unwrap_or(false)
            } else {
                false
            };
            (u, !is_exception, tab.blocked_trackers.borrow().clone())
        } else {
            (String::new(), true, Vec::new())
        };
        let tracker_count = tracker_list.len();
        self.shield_popover
            .update_state(&url, is_protected, tracker_count, &tracker_list);
    }

    pub(super) fn record_tracker_blocked(&self, domain: &str, rule: &str) {
        if let Some(tab) = self.current_tab() {
            let mut list = tab.blocked_trackers.borrow_mut();
            if !list.contains(&domain.to_string()) {
                list.push(domain.to_string());
            }
        }
        self.refresh_shield_popover();
        if self.is_private_mode.get() {
            return;
        }
        if self.async_db.is_enabled() {
            self.async_db.record_tracker_blocked(domain, rule);
        } else if let Err(err) = self.db.record_tracker_blocked(domain, rule) {
            log::warn!("Failed to record blocked tracker {domain}: {err}");
        }
    }

    pub(super) fn record_fingerprint_defended(&self, vector: &str, count: u64) {
        if self.is_private_mode.get() {
            return;
        }
        if self.async_db.is_enabled() {
            self.async_db.record_fingerprint_defended(vector, count);
        } else if let Err(err) = self.db.record_fingerprint_defended_n(vector, count) {
            log::warn!("Failed to record fingerprint defense {vector}: {err}");
        }
    }

    pub(super) fn clear_tracker_stats(&self) {
        if self.async_db.is_enabled() {
            self.async_db.clear_tracker_stats();
        } else if let Err(err) = self.db.clear_tracker_stats() {
            log::warn!("Failed to clear tracker stats: {err}");
        }
    }

    /// Pushes fresh numbers into every open start/privacy page right now.
    pub(super) fn refresh_privacy_pages(&self) {
        for tab in self.tabs.borrow().iter() {
            let logical = tab.logical_url();
            if logical == START_URL {
                self.push_start_page_data(&tab.web_view);
            } else if logical == TRACKERS_URL || logical == PRIVACY_URL {
                self.push_trackers_page_data(&tab.web_view);
            }
        }
    }

    /// Called for every blocked request; coalesces the page refreshes so a page with
    /// hundreds of trackers triggers one update instead of hundreds.
    pub(super) fn privacy_stats_changed(self: &Rc<Self>) {
        if self.stats_refresh_pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(2000), move || {
            if let Some(this) = weak.upgrade() {
                this.stats_refresh_pending.set(false);
                this.refresh_privacy_pages();
            }
        });
    }

    pub(super) fn on_setting_changed(&self, key: &str, value: &str) {
        log::info!("Setting updated from WebKit: {key} = {value}");

        let is_known_key = matches!(
            key,
            "clear_history"
                | "clear_tracker_stats"
                | "clear_site_permissions"
                | "clear_cache"
                | "theme"
                | "default_zoom"
                | "omnibox_suggestions"
                | "open_tabs_related"
                | "close_window_with_last_tab"
                | "sidebar_default_visible"
                | "download_dir"
                | "ask_download_location"
                | "hardware_acceleration"
                | "smooth_scrolling"
                | "enable_javascript"
                | "block_autoplay"
                | "webrtc_protection"
                | "shield_adblock"
                | "shield_fingerprint"
                | "shield_itp"
                | "shield_https_only"
                | "cookie_policy"
                | "default_search_engine"
                | "startup_action"
                | "privacy_level"
                | "privacy_config"
                | "ui_language"
                | "delete_bergamot_model"
                | "delete_all_bergamot_models"
        );
        if !is_known_key {
            log::warn!("SECURITY ALERT: Rejected unknown setting key: {key}");
            return;
        }

        if key == "ui_language" {
            let _ = self.db.set_setting("ui_language", value);
            if value == "auto" {
                crate::i18n::init_locale(None);
            } else {
                crate::i18n::set_language_from_code(value);
            }
            self.toast(crate::i18n::tr("settings.restart_required"));
            return;
        }

        if key == "delete_bergamot_model" {
            if let Err(e) = crate::browser::translator::delete_model_pair(value) {
                log::error!("Failed to delete Bergamot model {value}: {e}");
            } else {
                log::info!("Deleted Bergamot model {value} via settings");
                self.evict_translator_host_models();
                self.translate_popover.update_model_status_ui();
                self.refresh_settings_pages();
                self.toast(crate::i18n::tr("translate_model_deleted_toast"));
            }
            return;
        }

        if key == "delete_all_bergamot_models" {
            if let Err(e) = crate::browser::translator::delete_all_models() {
                log::error!("Failed to delete all Bergamot models: {e}");
            } else {
                log::info!("Deleted all Bergamot models via settings");
                self.evict_translator_host_models();
                self.translate_popover.update_model_status_ui();
                self.refresh_settings_pages();
                self.toast(crate::i18n::tr("translate_model_deleted_toast"));
            }
            return;
        }

        if key == "clear_history" {
            if let Err(e) = self.db.clear_history() {
                log::error!("Failed to clear history: {e}");
            } else {
                log::info!("History cleared successfully via settings");
                self.refresh_library();
            }
            return;
        }

        if key == "clear_tracker_stats" {
            if let Err(e) = self.db.clear_tracker_stats() {
                log::error!("Failed to clear tracker stats: {e}");
            } else {
                log::info!("Tracker stats cleared successfully via settings");
            }
            return;
        }

        if key == "clear_site_permissions" {
            if let Err(e) = self.db.clear_site_permissions() {
                log::error!("Failed to clear site permissions: {e}");
            } else {
                log::info!("Site permissions cleared successfully via settings");
            }
            return;
        }

        if key == "clear_cache" {
            self.engine.borrow().network_manager().clear_cache();
            log::info!("Browser cache cleared successfully via settings");
            return;
        }

        let _ = self.db.set_setting(key, value);
        if self.async_db.is_enabled() {
            self.async_db.set_setting(key, value);
        }

        match key {
            "theme" => {
                let style_mgr = adw::StyleManager::default();
                match value {
                    "light" => style_mgr.set_color_scheme(adw::ColorScheme::ForceLight),
                    "dark" => style_mgr.set_color_scheme(adw::ColorScheme::ForceDark),
                    _ => style_mgr.set_color_scheme(adw::ColorScheme::Default),
                }
                let is_dark = match value {
                    "dark" => true,
                    "light" => false,
                    _ => style_mgr.is_dark(),
                };
                self.sync_theme_classes(is_dark);
            }
            "default_zoom" => {
                if let Ok(zoom_num) = value.parse::<f64>() {
                    let zoom_level = zoom_num / 100.0;
                    if let Some(tab) = self.current_tab() {
                        tab.web_view.set_zoom_level(zoom_level);
                    }
                }
            }
            "omnibox_suggestions" => {
                let enabled = value != "false";
                self.omnibox_suggestions_enabled.set(enabled);
                if !enabled {
                    self.omnibox_popup.close();
                }
            }
            "open_tabs_related" => {
                self.open_tabs_related.set(value != "false");
            }
            "close_window_with_last_tab" => {
                self.close_window_with_last_tab.set(value != "false");
            }
            "sidebar_default_visible" => {
                let show = value != "false";
                self.sidebar_default_visible.set(show);
                set_sidebar_visible(&self.split_view, show);
            }
            "download_dir" => {
                let path = std::path::PathBuf::from(value);
                if path.is_absolute() && !BrowserWindow::is_dangerous_system_dir(&path) {
                    self.download_manager.set_download_dir(Some(path));
                } else {
                    log::warn!(
                        "SECURITY ALERT: Rejected invalid or dangerous download directory: {value}"
                    );
                }
            }
            "ask_download_location" => {
                self.download_manager.set_ask_destination(value == "true");
            }
            "hardware_acceleration" => {
                self.engine
                    .borrow()
                    .set_hardware_acceleration(value != "false");
            }
            "smooth_scrolling" => {
                self.engine.borrow().set_smooth_scrolling(value != "false");
            }
            "enable_javascript" => {
                let enabled = value != "false";
                self.engine.borrow().set_javascript_enabled(enabled);
                for tab in self.tabs.borrow().iter() {
                    let logical = tab.logical_url();
                    self.engine
                        .borrow()
                        .apply_settings_for_uri(&tab.web_view, &logical);
                }
            }
            "block_autoplay" => {
                self.engine.borrow().set_block_autoplay(value == "true");
            }
            "webrtc_protection" => {
                self.engine.borrow().set_webrtc_protection(value == "true");
            }
            "shield_adblock" => {
                self.engine.borrow().set_adblock_enabled(value != "false");
            }
            "shield_fingerprint" => {
                self.engine.borrow().set_anti_fingerprint(value != "false");
            }
            "shield_itp" => {
                self.engine
                    .borrow()
                    .network_manager()
                    .set_itp_enabled(value != "false");
            }
            "shield_https_only" => {
                self.engine.borrow().set_https_only(value != "false");
            }
            "cookie_policy" => {
                self.engine
                    .borrow()
                    .network_manager()
                    .set_cookie_policy(value);
            }
            "privacy_level" => {
                log::info!("Privacy level updated: {value}");
            }
            _ => {}
        }
    }

    pub(super) fn connect_sidebar(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let handler: ActionHandler = Rc::new(move |action| {
            if let Some(this) = weak.upgrade() {
                this.handle_sidebar_action(action);
            }
        });
        self.sidebar.set_handler(handler);
    }

    pub(super) fn register_tab_actions(self: &Rc<Self>) {
        let group = gio::SimpleActionGroup::new();

        let make = |name: &str, run: fn(&Rc<Inner>, &adw::TabPage)| {
            let weak = Rc::downgrade(self);
            ActionEntry::builder(name)
                .activate(move |_: &gio::SimpleActionGroup, _, _| {
                    let Some(this) = weak.upgrade() else {
                        return;
                    };
                    let page = this.context_page.borrow().clone();
                    if let Some(page) = page {
                        run(&this, &page);
                    }
                })
                .build()
        };

        group.add_action_entries([
            make("close", |this, page| this.tab_view().close_page(page)),
            make("close-others", |this, page| {
                this.tab_view().close_other_pages(page)
            }),
            make("close-right", |this, page| {
                this.tab_view().close_pages_after(page)
            }),
            make("pin", |this, page| {
                this.tab_view().set_page_pinned(page, !page.is_pinned())
            }),
            make("duplicate", |this, page| {
                if let Some(tab) = this.tab_for_page(page) {
                    let url = tab.logical_url();
                    this.open_tab(&url, Some(page), true);
                }
            }),
            make("mute", |this, page| {
                if let Some(tab) = this.tab_for_page(page) {
                    this.memory_manager.on_user_muted_toggle(tab.id);
                    tab.toggle_mute();
                }
            }),
            make("copy-url", |this, page| {
                if let Some(tab) = this.tab_for_page(page) {
                    this.window.clipboard().set_text(&tab.logical_url());
                    this.toast(crate::i18n::tr("app.link_copied"));
                }
            }),
        ]);

        self.window.insert_action_group("tab", Some(&group));
    }

    pub(super) fn register_window_actions(self: &Rc<Self>, app: &adw::Application) {
        let make = |name: &str, run: fn(&Rc<Inner>)| {
            let weak = Rc::downgrade(self);
            ActionEntry::builder(name)
                .activate(move |_: &adw::ApplicationWindow, _, _| {
                    if let Some(this) = weak.upgrade() {
                        run(&this);
                    }
                })
                .build()
        };

        let weak = Rc::downgrade(self);
        let goto_tab = ActionEntry::builder("goto-tab")
            .parameter_type(Some(glib::VariantTy::INT32))
            .activate(move |_: &adw::ApplicationWindow, _, param| {
                if let (Some(this), Some(number)) =
                    (weak.upgrade(), param.and_then(|p| p.get::<i32>()))
                {
                    this.select_number(number);
                }
            })
            .build();

        self.window.add_action_entries([
            make("new-tab", |this| this.open_user_tab()),
            make("close-tab", |this| {
                if let Some(page) = this.tab_view().selected_page() {
                    this.tab_view().close_page(&page);
                }
            }),
            make("reopen-tab", |this| this.reopen_closed_tab()),
            make("next-tab", |this| this.select_relative(1)),
            make("prev-tab", |this| this.select_relative(-1)),
            make("focus-address", |this| this.header.focus_address()),
            make("reload", |this| {
                if let Some(tab) = this.current_tab() {
                    tab.web_view.reload();
                }
            }),
            make("go-back", |this| {
                if let Some(tab) = this.current_tab() {
                    tab.web_view.go_back();
                }
            }),
            make("go-forward", |this| {
                if let Some(tab) = this.current_tab() {
                    tab.web_view.go_forward();
                }
            }),
            make("toggle-sidebar", |this| {
                set_sidebar_visible(&this.split_view, !this.split_view.shows_sidebar());
            }),
            make("bookmark", |this| this.show_bookmark_dialog()),
            make("reading-list", |this| this.add_current_to_reading_list()),
            make("find", |this| {
                if let Some(tab) = this.current_tab() {
                    this.find_bar.show_for_view(&tab.web_view);
                }
            }),
            make("translate-page", |this| {
                if let Some(tab) = this.current_tab() {
                    let detected = tab.detected_lang.borrow().clone();
                    this.translate_popover
                        .set_detected_language(detected.as_deref());
                }
                this.translate_popover.popup();
            }),
            make("toggle-fullscreen", |this| {
                if this.window.is_fullscreen() {
                    this.leave_fullscreen();
                } else {
                    this.enter_fullscreen();
                }
            }),
            make("history", |this| {
                if let Some(hist) = this.history_popover.borrow().as_ref() {
                    hist.popup();
                }
            }),
            make("downloads", |this| {
                this.header.downloads_button.set_visible(true);
                this.downloads_popover.popup();
            }),
            make("settings", |this| {
                this.open_settings_page();
            }),
            make("toggle-tab-overview", |this| {
                this.toggle_tab_overview();
            }),
            make("print", |this| {
                this.print_current_tab();
            }),
            make("new-window", |this| {
                if let Some(app) = this.window.application().and_downcast::<adw::Application>() {
                    let win = BrowserWindow::new(
                        &app,
                        this.is_test_suite,
                        &[],
                        this.is_private_mode.get(),
                    );
                    win.present();
                }
            }),
            make("zoom-in", |this| {
                this.zoom_in();
            }),
            make("zoom-out", |this| {
                this.zoom_out();
            }),
            make("zoom-reset", |this| {
                this.zoom_reset();
            }),
            make("toggle-split-view", |this| this.toggle_split_view()),
            make("toggle-reader-mode", |this| this.toggle_reader_mode()),
            make("install-pwa", |this| this.install_current_page_as_pwa()),
            goto_tab,
        ]);

        let accels: [(&str, &[&str]); 28] = [
            ("win.new-tab", &["<Primary>t"]),
            ("win.new-window", &["<Primary>n"]),
            ("win.close-tab", &["<Primary>w"]),
            ("win.reopen-tab", &["<Primary><Shift>t"]),
            ("win.next-tab", &["<Primary>Tab", "<Primary>Page_Down"]),
            ("win.prev-tab", &["<Primary><Shift>Tab", "<Primary>Page_Up"]),
            ("win.focus-address", &["<Primary>l", "<Alt>d"]),
            ("win.reload", &["<Primary>r", "F5"]),
            ("win.go-back", &["<Alt>Left"]),
            ("win.go-forward", &["<Alt>Right"]),
            ("win.toggle-sidebar", &["<Primary><Shift>s"]),
            ("win.toggle-split-view", &["F3", "<Primary>backslash"]),
            ("win.toggle-reader-mode", &["F9", "<Primary><Alt>r"]),
            ("win.bookmark", &["<Primary>d"]),
            ("win.reading-list", &["<Primary><Shift>d"]),
            ("win.find", &["<Primary>f"]),
            ("win.translate-page", &["<Alt><Shift>t"]),
            ("win.toggle-fullscreen", &["F11"]),
            ("win.history", &["<Primary>h"]),
            ("win.downloads", &["<Primary>j"]),
            ("win.settings", &["<Primary>comma"]),
            ("win.toggle-tab-overview", &["<Primary><Shift>o", "F10"]),
            ("win.print", &["<Primary>p"]),
            (
                "win.zoom-in",
                &["<Primary>plus", "<Primary>equal", "<Primary>KP_Add"],
            ),
            ("win.zoom-out", &["<Primary>minus", "<Primary>KP_Subtract"]),
            ("win.zoom-reset", &["<Primary>0", "<Primary>KP_0"]),
            ("win.goto-tab(9)", &["<Primary>9"]),
            ("win.goto-tab(1)", &["<Primary>1"]),
        ];
        for (action, keys) in accels {
            app.set_accels_for_action(action, keys);
        }
        for number in 2..=8 {
            app.set_accels_for_action(
                &format!("win.goto-tab({number})"),
                &[&format!("<Primary>{number}")],
            );
        }
    }

    pub(super) fn handle_sidebar_action(self: &Rc<Self>, action: SidebarAction) {
        match action {
            SidebarAction::ClosePrivateSession => self.wipe_private_session(),
            SidebarAction::NewTab => self.open_user_tab(),
            SidebarAction::NewGroup => {
                if self.is_private_mode.get() {
                    self.toast(crate::i18n::tr("app.groups_disabled_in_private"));
                } else {
                    self.prompt_group_name(None);
                }
            }
            SidebarAction::AddBookmark => {
                if self.is_private_mode.get() {
                    self.toast(crate::i18n::tr("app.bookmarks_disabled_in_private"));
                } else {
                    self.show_bookmark_dialog();
                }
            }
            SidebarAction::SelectTab(page) => {
                if self.tab_view().page_position(&page) >= 0 {
                    self.tab_view().set_selected_page(&page);
                }
            }
            SidebarAction::CloseTab(page) => {
                if self.tab_view().page_position(&page) >= 0 {
                    self.tab_view().close_page(&page);
                }
            }
            SidebarAction::CycleTabGroup(pages) => {
                let selected = self.tab_view().selected_page();
                let next_page = if let Some(curr) = selected {
                    if let Some(pos) = pages.iter().position(|p| p == &curr) {
                        pages[(pos + 1) % pages.len()].clone()
                    } else {
                        pages.first().cloned().unwrap_or(curr)
                    }
                } else if let Some(first) = pages.first().cloned() {
                    first
                } else {
                    return;
                };
                if self.tab_view().page_position(&next_page) >= 0 {
                    self.tab_view().set_selected_page(&next_page);
                }
            }
            SidebarAction::CloseDuplicateTab(pages) => {
                let selected = self.tab_view().selected_page();
                let to_close = if let Some(curr) = selected {
                    if pages.iter().any(|p| p == &curr) {
                        curr
                    } else {
                        pages.last().cloned().unwrap_or(curr)
                    }
                } else if let Some(last) = pages.last().cloned() {
                    last
                } else {
                    return;
                };
                if self.tab_view().page_position(&to_close) >= 0 {
                    self.tab_view().close_page(&to_close);
                }
            }
            SidebarAction::TabContextMenu(page, rect) => self.popup_tab_menu(&page, &rect),
            SidebarAction::SelectGroup(id) => self.switch_group(id),
            SidebarAction::RenameGroup(id) => self.prompt_group_name(Some(id)),
            SidebarAction::DeleteGroup(id) => self.delete_group(&id),
            SidebarAction::CloseGroupTabs(id) => self.close_group_tabs(&id),
            SidebarAction::OpenUrl(url) => self.open_in_current_tab(&url),
            SidebarAction::OpenUrlInNewTab(url) => {
                self.open_tab(&url, None, false);
            }
            SidebarAction::OpenReadingItem {
                id,
                url,
                offline_path,
            } => {
                if let Some(path) = offline_path
                    .as_deref()
                    .filter(|p| std::path::Path::new(p).exists())
                {
                    self.open_in_current_tab(&format!("file://{path}"));
                    self.toast(crate::i18n::tr("app.offline_copy_opened"));
                } else {
                    self.open_in_current_tab(&url);
                    self.toast(crate::i18n::tr("app.offline_copy_not_found"));
                }
                if let Err(err) = self.db.mark_reading_item_read(id, true) {
                    log::warn!("cannot mark reading item {id} as read: {err}");
                }
                self.library_changed();
            }
            SidebarAction::ToggleRead { id, read } => {
                if let Err(err) = self.db.mark_reading_item_read(id, read) {
                    log::warn!("cannot update reading item {id}: {err}");
                } else if read {
                    self.toast(crate::i18n::tr("app.marked_as_read"));
                } else {
                    self.toast(crate::i18n::tr("app.marked_as_unread"));
                }
                self.library_changed();
            }
            SidebarAction::DeleteReadingItem(id) => {
                match self.db.delete_reading_item(id) {
                    Ok(Some(path)) => {
                        let file_path = std::path::Path::new(&path);
                        if file_path.exists() {
                            if let Err(err) = std::fs::remove_file(file_path) {
                                log::warn!("cannot remove offline archive file {path}: {err}");
                            } else {
                                log::info!("Removed offline archive file {path}");
                            }
                        }
                        self.toast(crate::i18n::tr("app.removed_from_reading_list"));
                    }
                    Ok(None) => {
                        self.toast(crate::i18n::tr("app.removed_from_reading_list"));
                    }
                    Err(err) => {
                        log::warn!("cannot delete reading item {id}: {err}");
                        self.toast(crate::i18n::tr("app.delete_page_failed"));
                    }
                }
                self.library_changed();
            }
            SidebarAction::DeleteBookmark(id) => {
                match self.db.delete_bookmark(id) {
                    Ok(()) => {
                        self.toast(crate::i18n::tr("app.bookmark_deleted"));
                    }
                    Err(err) => {
                        log::warn!("cannot delete bookmark {id}: {err}");
                        self.toast(crate::i18n::tr("app.bookmark_delete_failed"));
                    }
                }
                self.library_changed();
            }
        }
    }

    pub(super) fn is_bookmarked(&self, url: &str) -> bool {
        if !is_web_url(url) {
            return false;
        }
        self.db.is_bookmarked(url).unwrap_or_else(|err| {
            log::warn!("bookmark lookup failed for {url}: {err}");
            false
        })
    }

    pub(super) fn refresh_library(&self) {
        if self.is_private_mode.get() {
            self.sidebar.render_reading_list(&[]);
            self.sidebar.render_bookmarks(&[]);
            return;
        }
        match self.db.get_reading_list(false) {
            Ok(items) => self.sidebar.render_reading_list(&items),
            Err(err) => log::warn!("cannot load reading list: {err}"),
        }
        match self.db.get_bookmarks(None) {
            Ok(bookmarks) => self.sidebar.render_bookmarks(&bookmarks),
            Err(err) => log::warn!("cannot load bookmarks: {err}"),
        }
    }

    pub(super) fn library_changed(&self) {
        self.refresh_library();
        self.sync_chrome();
        for tab in self.tabs.borrow().iter() {
            let logical = tab.logical_url();
            if logical == START_URL {
                self.push_start_page_data(&tab.web_view);
            }
        }
    }

    pub(super) fn show_bookmark_dialog(self: &Rc<Self>) {
        let Some(tab) = self.current_tab() else {
            return;
        };
        let url = tab.logical_url();
        if !is_web_url(&url) {
            self.toast(crate::i18n::tr("app.cannot_bookmark_page"));
            return;
        }

        let existing = self
            .db
            .get_bookmarks(None)
            .ok()
            .and_then(|all| all.into_iter().find(|b| b.url == url));
        let title = existing
            .as_ref()
            .map(|b| b.title.clone())
            .or_else(|| {
                tab.web_view
                    .title()
                    .map(|t| t.to_string())
                    .filter(|t| !t.is_empty())
            })
            .or_else(|| host_of(&url))
            .unwrap_or_else(|| url.clone());
        let folder = existing
            .as_ref()
            .and_then(|b| b.folder.clone())
            .map(|f| {
                if crate::storage::database::is_favorites_folder(&f) {
                    crate::i18n::tr("app.bookmark_folder_favorites").to_string()
                } else {
                    f
                }
            })
            .unwrap_or_else(|| crate::i18n::tr("app.bookmark_folder_favorites").to_string());

        let title_entry = Entry::builder()
            .text(&title)
            .activates_default(true)
            .build();
        let folder_entry = Entry::builder()
            .text(&folder)
            .placeholder_text(crate::i18n::tr("app.folder_optional_placeholder"))
            .activates_default(true)
            .build();
        let form = GtkBox::new(Orientation::Vertical, 8);
        form.append(&title_entry);
        form.append(&folder_entry);

        let dialog = adw::AlertDialog::new(
            Some(if existing.is_some() {
                crate::i18n::tr("app.edit_bookmark_dialog")
            } else {
                crate::i18n::tr("app.new_bookmark_dialog")
            }),
            Some(&url),
        );
        dialog.add_response("cancel", crate::i18n::tr("private_lock.cancel"));
        if existing.is_some() {
            dialog.add_response("remove", crate::i18n::tr("app.delete"));
            dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        }
        dialog.add_response("save", crate::i18n::tr("app.save"));
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        dialog.set_extra_child(Some(&form));

        let weak = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            match response {
                "save" => {
                    let name = title_entry.text().trim().to_string();
                    let name = if name.is_empty() { url.clone() } else { name };
                    let folder_text = folder_entry.text().trim().to_string();
                    let folder = (!folder_text.is_empty()).then(|| {
                        if folder_text == crate::i18n::tr("app.bookmark_folder_favorites")
                            || crate::storage::database::is_favorites_folder(&folder_text)
                        {
                            FAVORITES_FOLDER.to_string()
                        } else {
                            folder_text
                        }
                    });
                    match this.db.add_bookmark(&name, &url, folder.as_deref()) {
                        Ok(_) => this.toast(crate::i18n::tr("app.bookmark_saved")),
                        Err(err) => {
                            log::warn!("cannot save bookmark {url}: {err}");
                            this.toast(crate::i18n::tr("app.bookmark_save_failed"));
                        }
                    }
                    this.library_changed();
                }
                "remove" => {
                    if let Err(err) = this.db.delete_bookmark_by_url(&url) {
                        log::warn!("cannot remove bookmark {url}: {err}");
                    }
                    this.library_changed();
                }
                _ => {}
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn add_current_to_reading_list(self: &Rc<Self>) {
        let Some(tab) = self.current_tab() else {
            return;
        };
        let url = tab.logical_url();
        if !is_web_url(&url) {
            self.toast(crate::i18n::tr("app.cannot_add_reading_list"));
            return;
        }

        let domain = host_of(&url).unwrap_or_else(|| url.clone());
        let title = tab
            .web_view
            .title()
            .map(|t| t.to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| domain.clone());

        let web_view = tab.web_view.clone();
        let weak = Rc::downgrade(self);
        tab.web_view.evaluate_javascript(
            PREVIEW_SCRIPT,
            None,
            None,
            None::<&gio::Cancellable>,
            move |result| {
                let preview = match result {
                    Ok(value) if value.is_string() => {
                        Some(value.to_str().to_string()).filter(|text| !text.is_empty())
                    }
                    Ok(_) => None,
                    Err(err) => {
                        log::warn!("cannot read page preview: {err}");
                        None
                    }
                };
                let Some(this) = weak.upgrade() else {
                    return;
                };
                match this
                    .db
                    .add_reading_item(&title, &url, &domain, preview.as_deref())
                {
                    Ok(id) => {
                        this.toast(crate::i18n::tr("app.added_to_reading_list"));
                        this.library_changed();

                        // Asynchronously save MHTML archive for offline reading
                        let mut offline_dir = glib::user_data_dir();
                        offline_dir.push("takeweb");
                        offline_dir.push("offline_reading");
                        if let Err(e) = std::fs::create_dir_all(&offline_dir) {
                            log::warn!("Failed to create offline reading directory: {e}");
                            return;
                        }

                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let file_path = offline_dir.join(format!("page_{id}_{timestamp}.mht"));
                        let file_path_str = file_path.to_string_lossy().to_string();
                        let gio_file = gio::File::for_path(&file_path);

                        let weak_offline = weak.clone();
                        web_view.save_to_file(
                            &gio_file,
                            SaveMode::Mhtml,
                            None::<&gio::Cancellable>,
                            move |save_res| match save_res {
                                Ok(()) => {
                                    log::info!("Saved offline MHTML archive to {file_path_str}");
                                    if let Some(this) = weak_offline.upgrade() {
                                        if let Err(e) = this
                                            .db
                                            .set_reading_item_offline_path(id, Some(&file_path_str))
                                        {
                                            log::warn!(
                                                "Failed to set offline path in database: {e}"
                                            );
                                        }
                                        this.toast(crate::i18n::tr("app.offline_saved"));
                                        this.library_changed();
                                    }
                                }
                                Err(err) => {
                                    log::warn!(
                                        "Failed to save MHTML archive {file_path_str}: {err}"
                                    );
                                    if let Some(this) = weak_offline.upgrade() {
                                        this.toast(crate::i18n::tr("app.offline_save_failed"));
                                    }
                                }
                            },
                        );
                    }
                    Err(err) => {
                        log::warn!("cannot add {url} to reading list: {err}");
                        this.toast(crate::i18n::tr("app.save_page_failed"));
                    }
                }
            },
        );
    }

    pub(super) fn find_tab_by_url_or_current(self: &Rc<Self>, url: &str) -> Option<Rc<BrowserTab>> {
        if !url.is_empty() {
            for tab in self.tabs.borrow().iter() {
                if tab.logical_url() == url || tab.web_view.uri().as_deref() == Some(url) {
                    return Some(Rc::clone(tab));
                }
            }
        }
        self.current_tab()
    }

    pub(super) fn maybe_trigger_auto_translate(
        self: &Rc<Self>,
        tab: &Rc<BrowserTab>,
        detected_lang: &str,
        url: &str,
    ) {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return;
        }

        if let Some(ref last_url) = *tab.auto_translated_for_uri.borrow() {
            if last_url == url {
                return;
            }
        }

        if let Some(target_lang) = self.db.is_auto_translate_enabled(detected_lang) {
            if detected_lang != target_lang {
                *tab.auto_translated_for_uri.borrow_mut() = Some(url.to_string());
                log::info!(
                    "Auto-translating tab #{} from {} to {} for {}",
                    tab.id,
                    detected_lang,
                    target_lang,
                    url
                );
                self.start_bergamot_translation_flow_for_tab(
                    Some(Rc::clone(tab)),
                    detected_lang,
                    &target_lang,
                    "page",
                    "auto_page",
                );
            }
        }
    }

    pub(super) fn trigger_auto_translate_if_needed(
        self: &Rc<Self>,
        tab: &Rc<BrowserTab>,
        url: &str,
    ) {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return;
        }

        if !self.db.is_any_auto_translate_enabled() {
            return;
        }

        if let Some(ref last_url) = *tab.auto_translated_for_uri.borrow() {
            if last_url == url {
                return;
            }
        }

        let target_lang = self
            .db
            .get_setting("translate_auto_target_lang")
            .ok()
            .flatten()
            .unwrap_or_else(|| "ru".to_string());

        let detected = tab.detected_lang.borrow().clone();

        if let Some(detected) = detected {
            if detected != target_lang {
                *tab.auto_translated_for_uri.borrow_mut() = Some(url.to_string());
                log::info!(
                    "Triggering auto-translate for tab #{} ({detected} -> {target_lang}) on {url}",
                    tab.id
                );
                self.start_bergamot_translation_flow_for_tab(
                    Some(Rc::clone(tab)),
                    &detected,
                    &target_lang,
                    "page",
                    "auto_page",
                );
            }
        }
    }

    pub(super) fn toggle_reader_mode(self: &Rc<Self>) {
        let Some(tab) = self.current_tab() else {
            return;
        };
        let url = tab.logical_url();
        if url.starts_with("take://") {
            self.toast(crate::i18n::tr("reader_mode_not_available"));
            return;
        }

        tab.web_view.evaluate_javascript(
            "if (window.__TakeReaderMode) { window.__TakeReaderMode.toggle(); }",
            None,
            None,
            None::<&gio::Cancellable>,
            |_| {},
        );
    }

    pub(super) fn on_reader_mode_changed(&self, active: bool) {
        self.is_reader_active.set(active);
        if active {
            self.header.reader_button.add_css_class("suggested-action");
            self.header.reader_button.add_css_class("active");
        } else {
            self.header
                .reader_button
                .remove_css_class("suggested-action");
            self.header.reader_button.remove_css_class("active");
        }
    }

    pub(super) fn install_current_page_as_pwa(self: &Rc<Self>) {
        let Some(tab) = self.current_tab() else {
            return;
        };
        let url = tab.logical_url();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.toast(crate::i18n::tr("pwa_install_invalid_url"));
            return;
        }

        let parsed_url = match url::Url::parse(&url) {
            Ok(u) => u,
            Err(_) => {
                self.toast(crate::i18n::tr("pwa_install_invalid_url"));
                return;
            }
        };

        let host = parsed_url.host_str().unwrap_or("app").to_string();
        let domain_clean = host.trim_start_matches("www.");
        let page_title = tab
            .web_view
            .title()
            .map(|t| t.to_string())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| domain_clean.to_string());

        let slug: String = domain_clean
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let app_id = format!("takeweb-app-{}", slug);

        // Save favicon if available to ~/.local/share/icons/<app_id>.png
        let icons_dir = glib::user_data_dir().join("icons");
        let _ = std::fs::create_dir_all(&icons_dir);
        let icon_path = icons_dir.join(format!("{app_id}.png"));

        let mut icon_saved = false;
        if let Some(texture) = tab.web_view.favicon() {
            if texture.save_to_png(&icon_path).is_ok() {
                icon_saved = true;
            }
        }
        if !icon_saved {
            let cached_fav = glib::user_data_dir()
                .join("takeweb")
                .join("favicons")
                .join(format!("{domain_clean}.png"));
            if cached_fav.exists() {
                let _ = std::fs::copy(&cached_fav, &icon_path);
                icon_saved = true;
            }
        }
        if !icon_saved {
            let fallback = std::path::PathBuf::from("/usr/share/takeweb/icons/takeweb.png");
            if fallback.exists() {
                let _ = std::fs::copy(&fallback, &icon_path);
            } else if let Ok(exe) = std::env::current_exe() {
                if let Some(parent) = exe.parent() {
                    let local_icon = parent.join("resources/icons/takeweb.png");
                    if local_icon.exists() {
                        let _ = std::fs::copy(&local_icon, &icon_path);
                    }
                }
            }
        }

        let apps_dir = glib::user_data_dir().join("applications");
        let _ = std::fs::create_dir_all(&apps_dir);
        let desktop_path = apps_dir.join(format!("{app_id}.desktop"));

        let exe_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok().or(Some(p)))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "takeweb".to_string());

        if let Ok(curr_exe) = std::env::current_exe() {
            if let Some(home) = std::env::var_os("HOME") {
                let local_bin = std::path::PathBuf::from(home).join(".local/bin/takeweb");
                if local_bin.exists() && curr_exe != local_bin {
                    let _ = std::fs::copy(&curr_exe, &local_bin);
                }
            }
        }

        let desktop_content = format!(
            "[Desktop Entry]\n\
            Version=1.0\n\
            Type=Application\n\
            Name={page_title}\n\
            Comment=Web App powered by TakeWeb\n\
            Exec={exe_path} --app={url}\n\
            Icon={icon_str}\n\
            Terminal=false\n\
            Categories=Network;WebBrowser;\n\
            StartupWMClass=takeweb\n",
            page_title = page_title,
            exe_path = exe_path,
            url = url,
            icon_str = icon_path.to_string_lossy(),
        );

        match std::fs::write(&desktop_path, desktop_content) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &desktop_path,
                        std::fs::Permissions::from_mode(0o755),
                    );
                }
                let _ = std::process::Command::new("update-desktop-database")
                    .arg(&apps_dir)
                    .spawn();
                log::info!("Installed PWA desktop entry at {}", desktop_path.display());
                let toast_msg = format!(
                    "{} {}",
                    crate::i18n::tr("pwa_installed_success"),
                    page_title
                );
                self.toast(&toast_msg);
            }
            Err(e) => {
                log::error!("Failed to create .desktop file: {e}");
                self.toast(crate::i18n::tr("pwa_install_failed"));
            }
        }
    }

    pub(super) fn set_split_pane_focused(&self, right_focused: bool) {
        self.is_split_focused.set(right_focused);
        if let Some(ref split_box) = *self.split_container.borrow() {
            if right_focused {
                split_box.add_css_class("take-split-active-pane");
            } else {
                split_box.remove_css_class("take-split-active-pane");
            }
        }
        if let Some(start_widget) = self.view_paned.start_child() {
            if !right_focused && self.is_split_view.get() {
                start_widget.add_css_class("take-split-active-pane");
            } else {
                start_widget.remove_css_class("take-split-active-pane");
            }
        }
        self.sync_chrome();
    }

    pub(super) fn toggle_split_view(self: &Rc<Self>) {
        if self.is_split_view.get() {
            self.close_split_view();
        } else {
            self.open_split_view(None);
        }
    }

    pub(super) fn close_split_view(self: &Rc<Self>) {
        self.is_split_view.set(false);
        self.is_split_focused.set(false);
        if let Some(start_widget) = self.view_paned.start_child() {
            start_widget.remove_css_class("take-split-active-pane");
            start_widget.remove_css_class("take-split-left-pane");
        }
        self.view_paned.set_end_child(None::<&gtk4::Widget>);
        *self.split_container.borrow_mut() = None;
        *self.split_web_view.borrow_mut() = None;
        self.sync_chrome();
    }

    pub(super) fn open_split_view(self: &Rc<Self>, url_opt: Option<&str>) {
        let split_box = GtkBox::new(Orientation::Vertical, 0);
        split_box.set_hexpand(true);
        split_box.set_vexpand(true);
        split_box.add_css_class("take-split-box");
        split_box.add_css_class("take-split-active-pane");

        if let Some(start_widget) = self.view_paned.start_child() {
            start_widget.add_css_class("take-split-left-pane");
            start_widget.remove_css_class("take-split-active-pane");
        }

        // Track focus when clicking anywhere inside the right split pane
        {
            let click_right = gtk4::GestureClick::new();
            click_right.set_propagation_phase(gtk4::PropagationPhase::Capture);
            click_right.set_button(0);
            let weak_right = Rc::downgrade(self);
            click_right.connect_pressed(move |_, _, _, _| {
                if let Some(this) = weak_right.upgrade() {
                    if this.is_split_view.get() && !this.is_split_focused.get() {
                        this.set_split_pane_focused(true);
                    }
                }
            });
            split_box.add_controller(click_right);
        }

        let split_overlay = gtk4::Overlay::new();
        split_overlay.set_hexpand(true);
        split_overlay.set_vexpand(true);

        let split_stack = gtk4::Stack::new();
        split_stack.set_hexpand(true);
        split_stack.set_vexpand(true);
        split_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        split_stack.set_transition_duration(140);

        let web_view = if self.is_private_mode.get() {
            self.engine.borrow().create_private_web_view()
        } else {
            self.engine.borrow().create_web_view()
        };
        web_view.set_hexpand(true);
        web_view.set_vexpand(true);

        // Floating mini-controls in top-right corner of the right pane
        let floating_bar = GtkBox::new(Orientation::Horizontal, 4);
        floating_bar.add_css_class("take-split-floating-bar");
        floating_bar.set_halign(gtk4::Align::End);
        floating_bar.set_valign(gtk4::Align::Start);
        floating_bar.set_margin_top(8);
        floating_bar.set_margin_end(10);

        let switch_tab_btn = Button::builder()
            .icon_name("view-list-symbolic")
            .tooltip_text(crate::i18n::tr("split_view_change_tab_tooltip"))
            .css_classes(["flat", "circular", "take-split-float-btn"])
            .visible(url_opt.is_some())
            .build();

        let close_btn = Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text(crate::i18n::tr("split_view_close_tooltip"))
            .css_classes(["flat", "circular", "take-split-close-btn"])
            .build();

        floating_bar.append(&switch_tab_btn);
        floating_bar.append(&close_btn);

        // --- Page 1: Firefox-style Tab Picker ---
        let picker_root = GtkBox::new(Orientation::Vertical, 0);
        picker_root.set_hexpand(true);
        picker_root.set_vexpand(true);
        picker_root.add_css_class("take-split-picker");

        let picker_center = GtkBox::new(Orientation::Vertical, 14);
        picker_center.set_halign(gtk4::Align::Center);
        picker_center.set_valign(gtk4::Align::Center);
        picker_center.set_vexpand(true);
        picker_center.set_margin_start(24);
        picker_center.set_margin_end(24);

        let picker_title = gtk4::Label::builder()
            .label(crate::i18n::tr("split_view_picker_title"))
            .css_classes(["take-split-picker-title"])
            .halign(gtk4::Align::Center)
            .justify(gtk4::Justification::Center)
            .wrap(true)
            .build();

        let search_entry = Entry::builder()
            .placeholder_text(crate::i18n::tr("split_view_search_tabs"))
            .primary_icon_name("system-search-symbolic")
            .width_request(330)
            .halign(gtk4::Align::Center)
            .css_classes(["take-split-search-entry"])
            .build();

        let card_box = GtkBox::new(Orientation::Vertical, 2);
        card_box.add_css_class("take-split-tabs-card");
        card_box.set_width_request(330);
        card_box.set_halign(gtk4::Align::Center);

        let tabs_list_box = GtkBox::new(Orientation::Vertical, 2);

        // Collect open tabs for the picker list
        let current_tab_id = self.current_tab().map(|t| t.id);
        let mut picker_items: Vec<(String, String, Option<gtk4::gdk::Texture>)> = Vec::new();
        {
            let tabs = self.tabs.borrow();
            // Put other open tabs first, then current tab
            for tab in tabs.iter().filter(|t| Some(t.id) != current_tab_id) {
                let url = tab.logical_url();
                let title = tab
                    .web_view
                    .title()
                    .map(|t| t.to_string())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| url.clone());
                picker_items.push((title, url, tab.web_view.favicon()));
            }
            for tab in tabs.iter().filter(|t| Some(t.id) == current_tab_id) {
                let url = tab.logical_url();
                let title = tab
                    .web_view
                    .title()
                    .map(|t| t.to_string())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| url.clone());
                picker_items.push((title, url, tab.web_view.favicon()));
            }
        }
        // Always include Start Page at the bottom if not already in the list
        if !picker_items.iter().any(|(_, u, _)| u == START_URL) {
            picker_items.push((
                crate::i18n::tr("split_view_new_startpage").to_string(),
                START_URL.to_string(),
                None,
            ));
        }

        let mut row_entries: Vec<(Button, String, String)> = Vec::new();
        for (title, target_url, fav_tex) in picker_items {
            let row_btn = Button::builder()
                .css_classes(["flat", "take-split-tab-item"])
                .hexpand(true)
                .build();

            let row_box = GtkBox::new(Orientation::Horizontal, 10);
            row_box.set_valign(gtk4::Align::Center);

            let icon = if let Some(ref tex) = fav_tex {
                gtk4::Image::from_paintable(Some(tex))
            } else {
                gtk4::Image::from_icon_name("applications-internet-symbolic")
            };
            icon.set_pixel_size(16);
            icon.add_css_class("take-split-tab-icon");

            let lbl = gtk4::Label::builder()
                .label(&title)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .max_width_chars(38)
                .css_classes(["take-split-tab-label"])
                .build();

            row_box.append(&icon);
            row_box.append(&lbl);
            row_btn.set_child(Some(&row_box));

            let view_clone = web_view.clone();
            let stack_clone = split_stack.clone();
            let sw_btn_clone = switch_tab_btn.clone();
            let url_for_click = target_url.clone();
            let weak_self = Rc::downgrade(self);
            row_btn.connect_clicked(move |_| {
                if let Some(this) = weak_self.upgrade() {
                    this.engine
                        .borrow()
                        .apply_settings_for_uri(&view_clone, &url_for_click);
                    view_clone.load_uri(&url_for_click);
                    stack_clone.set_visible_child_name("webview");
                    sw_btn_clone.set_visible(true);
                    this.set_split_pane_focused(true);
                }
            });

            tabs_list_box.append(&row_btn);
            row_entries.push((row_btn, title.to_lowercase(), target_url.to_lowercase()));
        }

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .propagate_natural_height(true)
            .max_content_height(260)
            .child(&tabs_list_box)
            .build();
        card_box.append(&scrolled);

        picker_center.append(&picker_title);
        picker_center.append(&search_entry);
        picker_center.append(&card_box);
        picker_root.append(&picker_center);

        // Live filter tabs on search entry change
        let rows_for_filter = Rc::new(row_entries);
        {
            let rows = Rc::clone(&rows_for_filter);
            search_entry.connect_changed(move |entry| {
                let q = entry.text().trim().to_lowercase();
                for (btn, title_lc, url_lc) in rows.iter() {
                    let visible = q.is_empty() || title_lc.contains(&q) || url_lc.contains(&q);
                    btn.set_visible(visible);
                }
            });
        }

        // Pressing Enter in search entry opens matching tab or navigates to typed URL/query
        {
            let rows = Rc::clone(&rows_for_filter);
            let view_clone = web_view.clone();
            let stack_clone = split_stack.clone();
            let sw_btn_clone = switch_tab_btn.clone();
            let weak_self = Rc::downgrade(self);
            search_entry.connect_activate(move |entry| {
                let text = entry.text().trim().to_string();
                if text.is_empty() {
                    return;
                }
                let q = text.to_lowercase();
                if !text.contains("://") && !text.contains('.') {
                    if let Some((btn, _, _)) = rows
                        .iter()
                        .find(|(b, t, u)| b.is_visible() && (t.contains(&q) || u.contains(&q)))
                    {
                        btn.emit_clicked();
                        return;
                    }
                }
                if let Some(this) = weak_self.upgrade() {
                    let engine = this.current_search_engine();
                    let target =
                        crate::browser::BrowserEngine::resolve_url_with_engine(&text, &engine);
                    this.engine
                        .borrow()
                        .apply_settings_for_uri(&view_clone, &target);
                    view_clone.load_uri(&target);
                    stack_clone.set_visible_child_name("webview");
                    sw_btn_clone.set_visible(true);
                    this.set_split_pane_focused(true);
                }
            });
        }

        split_stack.add_named(&picker_root, Some("picker"));
        split_stack.add_named(&web_view, Some("webview"));

        if let Some(initial_url) = url_opt {
            self.engine
                .borrow()
                .apply_settings_for_uri(&web_view, initial_url);
            web_view.load_uri(initial_url);
            split_stack.set_visible_child_name("webview");
        } else {
            split_stack.set_visible_child_name("picker");
        }

        // Switch back to picker when clicking switch_tab_btn
        {
            let stack_clone = split_stack.clone();
            let sw_btn_clone = switch_tab_btn.clone();
            let search_clone = search_entry.clone();
            switch_tab_btn.connect_clicked(move |_| {
                stack_clone.set_visible_child_name("picker");
                sw_btn_clone.set_visible(false);
                search_clone.set_text("");
                search_clone.grab_focus();
            });
        }

        // Sync top omnibox when split_web_view loads or changes URI
        {
            let weak_self = Rc::downgrade(self);
            web_view.connect_load_changed(move |_, _| {
                if let Some(this) = weak_self.upgrade() {
                    if this.is_split_view.get() && this.is_split_focused.get() {
                        this.sync_chrome();
                    }
                }
            });
        }
        {
            let weak_self = Rc::downgrade(self);
            web_view.connect_uri_notify(move |_| {
                if let Some(this) = weak_self.upgrade() {
                    if this.is_split_view.get() && this.is_split_focused.get() {
                        this.sync_chrome();
                    }
                }
            });
        }

        {
            let weak_self = Rc::downgrade(self);
            close_btn.connect_clicked(move |_| {
                if let Some(this) = weak_self.upgrade() {
                    this.close_split_view();
                }
            });
        }

        split_overlay.set_child(Some(&split_stack));
        split_overlay.add_overlay(&floating_bar);
        split_box.append(&split_overlay);

        self.view_paned.set_end_child(Some(&split_box));

        // Strictly divide 50/50 using view_paned's actual width (excluding sidebar)
        let paned_w = self.view_paned.width();
        let half = if paned_w > 100 {
            paned_w / 2
        } else {
            (self.window.width() / 2).max(400)
        };
        self.view_paned.set_position(half);

        let paned_idle = self.view_paned.clone();
        glib::idle_add_local_once(move || {
            let w = paned_idle.width();
            if w > 100 {
                paned_idle.set_position(w / 2);
            }
        });
        let paned_delayed = self.view_paned.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || {
            let w = paned_delayed.width();
            if w > 100 {
                paned_delayed.set_position(w / 2);
            }
        });

        *self.split_container.borrow_mut() = Some(split_box);
        *self.split_web_view.borrow_mut() = Some(web_view);
        self.is_split_view.set(true);
        self.is_split_focused.set(true);
    }

    pub(super) fn dispatch_media_command(
        self: &Rc<Self>,
        cmd: crate::browser::mpris::MediaCommand,
    ) {
        use crate::browser::mpris::MediaCommand;
        match cmd {
            MediaCommand::Raise => {
                self.window.present();
            }
            MediaCommand::Quit => {
                self.window.close();
            }
            MediaCommand::PlayPause => {
                if let Some(tab) = self.current_tab() {
                    let js = "if (window.__take_media_toggle__) { window.__take_media_toggle__(); } else { \
                        const m = Array.from(document.querySelectorAll('video, audio')); \
                        if (m.length > 0) { \
                            const anyPlaying = m.some(el => !el.paused && !el.ended); \
                            m.forEach(el => anyPlaying ? el.pause() : el.play()); \
                        } \
                    }";
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
            MediaCommand::Play => {
                if let Some(tab) = self.current_tab() {
                    let js = "document.querySelectorAll('video, audio').forEach(el => el.play());";
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
            MediaCommand::Pause => {
                if let Some(tab) = self.current_tab() {
                    let js = "document.querySelectorAll('video, audio').forEach(el => el.pause());";
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
            MediaCommand::Stop => {
                if let Some(tab) = self.current_tab() {
                    let js = "document.querySelectorAll('video, audio').forEach(el => { el.pause(); el.currentTime = 0; });";
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
            MediaCommand::Next => {
                if let Some(tab) = self.current_tab() {
                    let js = "document.querySelector('.ytp-next-button, [aria-label*=\"Next\"], [aria-label*=\"Следующ\"], button[title*=\"Next\"]')?.click();";
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
            MediaCommand::Previous => {
                if let Some(tab) = self.current_tab() {
                    let js = "document.querySelector('.ytp-prev-button, [aria-label*=\"Previous\"], [aria-label*=\"Предыдущ\"], button[title*=\"Previous\"]')?.click();";
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
        }
    }
}
