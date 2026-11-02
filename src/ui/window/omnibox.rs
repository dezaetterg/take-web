use std::rc::Rc;

use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{Entry, EventControllerFocus, EventControllerKey};
use webkit6::prelude::*;

use super::{Inner, PRIVACY_URL, SETTINGS_URL, START_URL, TRACKERS_URL};
use crate::browser::favicon::FaviconManager;
use crate::browser::BrowserEngine;
use crate::ui::tab::BrowserTab;

impl Inner {
    pub(super) fn connect_header(self: &Rc<Self>) {
        let header = &self.header;

        // Omnibox popup click-to-navigate
        let weak = Rc::downgrade(self);
        self.omnibox_popup.set_on_select(move |url| {
            if let Some(this) = weak.upgrade() {
                this.suppress_omnibox_search.set(true);
                this.header.address_entry.set_text(&url);
                this.suppress_omnibox_search.set(false);
                this.open_in_current_tab(&url);
                if let Some(tab) = this.current_tab() {
                    tab.web_view.grab_focus();
                }
            }
        });

        // Search Omnibox FTS on typing in address entry
        let weak = Rc::downgrade(self);
        header.address_entry.connect_changed(move |entry| {
            if let Some(this) = weak.upgrade() {
                if this.suppress_omnibox_search.get() {
                    return;
                }
                if !this.omnibox_suggestions_enabled.get() {
                    this.omnibox_popup.close();
                    return;
                }
                if !this.header.is_editing() {
                    return;
                }
                let text = entry.text();
                let query = text.trim();
                let effective_query = if query == START_URL || query == "echo://start" {
                    ""
                } else {
                    query
                };
                if effective_query.is_empty() {
                    this.omnibox_popup.close();
                    return;
                }
                match this.db.search_omnibox(effective_query, 5) {
                    Ok(results) => {
                        this.omnibox_popup.show_results(results);
                    }
                    Err(err) => {
                        log::warn!("Omnibox search error: {err}");
                        this.omnibox_popup.close();
                    }
                }
            }
        });

        // Close Omnibox popup when focus leaves address entry
        let focus_ctrl = EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus_ctrl.connect_leave(move |_| {
            if let Some(this) = weak.upgrade() {
                let weak_popup = Rc::downgrade(&this.omnibox_popup);
                glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
                    if let Some(popup) = weak_popup.upgrade() {
                        popup.close();
                    }
                });
            }
        });
        header.address_entry.add_controller(focus_ctrl);

        // Keyboard navigation: Down/Up to move focus in popup, Enter to load, Escape to close
        let key_ctrl = EventControllerKey::new();
        let weak = Rc::downgrade(self);
        key_ctrl.connect_key_pressed(move |_ctrl, keyval, _keycode, _state| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match keyval {
                Key::Down if this.omnibox_popup.is_visible() => {
                    this.omnibox_popup.select_next();
                    glib::Propagation::Stop
                }
                Key::Up if this.omnibox_popup.is_visible() => {
                    this.omnibox_popup.select_prev();
                    glib::Propagation::Stop
                }
                Key::Escape if this.omnibox_popup.is_visible() => {
                    this.omnibox_popup.close();
                    glib::Propagation::Stop
                }
                Key::Return | Key::KP_Enter if this.omnibox_popup.is_visible() => {
                    if let Some(url) = this.omnibox_popup.selected_url() {
                        this.omnibox_popup.close();
                        this.suppress_omnibox_search.set(true);
                        this.header.address_entry.set_text(&url);
                        this.suppress_omnibox_search.set(false);
                        this.open_in_current_tab(&url);
                        if let Some(tab) = this.current_tab() {
                            tab.web_view.grab_focus();
                        }
                        glib::Propagation::Stop
                    } else {
                        this.omnibox_popup.close();
                        glib::Propagation::Proceed
                    }
                }
                _ => glib::Propagation::Proceed,
            }
        });
        header.address_entry.add_controller(key_ctrl);

        let weak = Rc::downgrade(self);
        header.address_entry.connect_activate(move |entry| {
            if let Some(this) = weak.upgrade() {
                this.navigate_from_entry(entry);
            }
        });

        let weak = Rc::downgrade(self);
        header.new_tab_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.open_user_tab();
            }
        });

        let weak = Rc::downgrade(self);
        header.back_button.connect_clicked(move |_| {
            if let Some(tab) = weak.upgrade().and_then(|this| this.current_tab()) {
                tab.web_view.go_back();
            }
        });

        let weak = Rc::downgrade(self);
        header.forward_button.connect_clicked(move |_| {
            if let Some(tab) = weak.upgrade().and_then(|this| this.current_tab()) {
                tab.web_view.go_forward();
            }
        });

        let weak = Rc::downgrade(self);
        header.reload_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.toggle_reload();
            }
        });

        let weak = Rc::downgrade(self);
        header.bookmark_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.show_bookmark_dialog();
            }
        });

        let weak = Rc::downgrade(self);
        header.reading_list_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.add_current_to_reading_list();
            }
        });

        let weak = Rc::downgrade(self);
        header.tab_overview_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.toggle_tab_overview();
            }
        });
    }

    pub(super) fn sync_chrome(&self) {
        let Some(tab) = self.current_tab() else {
            return;
        };
        let view = &tab.web_view;
        let url = tab.logical_url();

        self.omnibox_popup.close();
        self.suppress_omnibox_search.set(true);
        self.header.set_address(&url);
        self.suppress_omnibox_search.set(false);
        self.header.back_button.set_sensitive(view.can_go_back());
        self.header
            .forward_button
            .set_sensitive(view.can_go_forward());
        self.header.set_loading(view.is_loading());
        self.header.set_progress(view.estimated_load_progress());
        self.header.set_bookmarked(self.is_bookmarked(&url));

        let title = if url == START_URL || url == "echo://start" {
            "Take Web".to_string()
        } else if url == "take://test-suite" || url == "echo://test-suite" {
            "Echo Test Suite".to_string()
        } else if url == TRACKERS_URL
            || url == PRIVACY_URL
            || url == "echo://trackers"
            || url == "echo://privacy"
        {
            "Отчет о защите — Take Web".to_string()
        } else if url == SETTINGS_URL
            || url == "echo://settings"
            || url.starts_with("take://settings")
            || url.starts_with("echo://settings")
        {
            "Настройки — Take Web".to_string()
        } else {
            match view.title().filter(|t| !t.is_empty()) {
                Some(title) => format!("{title} — Take Web"),
                None => "Take Web".to_string(),
            }
        };
        self.window.set_title(Some(&title));

        let domain = url::Url::parse(&url)
            .ok()
            .and_then(|u| {
                u.host_str()
                    .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
            })
            .unwrap_or_default();
        let is_exception = if !domain.is_empty() {
            self.db.is_protection_exception(&domain).unwrap_or(false)
        } else {
            false
        };
        let is_protected = !is_exception;
        let tracker_count = if !domain.is_empty() {
            self.db
                .get_all_tracker_stats()
                .map(|stats| {
                    stats
                        .iter()
                        .find(|s| s.domain == domain)
                        .map(|s| s.blocked_count as usize)
                        .unwrap_or(0)
                })
                .unwrap_or(0)
        } else {
            0
        };
        self.shield_popover
            .update_state(&url, is_protected, tracker_count);
    }

    pub(super) fn current_search_engine(&self) -> String {
        self.db
            .get_setting("default_search_engine")
            .ok()
            .flatten()
            .unwrap_or_else(|| "duckduckgo".to_string())
    }

    pub(super) fn navigate_from_entry(&self, entry: &Entry) {
        self.omnibox_popup.close();
        if let Some(tab) = self.current_tab() {
            let engine = self.current_search_engine();
            let target = BrowserEngine::resolve_url_with_engine(&entry.text(), &engine);
            self.engine
                .borrow()
                .apply_settings_for_uri(&tab.web_view, &target);
            tab.load_url(&target);
            tab.web_view.grab_focus();
        }
    }

    pub(super) fn toggle_reload(&self) {
        if let Some(tab) = self.current_tab() {
            if tab.web_view.is_loading() {
                tab.web_view.stop_loading();
            } else {
                tab.web_view.reload();
            }
        }
    }

    pub(super) fn open_in_current_tab(self: &Rc<Self>, url: &str) {
        match self.current_tab() {
            Some(tab) => {
                self.engine
                    .borrow()
                    .apply_settings_for_uri(&tab.web_view, url);
                tab.load_url(url);
            }
            None => {
                self.open_tab(url, None, true);
            }
        }
    }

    pub(super) fn set_zoom_for_tab(&self, tab: &Rc<BrowserTab>, zoom: f64) {
        tab.web_view.set_zoom_level(zoom);
        if let Some(menu) = self.app_menu.borrow().as_ref() {
            menu.set_zoom_text(&format!("{}%", (zoom * 100.0).round() as i32));
        }
        let url = tab.logical_url();
        let domain = FaviconManager::extract_domain(&url);
        if !domain.is_empty()
            && !BrowserEngine::is_internal_uri(&url)
            && !tab.is_private
            && !self.is_private_mode.get()
        {
            if let Err(err) = self.db.set_domain_zoom(&domain, zoom) {
                log::warn!("Failed to persist domain zoom for {domain}: {err}");
            }
        }
    }

    pub(super) fn zoom_in(self: &Rc<Self>) {
        if let Some(tab) = self.current_tab() {
            let current = tab.web_view.zoom_level();
            let next = (current + 0.1).min(3.0);
            self.set_zoom_for_tab(&tab, next);
        }
    }

    pub(super) fn zoom_out(self: &Rc<Self>) {
        if let Some(tab) = self.current_tab() {
            let current = tab.web_view.zoom_level();
            let next = (current - 0.1).max(0.3);
            self.set_zoom_for_tab(&tab, next);
        }
    }

    pub(super) fn zoom_reset(self: &Rc<Self>) {
        if let Some(tab) = self.current_tab() {
            self.set_zoom_for_tab(&tab, 1.0);
        }
    }

    pub(super) fn load_history_for_popover(
        &self,
        query: &str,
    ) -> Vec<crate::storage::models::HistoryEntry> {
        match self.db.search_history(query, 50) {
            Ok(entries) => entries,
            Err(err) => {
                log::warn!("failed to search history: {err}");
                Vec::new()
            }
        }
    }

    pub(super) fn clear_history_from_popover(&self) {
        if let Err(err) = self.db.clear_history() {
            log::warn!("failed to clear history: {err}");
        } else {
            self.toast("История просмотров очищена");
        }
    }
}
