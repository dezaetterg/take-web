use std::rc::Rc;

use gtk4::gio::{self, ActionEntry};
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Entry, Orientation};
use libadwaita as adw;
use libadwaita::prelude::*;
use webkit6::prelude::*;
use webkit6::{PrintOperation, SaveMode};

use super::{
    host_of, is_web_url, set_sidebar_visible, BrowserWindow, Inner, PREVIEW_SCRIPT, PRIVACY_URL,
    SETTINGS_URL, START_URL, TRACKERS_URL,
};
use crate::browser::session_manager::SessionManager;
use crate::browser::BrowserProfile;
use crate::storage::database::FAVORITES_FOLDER;
use crate::storage::{page_data, BrowserDatabase};
use crate::ui::app_menu::AppMenuAction;
use crate::ui::sidebar::{ActionHandler, SidebarAction};

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
                        self.toast("Сессия успешно восстановлена");
                        self.restore_banner.set_revealed(false);
                        self.restore_banner.set_visible(false);
                    } else {
                        self.toast("В сохраненной сессии нет вкладок");
                    }
                } else {
                    self.toast("Нет сохраненной сессии для восстановления");
                }
            }
            AppMenuAction::OpenBookmarks => {
                set_sidebar_visible(&self.split_view, true);
                self.toast("Закладки открыты в боковой панели");
            }
            AppMenuAction::OpenHistory => {
                if let Some(popover) = self.history_popover.borrow().as_ref() {
                    popover.popup();
                }
            }
            AppMenuAction::OpenDownloads => {
                self.header.downloads_button.set_visible(true);
                self.downloads_popover.popup();
            }
            AppMenuAction::OpenReadingList => {
                set_sidebar_visible(&self.split_view, true);
                self.toast("Список для чтения открыт в боковой панели");
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
                    this.toast("Не удалось напечатать страницу");
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
        } else {
            self.window.remove_css_class("dark-mode");
            self.window.add_css_class("light-mode");
            self.downloads_popover.popover().add_css_class("light-mode");
            if let Some(h) = self.history_popover.borrow().as_ref() {
                h.popover().add_css_class("light-mode");
            }
            if let Some(m) = self.app_menu.borrow().as_ref() {
                m.popover.add_css_class("light-mode");
            }
            self.omnibox_popup.popover().add_css_class("light-mode");
            self.shield_popover.popover().add_css_class("light-mode");
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
            self.toast("Светлая тема включена");
        } else {
            self.toast("Тёмная тема включена");
        }
    }

    pub(super) fn close_all_popovers(&self) {
        self.shield_popover.popdown();
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
        if let Some(p) = self.header.burger_button.popover() {
            p.popdown();
        }
        if let Some(p) = self.header.downloads_button.popover() {
            p.popdown();
        }
    }

    pub(super) fn open_settings_page(self: &Rc<Self>) {
        for tab in self.tabs.borrow().iter() {
            let logical = tab.logical_url();
            if logical == SETTINGS_URL || logical == "echo://settings" {
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

    pub(super) fn record_tracker_blocked(&self, domain: &str, rule: &str) {
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
            if logical == START_URL || logical == "echo://start" {
                self.push_start_page_data(&tab.web_view);
            } else if logical == TRACKERS_URL
                || logical == PRIVACY_URL
                || logical == "echo://trackers"
                || logical == "echo://privacy"
            {
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
        );
        if !is_known_key {
            log::warn!("SECURITY ALERT: Rejected unknown setting key: {key}");
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

    pub(super) fn set_profile(&self, profile: BrowserProfile) {
        self.engine.borrow_mut().set_profile(profile);
        let name = match profile {
            BrowserProfile::Work => "Рабочий",
            BrowserProfile::Hardened => "Защищенный",
        };
        self.toast(&format!("Профиль «{name}» будет применён к новым вкладкам"));
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
                    this.toast("Адрес скопирован");
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
            goto_tab,
        ]);

        let accels: [(&str, &[&str]); 25] = [
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
            ("win.bookmark", &["<Primary>d"]),
            ("win.reading-list", &["<Primary><Shift>d"]),
            ("win.find", &["<Primary>f"]),
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
                    self.toast("Группы вкладок отключены в приватном режиме");
                } else {
                    self.prompt_group_name(None);
                }
            }
            SidebarAction::AddBookmark => {
                if self.is_private_mode.get() {
                    self.toast("Закладки не сохраняются в приватном режиме");
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
                    self.toast("Открыта сохраненная офлайн-копия");
                } else {
                    self.open_in_current_tab(&url);
                    self.toast("У этой страницы нет офлайн-копии, загрузка из сети...");
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
                    self.toast("Отмечено как прочитанное");
                } else {
                    self.toast("Отмечено как непрочитанное");
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
                        self.toast("Удалено из списка для чтения");
                    }
                    Ok(None) => {
                        self.toast("Удалено из списка для чтения");
                    }
                    Err(err) => {
                        log::warn!("cannot delete reading item {id}: {err}");
                        self.toast("Не удалось удалить страницу");
                    }
                }
                self.library_changed();
            }
            SidebarAction::DeleteBookmark(id) => {
                match self.db.delete_bookmark(id) {
                    Ok(()) => {
                        self.toast("Закладка удалена");
                    }
                    Err(err) => {
                        log::warn!("cannot delete bookmark {id}: {err}");
                        self.toast("Не удалось удалить закладку");
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
            if logical == START_URL || logical == "echo://start" {
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
            self.toast("Эту страницу нельзя добавить в закладки");
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
            .unwrap_or_else(|| FAVORITES_FOLDER.to_string());

        let title_entry = Entry::builder()
            .text(&title)
            .activates_default(true)
            .build();
        let folder_entry = Entry::builder()
            .text(&folder)
            .placeholder_text("Папка (необязательно)")
            .activates_default(true)
            .build();
        let form = GtkBox::new(Orientation::Vertical, 8);
        form.append(&title_entry);
        form.append(&folder_entry);

        let dialog = adw::AlertDialog::new(
            Some(if existing.is_some() {
                "Изменить закладку"
            } else {
                "Новая закладка"
            }),
            Some(&url),
        );
        dialog.add_response("cancel", "Отмена");
        if existing.is_some() {
            dialog.add_response("remove", "Удалить");
            dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        }
        dialog.add_response("save", "Сохранить");
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
                    let folder = (!folder_text.is_empty()).then_some(folder_text);
                    match this.db.add_bookmark(&name, &url, folder.as_deref()) {
                        Ok(_) => this.toast("Закладка сохранена"),
                        Err(err) => {
                            log::warn!("cannot save bookmark {url}: {err}");
                            this.toast("Не удалось сохранить закладку");
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
            self.toast("Эту страницу нельзя добавить в список для чтения");
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
                        this.toast("Добавлено в список для чтения");
                        this.library_changed();

                        // Asynchronously save MHTML archive for offline reading
                        let mut offline_dir = glib::user_data_dir();
                        offline_dir.push("echo-browser");
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
                                        this.toast("Страница сохранена для автономного чтения");
                                        this.library_changed();
                                    }
                                }
                                Err(err) => {
                                    log::warn!(
                                        "Failed to save MHTML archive {file_path_str}: {err}"
                                    );
                                    if let Some(this) = weak_offline.upgrade() {
                                        this.toast("Не удалось создать офлайн-копию страницы");
                                    }
                                }
                            },
                        );
                    }
                    Err(err) => {
                        log::warn!("cannot add {url} to reading list: {err}");
                        this.toast("Не удалось сохранить страницу");
                    }
                }
            },
        );
    }
}
