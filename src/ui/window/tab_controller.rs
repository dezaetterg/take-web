use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{Entry, EventControllerScroll, EventControllerScrollFlags, PopoverMenu};
use libadwaita as adw;
use libadwaita::prelude::*;
use webkit6::prelude::*;
use webkit6::{
    ContextMenuAction, ContextMenuItem, LoadEvent, NavigationPolicyDecision, PolicyDecisionType,
    ResponsePolicyDecision, WebProcessTerminationReason,
};

use super::{
    if_selected, tab_menu, BrowserWindow, Inner, PRIVACY_URL, SETTINGS_URL, START_URL, TRACKERS_URL,
};
use crate::browser::favicon::FaviconManager;
use crate::browser::permissions::{self, PermissionType};
use crate::browser::{BrowserEngine, NetworkManager};
use crate::privacy::url_cleaner::UrlCleaner;
use crate::ui::private_lock_dialog::{show_setup_password_dialog, show_unlock_password_dialog};
use crate::ui::sidebar::{GroupEntry, TabEntry};
use crate::ui::tab::BrowserTab;
use crate::ui::window::session::{ClosedTab, TabRecord, MAX_CLOSED_TABS};

thread_local! {
    pub(super) static DETACHED_TABS: RefCell<Vec<Rc<BrowserTab>>> = const { RefCell::new(Vec::new()) };
}

impl Inner {
    // ---- tab lifecycle -------------------------------------------------

    pub(super) fn open_tab(
        self: &Rc<Self>,
        url: &str,
        parent: Option<&adw::TabPage>,
        select: bool,
    ) -> Rc<BrowserTab> {
        let is_priv = self.is_private_mode.get();
        let target_view = self.tab_view();
        let active_tab = self.current_tab();
        let effective_parent = match parent {
            Some(p) => Some(p),
            None if self.open_tabs_related.get() => active_tab.as_ref().map(|t| &t.page),
            None => None,
        };
        self.is_creating_tab.set(true);
        let tab = Rc::new(if is_priv {
            BrowserTab::new_private(&self.engine.borrow(), target_view, effective_parent)
        } else {
            BrowserTab::new(&self.engine.borrow(), target_view, effective_parent)
        });
        self.is_creating_tab.set(false);
        self.register_tab(&tab);
        self.engine
            .borrow()
            .apply_settings_for_uri(&tab.web_view, url);
        tab.load_url(url);
        if select {
            target_view.set_selected_page(&tab.page);
        }
        tab
    }

    pub(super) fn effective_start_url(&self) -> String {
        if let Ok(Some(custom)) = self.db.get_setting("custom_start_page_url") {
            let trimmed = custom.trim();
            if !trimmed.is_empty() && trimmed != "take://start" {
                return BrowserEngine::resolve_url(trimmed);
            }
        }
        START_URL.to_string()
    }

    pub(super) fn open_user_tab(self: &Rc<Self>) {
        let start = self.effective_start_url();
        self.open_tab(&start, None, true);
        self.header.focus_address();
    }

    pub(super) fn toggle_tab_overview(self: &Rc<Self>) {
        let open = self.tab_overview.borrow().is_open();
        if !open {
            if let Some(page) = self.tab_view().selected_page() {
                page.invalidate_thumbnail();
            }
        }
        self.tab_overview.borrow().set_open(!open);
    }

    pub(super) fn wire_tab_overview_signals(self: &Rc<Self>, overview: &adw::TabOverview) {
        let weak_dock = Rc::downgrade(self);
        overview.connect_open_notify(move |ov| {
            if let Some(this) = weak_dock.upgrade() {
                let open = ov.is_open();
                this.overview_dock.set_visible(open);
            }
        });
    }

    pub(super) fn rebuild_tab_overview(
        self: &Rc<Self>,
        target_view: &adw::TabView,
        was_open: bool,
    ) {
        let old_overview = self.tab_overview.borrow().clone();
        if was_open
            && old_overview
                .view()
                .map(|v| v.n_pages() > 0)
                .unwrap_or(false)
        {
            old_overview.set_open(false);
        }
        old_overview.set_child(None::<&gtk4::Widget>);

        let new_overview = adw::TabOverview::new();
        new_overview.set_view(Some(target_view));
        new_overview.set_enable_search(true);
        new_overview.set_enable_new_tab(false);
        new_overview.set_child(Some(&self.toolbar_view));

        self.wire_tab_overview_signals(&new_overview);
        self.overview_overlay.set_child(Some(&new_overview));
        *self.tab_overview.borrow_mut() = new_overview.clone();

        if was_open && target_view.n_pages() > 0 {
            new_overview.set_open(true);
        }
    }

    pub(super) fn enter_private_browsing(self: &Rc<Self>) {
        if self.is_private_mode.get() {
            return;
        }

        if !self.session_guard.has_prompted() {
            let this_set = Rc::clone(self);
            let this_skip = Rc::clone(self);
            show_setup_password_dialog(
                &self.window,
                move |password| {
                    this_set.session_guard.set_password(&password);
                    this_set.session_guard.set_has_prompted(true);
                    this_set.do_enter_private_browsing();
                },
                move || {
                    this_skip.session_guard.set_password("");
                    this_skip.session_guard.set_has_prompted(true);
                    this_skip.do_enter_private_browsing();
                },
            );
            return;
        }

        if self.session_guard.has_password() && self.session_guard.is_locked() {
            let this_unlock = Rc::clone(self);
            let this_wipe = Rc::clone(self);
            let guard = Rc::clone(&self.session_guard);
            show_unlock_password_dialog(
                &self.window,
                move |password| guard.check_password(password),
                move || {
                    this_unlock.session_guard.set_locked(false);
                    this_unlock.do_enter_private_browsing();
                },
                move || {
                    this_wipe.wipe_private_session();
                },
            );
            return;
        }

        self.do_enter_private_browsing();
    }

    pub(super) fn do_enter_private_browsing(self: &Rc<Self>) {
        if self.is_private_mode.get() {
            return;
        }
        let was_open = self.tab_overview.borrow().is_open();
        self.is_private_mode.set(true);
        self.tab_bar.set_view(Some(&self.private_tab_view));
        self.view_paned
            .set_start_child(Some(&self.private_tab_view));
        self.rebuild_tab_overview(&self.private_tab_view, was_open);

        if self.private_tab_view.n_pages() == 0 {
            self.open_tab(START_URL, None, true);
        }

        self.window.add_css_class("take-private-mode");
        self.header.container.add_css_class("take-private-mode");
        self.overview_dock.set_private_mode(true);
        self.sidebar.set_anonymous_mode(true);

        self.sync_chrome();
        self.refresh_tabs_and_groups();
        self.toast(crate::i18n::tr("app.private_mode_notice"));
    }

    pub(super) fn exit_private_browsing(self: &Rc<Self>) {
        if !self.is_private_mode.get() {
            return;
        }
        self.session_guard.set_locked(true);
        let was_open = self.tab_overview.borrow().is_open();
        self.is_private_mode.set(false);

        // Ensure normal_tab_view has at least 1 tab so the browser never crashes on an empty view
        if self.normal_tab_view.n_pages() == 0 {
            self.open_tab(START_URL, None, true);
        }

        self.tab_bar.set_view(Some(&self.normal_tab_view));
        self.view_paned.set_start_child(Some(&self.normal_tab_view));
        self.rebuild_tab_overview(&self.normal_tab_view, was_open);

        self.window.remove_css_class("take-private-mode");
        self.header.container.remove_css_class("take-private-mode");
        self.overview_dock.set_private_mode(false);
        self.sidebar.set_anonymous_mode(false);

        self.sync_chrome();
        self.refresh_tabs_and_groups();
    }

    pub(super) fn wipe_private_session(self: &Rc<Self>) {
        self.session_guard.reset();
        if self.is_private_mode.get() {
            self.exit_private_browsing();
        } else {
            self.overview_dock.set_private_mode(false);
        }

        self.switching_group.set(true);
        while self.private_tab_view.n_pages() > 0 {
            let page = self.private_tab_view.nth_page(0);
            if let Some(tab) = self.tab_for_page(&page) {
                self.memory_manager.unregister_tab(tab.id);
            }
            self.tabs.borrow_mut().retain(|tab| tab.page != page);
            self.private_tab_view.close_page(&page);
        }
        self.switching_group.set(false);

        self.engine.borrow().wipe_ephemeral_network();

        self.toast(crate::i18n::tr("app.private_session_ended"));
    }

    pub(super) fn register_tab(self: &Rc<Self>, tab: &Rc<BrowserTab>) {
        if self.tabs.borrow().iter().any(|t| t.page == tab.page) {
            return;
        }
        let is_selected = self.tab_view().selected_page().as_ref() == Some(&tab.page);
        self.memory_manager
            .register_tab(tab.id, &tab.web_view, is_selected);
        self.tabs.borrow_mut().push(Rc::clone(tab));
        self.attach_tab_signals(tab);

        let weak = Rc::downgrade(self);
        tab.page.connect_notify_local(None, move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.schedule_refresh();
            }
        });
        self.schedule_refresh();
    }

    pub(super) fn attach_tab_signals(self: &Rc<Self>, tab: &Rc<BrowserTab>) {
        let page_weak = tab.page.downgrade();
        let view = &tab.web_view;

        let chrome_update = {
            let weak = Rc::downgrade(self);
            let page_weak = page_weak.clone();
            move || {
                if let Some(this) = if_selected(&weak, &page_weak) {
                    this.sync_chrome();
                }
            }
        };

        let update = chrome_update.clone();
        view.connect_uri_notify(move |_| update());
        let update = chrome_update.clone();
        view.connect_title_notify(move |_| update());
        let update = chrome_update;
        view.connect_is_loading_notify(move |_| update());

        let weak_app_title = Rc::downgrade(self);
        view.connect_title_notify(move |v| {
            if let Some(this) = weak_app_title.upgrade() {
                if this.is_app_mode.get() {
                    let title = v
                        .title()
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "Take Web".to_string());
                    this.window.set_title(Some(&title));
                    this.header.app_title_label.set_label(&title);
                }
            }
        });

        let weak_audio = Rc::downgrade(self);
        view.connect_is_playing_audio_notify(move |v| {
            if let Some(this) = weak_audio.upgrade() {
                if let Some(ref mpris) = *this.mpris_controller.borrow() {
                    let is_playing = v.is_playing_audio();
                    let title = v.title().map(|t| t.to_string()).unwrap_or_default();
                    let url = v.uri().map(|u| u.to_string()).unwrap_or_default();
                    let status = if is_playing {
                        mpris_server::PlaybackStatus::Playing
                    } else {
                        mpris_server::PlaybackStatus::Paused
                    };
                    mpris.update_playback(status, &title, "", &url);
                }
            }
        });

        let weak_progress = Rc::downgrade(self);
        let page_progress = page_weak.clone();
        view.connect_estimated_load_progress_notify(move |v| {
            if let Some(this) = if_selected(&weak_progress, &page_progress) {
                this.header.set_progress(v.estimated_load_progress());
            }
        });

        let weak = Rc::downgrade(self);
        let tab_load = Rc::clone(tab);
        let page_load = page_weak.clone();
        view.connect_load_changed(move |view, event| {
            if event == LoadEvent::Started {
                tab_load.clear_crashed();
                tab_load.blocked_trackers.borrow_mut().clear();
                if let Some(this) = weak.upgrade() {
                    let uri_str = view.uri().map(|u| u.to_string()).unwrap_or_default();
                    let effective = if uri_str.is_empty() {
                        tab_load.logical_url()
                    } else {
                        uri_str
                    };
                    this.engine.borrow().apply_settings_for_uri(view, &effective);
                    this.refresh_shield_popover();
                }
                *tab_load.detected_lang.borrow_mut() = None;
                *tab_load.auto_translated_for_uri.borrow_mut() = None;
                if let Some(this) = if_selected(&weak, &page_load) {
                    this.update_crash_banner(None);
                    this.permission_bar.dismiss_all();
                    this.translate_popover.reset_for_navigation();
                    this.refresh_shield_popover();
                }
            }

            if event == LoadEvent::Committed {
                if let Some(this) = weak.upgrade() {
                    let uri_str = view.uri().map(|u| u.to_string()).unwrap_or_default();
                    let effective = if uri_str.is_empty() {
                        tab_load.logical_url()
                    } else {
                        uri_str
                    };
                    this.engine.borrow().apply_settings_for_uri(view, &effective);
                }
            }

            if event == LoadEvent::Committed || event == LoadEvent::Finished {
                if let Some(this) = weak.upgrade() {
                    let theme_str = if this.is_dark() { "dark" } else { "light" };
                    let js = format!("if (document.documentElement) document.documentElement.setAttribute('data-theme', '{theme_str}');");
                    view.evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |_| {});
                }
            }

            if event != LoadEvent::Finished {
                return;
            }
            if let Some(this) = weak.upgrade() {
                if let Some(raw_uri) = view.uri() {
                    let logical = BrowserEngine::logical_url(&raw_uri);
                    let domain = FaviconManager::extract_domain(&logical);
                    if !domain.is_empty() && !BrowserEngine::is_internal_uri(&logical) {
                        if let Ok(Some(saved_zoom)) = this.db.get_domain_zoom(&domain) {
                            view.set_zoom_level(saved_zoom);
                            if let Some(curr) = this.current_tab() {
                                if curr.id == tab_load.id {
                                    if let Some(menu) = this.app_menu.borrow().as_ref() {
                                        menu.set_zoom_text(&format!(
                                            "{}%",
                                            (saved_zoom * 100.0).round() as i32
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    if logical == START_URL {
                        this.push_start_page_data(view);
                    } else if logical == TRACKERS_URL || logical == PRIVACY_URL {
                        this.push_trackers_page_data(view);
                    } else if logical == SETTINGS_URL || logical.starts_with("take://settings") {
                        this.push_settings_page_data(view);
                    } else if logical.starts_with("http://") || logical.starts_with("https://") {
                        let title = view
                            .title()
                            .map(|t| t.to_string())
                            .filter(|t| !t.trim().is_empty())
                            .unwrap_or_else(|| logical.clone());
                        log::info!("Page load finished: {} (title: {:?})", logical, title);
                        if !tab_load.is_private {
                            this.record_history_visit(&title, &logical);
                        }
                        this.trigger_auto_translate_if_needed(&tab_load, &logical);
                        tab_load.web_view.evaluate_javascript(
                            "if (window.__TakeTranslator && window.__TakeTranslator.detectPageLanguage) { window.__TakeTranslator.detectPageLanguage(); }",
                            None,
                            None,
                            None::<&gio::Cancellable>,
                            |_| {},
                        );
                    }
                }
            }
        });

        let weak_crash = Rc::downgrade(self);
        let tab_crash = Rc::clone(tab);
        let page_crash = page_weak.clone();
        view.connect_web_process_terminated(move |_view, reason| {
            log::warn!(
                "WebProcess for tab #{} terminated with reason: {:?}",
                tab_crash.id,
                reason
            );
            match reason {
                WebProcessTerminationReason::Crashed
                | WebProcessTerminationReason::ExceededMemoryLimit => {
                    tab_crash.mark_crashed(reason);
                    if let Some(this) = if_selected(&weak_crash, &page_crash) {
                        this.update_crash_banner(Some(reason));
                    }
                }
                WebProcessTerminationReason::TerminatedByApi => {
                    tab_crash.clear_crashed();
                    if let Some(this) = if_selected(&weak_crash, &page_crash) {
                        this.update_crash_banner(None);
                    }
                }
                _ => {
                    tab_crash.mark_crashed(reason);
                    if let Some(this) = if_selected(&weak_crash, &page_crash) {
                        this.update_crash_banner(Some(reason));
                    }
                }
            }
        });

        let weak = Rc::downgrade(self);
        let hover_page = page_weak.clone();
        view.connect_mouse_target_changed(move |_, hit, _| {
            if let Some(this) = if_selected(&weak, &hover_page) {
                match hit.link_uri() {
                    Some(link) => {
                        let text = link.as_str();
                        if this.status_label.text().as_str() != text {
                            this.status_label.set_text(text);
                        }
                        if !this.status_label.is_visible() {
                            this.status_label.set_visible(true);
                        }
                    }
                    None => {
                        if this.status_label.is_visible() {
                            this.status_label.set_visible(false);
                        }
                    }
                }
            }
        });

        let weak = Rc::downgrade(self);
        let opener_page = page_weak.clone();
        view.connect_create(move |opener, nav_action| {
            let this = weak.upgrade()?;
            let parent = opener_page.upgrade();
            let is_priv = this.is_private_mode.get();
            let target_view = this.tab_view();
            let web_view = if is_priv {
                this.engine
                    .borrow()
                    .create_private_web_view_related_to(Some(opener))
            } else {
                this.engine
                    .borrow()
                    .create_web_view_related_to(Some(opener))
            };
            this.is_creating_tab.set(true);
            let tab = Rc::new(if is_priv {
                BrowserTab::from_private_web_view(web_view.clone(), target_view, parent.as_ref())
            } else {
                BrowserTab::from_web_view(web_view.clone(), target_view, parent.as_ref())
            });
            this.is_creating_tab.set(false);
            this.register_tab(&tab);
            let select = nav_action.mouse_button() != 2;
            if select {
                target_view.set_selected_page(&tab.page);
            }
            Some(web_view.upcast())
        });

        let weak = Rc::downgrade(self);
        view.connect_close(move |_| {
            if let (Some(this), Some(page)) = (weak.upgrade(), page_weak.upgrade()) {
                this.tab_view().close_page(&page);
            }
        });

        let weak_policy = Rc::downgrade(self);
        let tab_policy = Rc::clone(tab);
        view.connect_decide_policy(move |view, decision, decision_type| {
            match decision_type {
                PolicyDecisionType::Response => {
                    if let Some(resp_decision) = decision.downcast_ref::<ResponsePolicyDecision>() {
                        if !resp_decision.is_mime_type_supported() {
                            log::info!("DecidePolicy: unsupported response MIME type, downloading");
                            decision.download();
                            return true;
                        }
                    }
                    false
                }
                PolicyDecisionType::NavigationAction => {
                    if let Some(nav_decision) = decision.downcast_ref::<NavigationPolicyDecision>() {
                        let is_middle_click = nav_decision
                            .navigation_action()
                            .map(|a| a.mouse_button() == 2)
                            .unwrap_or(false);
                        if let Some(req) = nav_decision.navigation_action().and_then(|a| a.request()) {
                            if let Some(uri) = req.uri() {
                                if is_middle_click {
                                    if let Some(this) = weak_policy.upgrade() {
                                        this.open_tab(&uri, Some(&tab_policy.page), false);
                                    }
                                    decision.ignore();
                                    return true;
                                }
                                if let Some(this) = weak_policy.upgrade() {
                                    this.engine.borrow().apply_settings_for_uri(view, &uri);
                                }

                                let current_view_uri = view.uri().map(|u| u.to_string()).unwrap_or_default();
                                let is_from_internal_page = current_view_uri.starts_with("take://");

                                // 0. Block remote web pages from navigating to local file:// system
                                if uri.starts_with("file://") && !current_view_uri.is_empty() && !current_view_uri.starts_with("file://") {
                                    log::warn!("SECURITY ALERT: Blocked untrusted cross-origin navigation to file://: {uri} from {current_view_uri}");
                                    decision.ignore();
                                    return true;
                                }

                                // 1. Restrict internal actions to internal origins only
                                if uri.starts_with("take://action/") {
                                    if !is_from_internal_page {
                                        log::warn!("SECURITY ALERT: Blocked unauthorized action request from non-internal origin ({current_view_uri}): {uri}");
                                        decision.ignore();
                                        return true;
                                    }
                                    let action_path = uri.trim_start_matches("take://action/");
                                    if action_path == "clear-trackers" {
                                        if let Some(this) = weak_policy.upgrade() {
                                            this.clear_tracker_stats();
                                            this.refresh_privacy_pages();
                                        }
                                        decision.ignore();
                                        return true;
                                    }
                                    if action_path.starts_with("set-privacy") {
                                        if let Some(this) = weak_policy.upgrade() {
                                            if let Ok(parsed) = url::Url::parse(&uri) {
                                                for (k, v) in parsed.query_pairs() {
                                                    if k == "level" {
                                                        let _ = this.db.set_setting("privacy_level", &v);
                                                    } else if k == "config" {
                                                        let _ = this.db.set_setting("privacy_config", &v);
                                                    }
                                                }
                                                this.refresh_privacy_pages();
                                            }
                                        }
                                        decision.ignore();
                                        return true;
                                    }
                                    decision.ignore();
                                    return true;
                                }

                                // 2. Block web pages from navigating to internal scheme via link clicks or form submissions
                                if uri.starts_with("take://") {
                                    let nav_type = nav_decision.navigation_action().map(|a| a.navigation_type());
                                    let is_external_link = !is_from_internal_page
                                        && !current_view_uri.is_empty()
                                        && (nav_type == Some(webkit6::NavigationType::LinkClicked)
                                            || nav_type == Some(webkit6::NavigationType::FormSubmitted)
                                            || nav_type == Some(webkit6::NavigationType::FormResubmitted));
                                    if is_external_link {
                                        log::warn!("SECURITY ALERT: Blocked untrusted external navigation to internal URI: {uri} from {current_view_uri}");
                                        decision.ignore();
                                        return true;
                                    }
                                }

                                if uri == "take://trackers" || uri == "take://privacy" || uri == "take://start" || uri == "take://settings" {
                                    if let Some(this) = weak_policy.upgrade() {
                                        if let Some(tab) = this.current_tab() {
                                            this.engine.borrow().apply_settings_for_uri(&tab.web_view, &uri);
                                            tab.load_url(&uri);
                                            decision.ignore();
                                            return true;
                                        }
                                    }
                                }
                                let effective_uri = NetworkManager::enforce_https_upgrade(&uri)
                                    .unwrap_or_else(|| uri.to_string());

                                if let Some(this) = weak_policy.upgrade() {
                                    let is_hardened = this.engine.borrow().current_profile().enables_url_tracking_protection();
                                    let cleaned_uri = if is_hardened {
                                        let cleaned = UrlCleaner::clean_url(&effective_uri);
                                        if cleaned.is_some() {
                                            if let Ok(parsed_u) = url::Url::parse(&effective_uri) {
                                                if let Some(h) = parsed_u.host_str() {
                                                    if !tab_policy.is_private {
                                                        this.record_tracker_blocked(h, "URL Tracking Protection");
                                                        this.refresh_privacy_pages();
                                                    }
                                                }
                                            }
                                        }
                                        cleaned
                                    } else {
                                        None
                                    };

                                    let final_target = cleaned_uri.unwrap_or(effective_uri);
                                    if final_target != uri.as_str() {
                                        log::info!("Navigation policy rewrite: {uri} -> {final_target}");
                                        decision.ignore();
                                        view.load_uri(&final_target);
                                        return true;
                                    }
                                } else if effective_uri != uri.as_str() {
                                    log::info!("HTTPS-Only: upgrading in-flight navigation from {uri} to {effective_uri}");
                                    decision.ignore();
                                    view.load_uri(&effective_uri);
                                    return true;
                                }
                            }
                        }
                    }
                    false
                }
                _ => false,
            }
        });

        let weak = Rc::downgrade(self);
        view.connect_enter_fullscreen(move |_| {
            if let Some(this) = weak.upgrade() {
                this.enter_fullscreen();
            }
            false
        });

        let weak = Rc::downgrade(self);
        view.connect_leave_fullscreen(move |_| {
            if let Some(this) = weak.upgrade() {
                this.leave_fullscreen();
            }
            false
        });

        let weak_perm = Rc::downgrade(self);
        let tab_is_private = tab.is_private;
        view.connect_permission_request(move |view, request| {
            let Some(this) = weak_perm.upgrade() else {
                request.deny();
                return true;
            };

            let (perm_type, extra_details) = permissions::inspect_request(request);
            if perm_type == PermissionType::Unknown {
                log::warn!("Unhandled WebKit permission request type from view; strictly denying");
                request.deny();
                return true;
            }

            let uri = view.uri().map(|u| u.to_string()).unwrap_or_default();
            let origin = permissions::extract_origin_from_uri(&uri);
            let perm_key = perm_type.as_str();

            // 1. Check if a decision was already persisted in SQLite for this origin and type
            if let Ok(Some(saved_state)) = this.db.get_site_permission(&origin, perm_key) {
                if saved_state == "allow" {
                    log::info!("Permission auto-allowed from DB: {origin} -> {perm_key}");
                    request.allow();
                    return true;
                } else if saved_state == "deny" {
                    log::info!("Permission auto-denied from DB: {origin} -> {perm_key}");
                    request.deny();
                    return true;
                }
            }

            // 2. If no saved decision, prompt the user with the interactive PermissionBar
            let req_clone = request.clone();
            let weak_inner = Rc::downgrade(&this);
            let origin_clone = origin.clone();

            let info = permissions::PermissionRequestInfo {
                origin,
                permission_type: perm_type,
                description: perm_type.description().to_string(),
                icon_name: perm_type.icon_name(),
                extra_details,
            };

            this.permission_bar.prompt(info, move |allowed, remember| {
                if allowed {
                    log::info!("Permission granted by user: {origin_clone} -> {perm_key} (remember: {remember})");
                    req_clone.allow();
                } else {
                    log::info!("Permission denied by user: {origin_clone} -> {perm_key} (remember: {remember})");
                    req_clone.deny();
                }

                if remember && !tab_is_private {
                    if let Some(inner) = weak_inner.upgrade() {
                        if !inner.is_private_mode.get() {
                            let state = if allowed { "allow" } else { "deny" };
                            let _ = inner.db.set_site_permission(&origin_clone, perm_key, state);
                            if inner.async_db.is_enabled() {
                                inner.async_db.set_site_permission(&origin_clone, perm_key, state);
                            }
                        }
                    }
                }
            });

            true
        });

        let weak_cm = Rc::downgrade(self);
        view.connect_context_menu(move |view, menu, hit_test_result| {
            let Some(this) = weak_cm.upgrade() else {
                return false;
            };

            if hit_test_result.context_is_link() {
                if let Some(link_uri) = hit_test_result.link_uri() {
                    let link_str = link_uri.to_string();

                    let mut items_to_remove = Vec::new();
                    for item in &menu.items() {
                        let action = item.stock_action();
                        if action == ContextMenuAction::OpenLinkInNewWindow
                            || action == ContextMenuAction::CopyLinkToClipboard
                        {
                            items_to_remove.push(item.clone());
                        }
                    }
                    for item in items_to_remove {
                        menu.remove(&item);
                    }

                    let weak_act = Rc::downgrade(&this);
                    let link_for_tab = link_str.clone();
                    let new_tab_action = gio::SimpleAction::new("open-in-new-tab", None);
                    new_tab_action.connect_activate(move |_, _| {
                        if let Some(w) = weak_act.upgrade() {
                            w.open_tab(&link_for_tab, None, true);
                        }
                    });
                    let new_tab_item = ContextMenuItem::from_gaction(
                        &new_tab_action,
                        crate::i18n::tr("tab.open_in_new_tab"),
                        None,
                    );
                    menu.prepend(&new_tab_item);

                    let weak_copy = Rc::downgrade(&this);
                    let link_for_copy = link_str;
                    let copy_link_action = gio::SimpleAction::new("copy-link-address", None);
                    copy_link_action.connect_activate(move |_, _| {
                        if let Some(w) = weak_copy.upgrade() {
                            w.window.clipboard().set_text(&link_for_copy);
                            w.toast(crate::i18n::tr("app.link_copied"));
                        }
                    });
                    let copy_item = ContextMenuItem::from_gaction(
                        &copy_link_action,
                        crate::i18n::tr("tab.copy_link_address"),
                        None,
                    );
                    menu.insert(&copy_item, 1);
                }
            }

            if let Some(raw_uri) = view.uri() {
                let uri_str = raw_uri.to_string();
                if !uri_str.is_empty()
                    && (uri_str.starts_with("http://")
                        || uri_str.starts_with("https://")
                        || uri_str.starts_with("file://"))
                {
                    menu.append(&ContextMenuItem::new_separator());

                    let weak_src = Rc::downgrade(&this);
                    let view_source_url = format!("view-source:{uri_str}");
                    let view_source_action = gio::SimpleAction::new("view-page-source", None);
                    view_source_action.connect_activate(move |_, _| {
                        if let Some(w) = weak_src.upgrade() {
                            w.open_tab(&view_source_url, None, true);
                        }
                    });
                    let src_item = ContextMenuItem::from_gaction(
                        &view_source_action,
                        crate::i18n::tr("tab.open_source"),
                        None,
                    );
                    menu.append(&src_item);
                }
            }

            false
        });
    }

    pub(super) fn handle_close_page(self: &Rc<Self>, page: &adw::TabPage) {
        let switching = self.switching_group.get();
        let is_private = self
            .tab_for_page(page)
            .map(|t| t.is_private)
            .unwrap_or(false)
            || (0..self.private_tab_view.n_pages())
                .any(|i| self.private_tab_view.nth_page(i) == *page);

        if is_private
            && !switching
            && self.is_private_mode.get()
            && self.private_tab_view.n_pages() <= 1
        {
            if let Some(tab) = self.tab_for_page(page) {
                self.memory_manager.unregister_tab(tab.id);
            }
            self.tabs.borrow_mut().retain(|tab| tab.page != *page);
            self.engine.borrow().wipe_ephemeral_network();
            self.download_manager.clear_private_downloads();

            // Smoothly exit private mode FIRST so overview and tab bar reparent to normal_tab_view
            self.exit_private_browsing();
            self.private_tab_view.close_page_finish(page, true);
            self.toast(crate::i18n::tr("app.all_private_tabs_closed"));
            return;
        }

        // Closing the last tab closes the window, like Safari and Epiphany do, unless disabled in settings.
        if !is_private
            && !switching
            && self.normal_tab_view.n_pages() <= 1
            && !self.is_private_mode.get()
        {
            if self.close_window_with_last_tab.get() {
                self.normal_tab_view.close_page_finish(page, false);
                self.window.close();
                return;
            } else {
                if let Some(tab) = self.tab_for_page(page) {
                    tab.load_url(START_URL);
                    self.header.focus_address();
                }
                self.normal_tab_view.close_page_finish(page, false);
                return;
            }
        }

        if !switching && !is_private {
            if let Some(tab) = self.tab_for_page(page) {
                let url = tab.logical_url();
                if !url.is_empty() {
                    let mut closed = self.closed_tabs.borrow_mut();
                    closed.push(ClosedTab {
                        title: page.title().to_string(),
                        url,
                        pinned: page.is_pinned(),
                    });
                    if closed.len() > MAX_CLOSED_TABS {
                        closed.remove(0);
                    }
                }
            }
        }

        if let Some(tab) = self.tab_for_page(page) {
            self.memory_manager.unregister_tab(tab.id);
        }
        self.tabs.borrow_mut().retain(|tab| tab.page != *page);
        if is_private {
            self.private_tab_view.close_page_finish(page, true);
        } else {
            self.normal_tab_view.close_page_finish(page, true);
        }
    }

    pub(super) fn select_relative(&self, step: i32) {
        let count = self.tab_view().n_pages();
        if count < 2 {
            return;
        }
        if let Some(selected) = self.tab_view().selected_page() {
            let current = self.tab_view().page_position(&selected);
            let next = (current + step).rem_euclid(count);
            self.tab_view()
                .set_selected_page(&self.tab_view().nth_page(next));
        }
    }

    pub(super) fn select_number(&self, number: i32) {
        let count = self.tab_view().n_pages();
        let index = if number >= 9 { count - 1 } else { number - 1 };
        if index >= 0 && index < count {
            self.tab_view()
                .set_selected_page(&self.tab_view().nth_page(index));
        }
    }

    pub(super) fn connect_tab_view_signals(self: &Rc<Self>, tab_view: &adw::TabView) {
        let weak = Rc::downgrade(self);
        tab_view.connect_close_page(move |_, page| {
            if let Some(this) = weak.upgrade() {
                this.handle_close_page(page);
                this.schedule_refresh();
            }
            glib::Propagation::Stop
        });

        let weak = Rc::downgrade(self);
        tab_view.connect_page_detached(move |view, page, _| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let is_priv = view == &this.private_tab_view;
            let was_tracked =
                if let Some(pos) = this.tabs.borrow().iter().position(|t| t.page == *page) {
                    let tab = this.tabs.borrow_mut().remove(pos);
                    this.memory_manager.unregister_tab(tab.id);
                    DETACHED_TABS.with(|dt| dt.borrow_mut().push(tab));
                    true
                } else {
                    false
                };

            if was_tracked {
                if !is_priv && this.normal_tab_view.n_pages() == 0 && !this.is_private_mode.get() {
                    if this.close_window_with_last_tab.get() {
                        this.window.close();
                    } else {
                        this.open_tab(START_URL, None, true);
                    }
                } else if is_priv
                    && this.private_tab_view.n_pages() == 0
                    && this.is_private_mode.get()
                {
                    this.exit_private_browsing();
                }
                this.schedule_refresh();
            }
        });

        let weak = Rc::downgrade(self);
        tab_view.connect_page_attached(move |view, page, _| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            if this.is_creating_tab.get() {
                return;
            }
            let is_priv = view == &this.private_tab_view;
            let stashed = DETACHED_TABS.with(|dt| {
                let mut list = dt.borrow_mut();
                list.iter()
                    .position(|t| t.page == *page)
                    .map(|pos| list.remove(pos))
            });
            if let Some(tab) = stashed {
                this.register_tab(&tab);
            } else if this.tab_for_page(page).is_none() {
                if let Ok(web_view) = page.child().downcast::<webkit6::WebView>() {
                    let tab = Rc::new(BrowserTab::from_transferred(
                        web_view,
                        page.clone(),
                        is_priv,
                    ));
                    this.register_tab(&tab);
                }
            }
            this.schedule_refresh();
        });

        let weak = Rc::downgrade(self);
        tab_view.connect_create_window(move |view| {
            let this = weak.upgrade()?;
            let is_priv = view == &this.private_tab_view;
            let app = this
                .window
                .application()
                .and_downcast::<adw::Application>()?;
            let new_win = BrowserWindow::new_empty(&app, this.is_test_suite, is_priv);
            new_win.present();
            Some(new_win.tab_view().clone())
        });

        let weak = Rc::downgrade(self);
        tab_view.connect_selected_page_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                if let Some(tab) = this.current_tab() {
                    this.update_crash_banner(tab.crashed_reason());
                    this.memory_manager.switch_to_tab(tab.id);
                    if let Some(menu) = this.app_menu.borrow().as_ref() {
                        let zoom = tab.web_view.zoom_level();
                        menu.set_zoom_text(&format!("{}%", (zoom * 100.0).round() as i32));
                    }
                    if this.find_bar.is_visible() {
                        this.find_bar.update_view(Some(&tab.web_view));
                    }
                } else {
                    this.update_crash_banner(None);
                    if this.find_bar.is_visible() {
                        this.find_bar.update_view(None);
                    }
                }
                this.permission_bar.dismiss_all();
                this.sync_chrome();
                this.schedule_refresh();
            }
        });

        let weak = Rc::downgrade(self);
        tab_view.connect_page_reordered(move |_, _, _| {
            if let Some(this) = weak.upgrade() {
                this.schedule_refresh();
            }
        });

        let weak = Rc::downgrade(self);
        tab_view.connect_indicator_activated(move |_, page| {
            if let Some(this) = weak.upgrade() {
                if let Some(tab) = this.tab_for_page(page) {
                    this.memory_manager.on_user_muted_toggle(tab.id);
                    tab.toggle_mute();
                }
            }
        });

        let weak = Rc::downgrade(self);
        tab_view.connect_setup_menu(move |view, page| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            *this.context_page.borrow_mut() = page.cloned();
            if let Some(page) = page {
                let muted = this
                    .tab_for_page(page)
                    .map(|tab| tab.web_view.is_muted())
                    .unwrap_or(false);
                view.set_menu_model(Some(&tab_menu(page.is_pinned(), muted)));
            }
        });
    }

    pub(super) fn connect_tab_view(self: &Rc<Self>, tab_bar: &adw::TabBar) {
        self.connect_tab_view_signals(&self.normal_tab_view);
        self.connect_tab_view_signals(&self.private_tab_view);

        let scroll = EventControllerScroll::new(EventControllerScrollFlags::BOTH_AXES);
        let weak = Rc::downgrade(self);
        scroll.connect_scroll(move |_, dx, dy| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let step = if dy > 0.0 || dx > 0.0 {
                1
            } else if dy < 0.0 || dx < 0.0 {
                -1
            } else {
                0
            };
            if step != 0 {
                this.select_relative(step);
            }
            glib::Propagation::Stop
        });
        tab_bar.add_controller(scroll);

        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            if let Some(this) = weak.upgrade() {
                this.persist_active_group();
                this.save_current_session(true);
            }
            glib::Propagation::Proceed
        });
    }

    pub(super) fn schedule_refresh(self: &Rc<Self>) {
        if self.refresh_pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(this) = weak.upgrade() {
                this.refresh_pending.set(false);
                this.refresh_tabs_and_groups();
                this.persist_active_group();
                this.save_current_session(false);
            }
        });
    }

    pub(super) fn refresh_tabs_and_groups(&self) {
        let selected = self.tab_view().selected_page();
        let entries: Vec<TabEntry> = self
            .pages()
            .into_iter()
            .map(|page| {
                let url = self
                    .tab_for_page(&page)
                    .map(|t| t.logical_url())
                    .unwrap_or_default();
                TabEntry {
                    selected: selected.as_ref() == Some(&page),
                    page,
                    url,
                }
            })
            .collect();
        self.sidebar.render_tabs(&entries);

        if self.is_private_mode.get() {
            self.sidebar.render_groups(&[], None);
            self.sidebar.render_reading_list(&[]);
            self.sidebar.render_bookmarks(&[]);
            return;
        }

        let active = self.active_group.borrow().clone();
        let groups = match self.db.get_tab_groups() {
            Ok(groups) => groups,
            Err(err) => {
                log::warn!("cannot load tab groups: {err}");
                Vec::new()
            }
        };
        let group_entries: Vec<GroupEntry> = groups
            .into_iter()
            .map(|group| {
                let tab_count = if active.as_deref() == Some(group.id.as_str()) {
                    self.normal_tab_view.n_pages() as usize
                } else {
                    self.db
                        .get_group_tabs(&group.id)
                        .map(|t| t.len())
                        .unwrap_or(0)
                };
                GroupEntry {
                    id: group.id,
                    name: group.name,
                    tab_count,
                }
            })
            .collect();
        self.sidebar
            .render_groups(&group_entries, active.as_deref());
    }

    // ---- tab groups ------------------------------------------------------------

    pub(super) fn snapshot_open_tabs(&self) -> Vec<TabRecord> {
        (0..self.normal_tab_view.n_pages())
            .map(|i| self.normal_tab_view.nth_page(i))
            .filter_map(|page| {
                let tab = self.tab_for_page(&page)?;
                if tab.is_private {
                    return None;
                }
                let url = tab.logical_url();
                let url = if url.is_empty() {
                    START_URL.to_string()
                } else {
                    url
                };
                Some((page.title().to_string(), url, page.is_pinned()))
            })
            .collect()
    }

    pub(super) fn persist_active_group(&self) {
        if self.switching_group.get() {
            return;
        }
        let active = self.active_group.borrow().clone();
        if let Some(group_id) = active {
            if let Err(err) = self
                .db
                .save_group_tabs(&group_id, &self.snapshot_open_tabs())
            {
                log::warn!("cannot save tabs of group {group_id}: {err}");
            }
        }
    }

    pub(super) fn switch_group(self: &Rc<Self>, target: Option<String>) {
        if *self.active_group.borrow() == target {
            return;
        }

        let snapshot = self.snapshot_open_tabs();
        let current = self.active_group.borrow().clone();
        match &current {
            Some(group_id) => {
                if let Err(err) = self.db.save_group_tabs(group_id, &snapshot) {
                    log::warn!("cannot save tabs of group {group_id}: {err}");
                }
            }
            None => *self.ungrouped_snapshot.borrow_mut() = snapshot,
        }

        let incoming: Vec<TabRecord> = match &target {
            Some(group_id) => match self.db.get_group_tabs(group_id) {
                Ok(entries) => entries
                    .into_iter()
                    .map(|e| (e.title, e.url, e.is_pinned))
                    .collect(),
                Err(err) => {
                    log::warn!("cannot load tabs of group {group_id}: {err}");
                    Vec::new()
                }
            },
            None => self.ungrouped_snapshot.borrow().clone(),
        };

        // New pages open before the old ones close, so the strip is never empty.
        self.switching_group.set(true);
        let outgoing: Vec<_> = (0..self.normal_tab_view.n_pages())
            .map(|i| self.normal_tab_view.nth_page(i))
            .collect();
        let mut first_new: Option<adw::TabPage> = None;
        for (title, url, pinned) in &incoming {
            let tab = self.open_tab(url, None, false);
            tab.page.set_title(title);
            if *pinned {
                self.normal_tab_view.set_page_pinned(&tab.page, true);
            }
            first_new.get_or_insert_with(|| tab.page.clone());
        }
        let first_new =
            first_new.unwrap_or_else(|| self.open_tab(START_URL, None, false).page.clone());
        self.normal_tab_view.set_selected_page(&first_new);
        for page in &outgoing {
            self.normal_tab_view.close_page(page);
        }
        self.switching_group.set(false);

        *self.active_group.borrow_mut() = target;
        self.sync_chrome();
        self.refresh_tabs_and_groups();
    }

    pub(super) fn new_group_id() -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("grp-{millis:x}")
    }

    pub(super) fn prompt_group_name(self: &Rc<Self>, rename: Option<String>) {
        let current_name = rename.as_ref().and_then(|id| {
            self.db
                .get_tab_groups()
                .ok()
                .and_then(|groups| groups.into_iter().find(|g| &g.id == id))
                .map(|g| g.name)
        });

        let entry = Entry::builder()
            .placeholder_text(crate::i18n::tr("tab_groups.group_name_placeholder"))
            .text(current_name.as_deref().unwrap_or(""))
            .activates_default(true)
            .build();

        let (heading, confirm) = if rename.is_some() {
            (
                crate::i18n::tr("tab_groups.rename_group"),
                crate::i18n::tr("tab_groups.rename"),
            )
        } else {
            (
                crate::i18n::tr("tab_groups.new_group"),
                crate::i18n::tr("tab_groups.create"),
            )
        };
        let dialog = adw::AlertDialog::new(Some(heading), None);
        dialog.add_response("cancel", crate::i18n::tr("private_lock.cancel"));
        dialog.add_response("confirm", confirm);
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("confirm"));
        dialog.set_close_response("cancel");
        dialog.set_extra_child(Some(&entry));

        let weak = Rc::downgrade(self);
        let name_entry = entry.clone();
        dialog.connect_response(None, move |_, response| {
            if response != "confirm" {
                return;
            }
            let name = name_entry.text().trim().to_string();
            let Some(this) = weak.upgrade() else {
                return;
            };
            if name.is_empty() {
                return;
            }
            match &rename {
                Some(id) => {
                    if let Err(err) = this.db.rename_tab_group(id, &name) {
                        log::warn!("cannot rename group {id}: {err}");
                    }
                    this.refresh_tabs_and_groups();
                }
                None => {
                    let id = Self::new_group_id();
                    match this.db.create_tab_group(&id, &name, None, None) {
                        Ok(()) => this.switch_group(Some(id)),
                        Err(err) => {
                            log::warn!("cannot create group {name}: {err}");
                            this.toast(crate::i18n::tr("tab_groups.create_failed"));
                        }
                    }
                }
            }
        });
        dialog.present(Some(&self.window));
        entry.grab_focus();
    }

    pub(super) fn delete_group(self: &Rc<Self>, id: &str) {
        // Open tabs are kept: deleting the active group just makes them ungrouped.
        if self.active_group.borrow().as_deref() == Some(id) {
            *self.active_group.borrow_mut() = None;
        }
        if let Err(err) = self.db.delete_tab_group(id) {
            log::warn!("cannot delete group {id}: {err}");
            self.toast(crate::i18n::tr("tab_groups.delete_failed"));
        }
        self.refresh_tabs_and_groups();
    }

    pub(super) fn close_group_tabs(self: &Rc<Self>, id: &str) {
        if self.active_group.borrow().as_deref() == Some(id) {
            self.switching_group.set(true);
            let outgoing = self.pages();
            let fresh = self.open_tab(START_URL, None, true);
            for page in outgoing.iter().filter(|page| **page != fresh.page) {
                self.normal_tab_view.close_page(page);
            }
            self.switching_group.set(false);
        } else if let Err(err) = self.db.save_group_tabs(id, &[]) {
            log::warn!("cannot clear tabs of group {id}: {err}");
        }
        self.refresh_tabs_and_groups();
    }

    pub(super) fn popup_tab_menu(&self, page: &adw::TabPage, rect: &gtk4::gdk::Rectangle) {
        *self.context_page.borrow_mut() = Some(page.clone());
        let muted = self
            .tab_for_page(page)
            .map(|tab| tab.web_view.is_muted())
            .unwrap_or(false);

        let popover = PopoverMenu::from_model(Some(&tab_menu(page.is_pinned(), muted)));
        popover.set_autohide(true);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(rect));
        popover.set_parent(&self.sidebar.container);

        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let pop_weak = popover.downgrade();
        key_ctrl.connect_key_pressed(move |_, keyval, _, modifiers| {
            if keyval == gtk4::gdk::Key::Alt_L
                || keyval == gtk4::gdk::Key::Alt_R
                || keyval == gtk4::gdk::Key::Super_L
                || keyval == gtk4::gdk::Key::Super_R
                || (keyval == gtk4::gdk::Key::Tab
                    && modifiers.contains(gtk4::gdk::ModifierType::ALT_MASK))
            {
                if let Some(p) = pop_weak.upgrade() {
                    p.popdown();
                }
            }
            glib::Propagation::Proceed
        });
        popover.add_controller(key_ctrl);
        popover.connect_closed(|popover| {
            let popover = popover.clone();
            glib::idle_add_local_once(move || {
                if popover.parent().is_some() {
                    popover.unparent();
                }
            });
        });
        popover.popup();
    }
}
