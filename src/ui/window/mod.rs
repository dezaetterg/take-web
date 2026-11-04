mod actions;
mod listeners;
mod menu;
mod omnibox;
mod session;
mod tab_controller;
mod translation;

use menu::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::gdk::Display;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, CssProvider, Label, Orientation, Overlay, Paned};
use libadwaita as adw;
use webkit6::WebProcessTerminationReason;

pub(crate) use session::{ClosedTab, TabRecord};

use super::app_menu::AppMenu;
use super::downloads_popover::DownloadsPopover;
use super::find_bar::FindBar;
use super::header_bar::BrowserHeaderBar;
use super::history_popover::HistoryPopover;
use super::omnibox_popup::OmniboxPopup;
use super::permission_bar::PermissionBar;
use super::shield_popover::ShieldPopover;
use super::sidebar::BrowserSidebar;
use super::tab::BrowserTab;
use super::tab_overview_dock::TabOverviewDock;
use super::translate_popover::TranslatePopover;
use crate::browser::session_manager::{SessionManager, SessionState};
use crate::browser::{BrowserEngine, BrowserProfile, DownloadManager, MemoryManager};
use crate::privacy::PrivateSessionGuard;
use crate::storage::{AsyncDb, BrowserDatabase};

pub(super) const START_URL: &str = "take://start";
pub(super) const TRACKERS_URL: &str = "take://trackers";
pub(super) const PRIVACY_URL: &str = "take://privacy";
pub(super) const SETTINGS_URL: &str = "take://settings";

/// Grabs the first paragraph long enough to be article text for the reading list card.
pub(super) const PREVIEW_SCRIPT: &str = r#"
(function () {
    const paragraphs = document.querySelectorAll('article p, main p, p');
    for (const p of paragraphs) {
        const text = (p.innerText || '').trim().replace(/\s+/g, ' ');
        if (text.length > 60) { return text.slice(0, 240); }
    }
    return '';
})()
"#;

pub(crate) struct Inner {
    pub(super) window: adw::ApplicationWindow,
    pub(super) is_test_suite: bool,
    pub(super) is_creating_tab: Cell<bool>,
    pub(super) header: BrowserHeaderBar,
    pub(super) sidebar: BrowserSidebar,
    pub(super) normal_tab_view: adw::TabView,
    pub(super) private_tab_view: adw::TabView,
    pub(super) content_overlay: Overlay,
    pub(super) is_private_mode: Cell<bool>,
    pub(super) session_guard: Rc<PrivateSessionGuard>,
    pub(super) tab_bar: adw::TabBar,
    pub(super) toolbar_view: adw::ToolbarView,
    pub(super) tab_overview: RefCell<adw::TabOverview>,
    pub(super) overview_overlay: Overlay,
    pub(super) overview_dock: Rc<TabOverviewDock>,
    pub(super) split_view: adw::OverlaySplitView,
    pub(super) toast_overlay: adw::ToastOverlay,
    pub(super) status_label: Label,
    pub(super) find_bar: Rc<FindBar>,
    pub(super) permission_bar: Rc<PermissionBar>,
    pub(super) crash_banner: adw::Banner,
    pub(super) restore_banner: adw::Banner,
    pub(super) session_path: Option<std::path::PathBuf>,
    pub(super) last_crashed_session: RefCell<Option<SessionState>>,
    pub(super) download_manager: Rc<DownloadManager>,
    pub(super) downloads_popover: Rc<DownloadsPopover>,
    pub(super) shield_popover: Rc<ShieldPopover>,
    pub(super) translate_popover: Rc<TranslatePopover>,
    pub(super) translate_download_cancellable: RefCell<Option<gio::Cancellable>>,
    pub(super) translator_view: RefCell<Option<webkit6::WebView>>,
    pub(super) translator_host_ready: Cell<bool>,
    pub(super) translator_pending_jobs: RefCell<Vec<String>>,
    pub(super) translator_inflight_requests:
        RefCell<std::collections::HashMap<String, webkit6::WebView>>,
    pub(super) history_popover: RefCell<Option<Rc<HistoryPopover>>>,
    pub(super) app_menu: RefCell<Option<Rc<AppMenu>>>,
    pub(super) omnibox_popup: Rc<OmniboxPopup>,
    pub(super) omnibox_suggestions_enabled: Cell<bool>,
    pub(super) open_tabs_related: Cell<bool>,
    pub(super) sidebar_default_visible: Cell<bool>,
    pub(super) close_window_with_last_tab: Cell<bool>,
    pub(super) suppress_omnibox_search: Cell<bool>,
    pub(super) fullscreen_prev_sidebar: Cell<bool>,
    pub(super) engine: RefCell<BrowserEngine>,
    pub(super) memory_manager: Rc<MemoryManager>,
    pub(super) db: BrowserDatabase,
    pub(super) async_db: AsyncDb,
    pub(super) tabs: RefCell<Vec<Rc<BrowserTab>>>,
    pub(super) closed_tabs: RefCell<Vec<ClosedTab>>,
    pub(super) active_group: RefCell<Option<String>>,
    pub(super) ungrouped_snapshot: RefCell<Vec<TabRecord>>,
    pub(super) context_page: RefCell<Option<adw::TabPage>>,
    pub(super) switching_group: Cell<bool>,
    pub(super) refresh_pending: Cell<bool>,
    pub(super) stats_refresh_pending: Cell<bool>,
    pub(super) theme_dark: Cell<bool>,
    pub(super) is_app_mode: Cell<bool>,
    pub(super) is_split_view: Cell<bool>,
    pub(super) is_split_focused: Cell<bool>,
    pub(super) view_paned: Paned,
    pub(super) split_container: RefCell<Option<GtkBox>>,
    pub(super) split_web_view: RefCell<Option<webkit6::WebView>>,
    pub(super) is_reader_active: Cell<bool>,
    pub(super) mpris_controller: RefCell<Option<Arc<crate::browser::mpris::MprisController>>>,
}

pub struct BrowserWindow {
    pub(crate) inner: Rc<Inner>,
}

thread_local! {
    static MPRIS_ACTIVE_WINDOW: std::cell::RefCell<Option<std::rc::Weak<Inner>>> = const { std::cell::RefCell::new(None) };
}

impl BrowserWindow {
    pub fn is_dangerous_system_dir(p: &std::path::Path) -> bool {
        Inner::is_dangerous_system_dir(p)
    }

    pub fn new(
        app: &adw::Application,
        is_test_suite: bool,
        start_urls: &[String],
        start_private: bool,
    ) -> Self {
        Self::new_internal(app, is_test_suite, start_urls, start_private, false, false)
    }

    pub fn new_empty(app: &adw::Application, is_test_suite: bool, start_private: bool) -> Self {
        Self::new_internal(app, is_test_suite, &[], start_private, true, false)
    }

    pub fn new_app_mode(app: &adw::Application, app_url: &str) -> Self {
        Self::new_internal(app, false, &[app_url.to_string()], false, false, true)
    }

    pub fn tab_view(&self) -> &adw::TabView {
        self.inner.tab_view()
    }

    fn new_internal(
        app: &adw::Application,
        is_test_suite: bool,
        start_urls: &[String],
        start_private: bool,
        is_empty: bool,
        is_app_mode: bool,
    ) -> Self {
        Self::load_styles();

        let (db, db_path) = Self::open_database(is_test_suite);
        let async_db = AsyncDb::open(db_path.as_deref());
        let header = BrowserHeaderBar::new();
        let sidebar = BrowserSidebar::new();

        let normal_tab_view = adw::TabView::new();
        normal_tab_view.set_hexpand(true);
        normal_tab_view.set_vexpand(true);

        let private_tab_view = adw::TabView::new();
        private_tab_view.set_hexpand(true);
        private_tab_view.set_vexpand(true);

        let tab_bar = adw::TabBar::new();
        tab_bar.set_view(Some(&normal_tab_view));
        tab_bar.set_autohide(false);
        tab_bar.set_end_action_widget(Some(&header.new_tab_button));
        tab_bar.add_css_class("take-tab-bar");

        let progress = header.progress_bar.clone();
        progress.set_can_target(false);

        let status_label = Label::new(None);
        status_label.add_css_class("take-status-pill");
        status_label.set_halign(gtk4::Align::Start);
        status_label.set_valign(gtk4::Align::End);
        status_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        status_label.set_max_width_chars(80);
        status_label.set_can_target(false);
        status_label.set_visible(false);

        let find_bar = FindBar::new();
        let permission_bar = PermissionBar::new();
        let crash_banner = adw::Banner::new(crate::i18n::tr("app.crash_banner"));
        crash_banner.set_button_label(Some(crate::i18n::tr("app.crash_reload")));
        crash_banner.set_revealed(false);
        crash_banner.set_visible(false);
        crash_banner.add_css_class("take-crash-banner");

        let restore_banner = adw::Banner::new(crate::i18n::tr("app.restore_banner"));
        restore_banner.set_button_label(Some(crate::i18n::tr("app.restore_tabs")));
        restore_banner.set_revealed(false);
        restore_banner.set_visible(false);
        restore_banner.add_css_class("take-restore-banner");

        let view_paned = Paned::new(Orientation::Horizontal);
        view_paned.set_hexpand(true);
        view_paned.set_vexpand(true);
        view_paned.set_start_child(Some(&normal_tab_view));
        view_paned.set_shrink_start_child(true);
        view_paned.set_resize_start_child(true);
        view_paned.set_shrink_end_child(true);
        view_paned.set_resize_end_child(true);
        view_paned.add_css_class("take-view-paned");

        let content_overlay = Overlay::new();
        content_overlay.set_child(Some(&view_paned));
        content_overlay.add_overlay(&progress);
        content_overlay.add_overlay(&status_label);
        content_overlay.add_overlay(find_bar.widget());
        content_overlay.add_overlay(permission_bar.widget());

        let split_view = adw::OverlaySplitView::new();
        split_view.set_sidebar(Some(&sidebar.container));
        split_view.set_content(Some(&content_overlay));
        split_view.set_min_sidebar_width(220.0);
        split_view.set_max_sidebar_width(320.0);
        split_view.set_sidebar_width_fraction(0.2);
        split_view.set_show_sidebar(true);
        split_view.set_enable_hide_gesture(true);
        // One-way binding (sidebar -> button); the button drives the sidebar through
        // `set_sidebar_visible` so the show/hide skips the width animation.
        split_view
            .bind_property("show-sidebar", &header.sidebar_toggle, "active")
            .sync_create()
            .build();
        {
            let split = split_view.clone();
            let paned_for_sidebar = view_paned.clone();
            header.sidebar_toggle.connect_toggled(move |btn| {
                set_sidebar_visible(&split, btn.is_active());
                if paned_for_sidebar.end_child().is_some() {
                    let p = paned_for_sidebar.clone();
                    glib::idle_add_local_once(move || {
                        let w = p.width();
                        if w > 100 {
                            p.set_position(w / 2);
                        }
                    });
                }
            });
        }

        if is_app_mode {
            tab_bar.set_visible(false);
            header.sidebar_toggle.set_visible(false);
            header.back_button.set_visible(false);
            header.forward_button.set_visible(false);
            header.reload_button.set_visible(false);
            header.address_capsule.set_visible(false);
            header.app_title_label.set_visible(true);
            header.new_tab_button.set_visible(false);
            header.tab_overview_button.set_visible(false);
            header.burger_button.set_visible(false);
            header.downloads_button.set_visible(false);
            split_view.set_show_sidebar(false);
            split_view.set_enable_hide_gesture(false);
        }

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&split_view));

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.set_top_bar_style(adw::ToolbarStyle::Flat);
        toolbar_view.add_top_bar(&header.container);
        if !is_app_mode {
            toolbar_view.add_top_bar(&tab_bar);
        }
        toolbar_view.add_top_bar(&restore_banner);
        toolbar_view.add_top_bar(&crash_banner);
        toolbar_view.set_content(Some(&toast_overlay));

        let tab_overview = adw::TabOverview::new();
        tab_overview.set_view(Some(&normal_tab_view));
        tab_overview.set_enable_search(true);
        tab_overview.set_enable_new_tab(false);
        tab_overview.set_child(Some(&toolbar_view));
        tab_overview.add_css_class("take-tab-overview");

        let overview_dock = Rc::new(TabOverviewDock::new());

        let overview_overlay = Overlay::new();
        overview_overlay.add_css_class("take-overview-overlay");
        overview_overlay.set_child(Some(&tab_overview));
        overview_overlay.add_overlay(&overview_dock.container);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(if is_test_suite {
                "Take Test Suite"
            } else {
                "Take Web"
            })
            .default_width(1280)
            .default_height(840)
            .content(&overview_overlay)
            .icon_name("takeweb")
            .build();
        window.add_css_class("take-browser-window");
        if is_app_mode {
            window.add_css_class("take-app-mode-window");
        }

        let download_manager = DownloadManager::new();
        download_manager.set_parent_window(Some(window.clone().upcast::<gtk4::Window>()));
        let downloads_popover = DownloadsPopover::new(Rc::clone(&download_manager));
        header
            .downloads_button
            .set_popover(Some(downloads_popover.popover()));

        let omnibox_popup = OmniboxPopup::new(&header.address_capsule);

        let shield_popover = ShieldPopover::new();
        header
            .security_button
            .set_popover(Some(shield_popover.popover()));

        let translate_popover = TranslatePopover::new();
        header
            .translate_button
            .set_popover(Some(translate_popover.popover()));

        let engine = BrowserEngine::new(BrowserProfile::Hardened, is_test_suite);
        let dl_mgr = Rc::clone(&download_manager);
        let dl_popover = Rc::clone(&downloads_popover);
        let dl_header = header.clone();
        engine
            .network_manager()
            .session()
            .connect_download_started(move |_session, download| {
                dl_header.downloads_button.set_visible(true);
                dl_mgr.handle_download(download, false);
                dl_popover.popup();
            });

        let dl_mgr_eph = Rc::clone(&download_manager);
        let dl_popover_eph = Rc::clone(&downloads_popover);
        let dl_header_eph = header.clone();
        engine.set_on_ephemeral_session_created(move |session| {
            let mgr = Rc::clone(&dl_mgr_eph);
            let popover = Rc::clone(&dl_popover_eph);
            let hdr = dl_header_eph.clone();
            session.connect_download_started(move |_session, download| {
                hdr.downloads_button.set_visible(true);
                mgr.handle_download(download, true);
                popover.popup();
            });
        });

        let memory_manager = MemoryManager::new();
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        if let Some(ratio) = crate::browser::performance::parse_meminfo_ratio(&meminfo) {
            memory_manager.set_low_memory_mode(crate::browser::performance::low_memory(ratio));
        }

        let session_path = if is_test_suite {
            None
        } else {
            Some(SessionManager::default_session_path())
        };

        let previous_session = session_path
            .as_ref()
            .and_then(|p| SessionManager::load_session(p).ok().flatten());

        let had_crash = previous_session
            .as_ref()
            .map(|s| !s.clean_shutdown && !s.tabs.is_empty())
            .unwrap_or(false);

        let last_crashed_session = if had_crash {
            previous_session.clone()
        } else {
            None
        };

        let session_guard = Rc::new(PrivateSessionGuard::new());

        let inner = Rc::new(Inner {
            window,
            is_test_suite,
            is_creating_tab: Cell::new(false),
            header,
            sidebar,
            normal_tab_view,
            private_tab_view,
            content_overlay,
            is_private_mode: Cell::new(false),
            session_guard: Rc::clone(&session_guard),
            tab_bar: tab_bar.clone(),
            toolbar_view,
            tab_overview: RefCell::new(tab_overview),
            overview_overlay,
            overview_dock,
            split_view,
            toast_overlay,
            status_label,
            find_bar,
            permission_bar,
            crash_banner,
            restore_banner,
            session_path,
            last_crashed_session: RefCell::new(last_crashed_session),
            download_manager,
            downloads_popover,
            shield_popover: Rc::clone(&shield_popover),
            translate_popover: Rc::clone(&translate_popover),
            translate_download_cancellable: RefCell::new(None),
            translator_view: RefCell::new(None),
            translator_host_ready: Cell::new(false),
            translator_pending_jobs: RefCell::new(Vec::new()),
            translator_inflight_requests: RefCell::new(std::collections::HashMap::new()),
            history_popover: RefCell::new(None),
            app_menu: RefCell::new(None),
            omnibox_popup,
            omnibox_suggestions_enabled: Cell::new(true),
            open_tabs_related: Cell::new(true),
            sidebar_default_visible: Cell::new(true),
            close_window_with_last_tab: Cell::new(true),
            suppress_omnibox_search: Cell::new(false),
            fullscreen_prev_sidebar: Cell::new(true),
            engine: RefCell::new(engine),
            memory_manager,
            db,
            async_db,
            tabs: RefCell::new(Vec::new()),
            closed_tabs: RefCell::new(Vec::new()),
            active_group: RefCell::new(None),
            ungrouped_snapshot: RefCell::new(Vec::new()),
            context_page: RefCell::new(None),
            switching_group: Cell::new(false),
            refresh_pending: Cell::new(false),
            stats_refresh_pending: Cell::new(false),
            theme_dark: Cell::new(true),
            is_app_mode: Cell::new(is_app_mode),
            is_split_view: Cell::new(false),
            is_split_focused: Cell::new(false),
            view_paned,
            split_container: RefCell::new(None),
            split_web_view: RefCell::new(None),
            is_reader_active: Cell::new(false),
            mpris_controller: RefCell::new(None),
        });

        // Track focus when clicking left pane in Split View
        for tv in [&inner.normal_tab_view, &inner.private_tab_view] {
            let click_left = gtk4::GestureClick::new();
            click_left.set_propagation_phase(gtk4::PropagationPhase::Capture);
            click_left.set_button(0);
            let weak_left = Rc::downgrade(&inner);
            click_left.connect_pressed(move |_, _, _, _| {
                if let Some(this) = weak_left.upgrade() {
                    if this.is_split_view.get() && this.is_split_focused.get() {
                        this.set_split_pane_focused(false);
                    }
                }
            });
            tv.add_controller(click_left);
        }

        // Initialize MPRIS media playback controller
        let weak_mpris = Rc::downgrade(&inner);
        MPRIS_ACTIVE_WINDOW.with(|w| {
            *w.borrow_mut() = Some(weak_mpris);
        });

        let mpris_controller = Arc::new(crate::browser::mpris::MprisController::new(move |cmd| {
            glib::idle_add_once(move || {
                MPRIS_ACTIVE_WINDOW.with(|w| {
                    if let Some(ref weak) = *w.borrow() {
                        if let Some(this) = weak.upgrade() {
                            this.dispatch_media_command(cmd);
                        }
                    }
                });
            });
        }));
        *inner.mpris_controller.borrow_mut() = Some(mpris_controller);

        // Connect Reader Mode header button
        {
            let weak_reader = Rc::downgrade(&inner);
            inner.header.reader_button.connect_clicked(move |_| {
                if let Some(this) = weak_reader.upgrade() {
                    this.toggle_reader_mode();
                }
            });
        }

        // Connect Shield Popover actions
        listeners::setup_window_listeners(&inner, &tab_bar, app);
        if !is_empty {
            let test_url = std::env::var("TAKEWEB_TEST_URL").ok();
            let target = start_urls.first().cloned().or(test_url);
            let startup_action = inner
                .db
                .get_setting("startup_action")
                .unwrap_or(None)
                .unwrap_or_else(|| "startpage".to_string());

            let initial_url = match (target.as_deref(), is_test_suite) {
                (Some(url), _) => BrowserEngine::resolve_url(url),
                (None, true) => "take://test-suite".to_string(),
                (None, false) => match startup_action.as_str() {
                    "blank" => "about:blank".to_string(),
                    _ => inner.effective_start_url(),
                },
            };
            inner.open_tab(&initial_url, None, true);

            if start_urls.len() > 1 {
                for extra_url in &start_urls[1..] {
                    let resolved = BrowserEngine::resolve_url(extra_url);
                    inner.open_tab(&resolved, None, false);
                }
            }

            if start_private {
                inner.enter_private_browsing();
            }

            inner.refresh_library();
            inner.refresh_tabs_and_groups();

            if target.is_none() && !is_test_suite && startup_action == "restore" {
                let session_path = SessionManager::default_session_path();
                if let Ok(Some(session)) = SessionManager::load_session(&session_path) {
                    inner.restore_session(&session);
                }
            } else if had_crash && target.is_none() && !is_test_suite {
                inner.restore_banner.set_visible(true);
                inner.restore_banner.set_revealed(true);
            }

            inner.save_current_session(false);
        } else {
            if start_private {
                inner.enter_private_browsing();
            }
            inner.refresh_library();
            inner.refresh_tabs_and_groups();
        }

        // Pre-warm thumbnail rendering on first launch so the very first
        // transition to Tab Overview doesn't freeze the compositor.
        {
            let weak_warm = Rc::downgrade(&inner);
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                if let Some(this) = weak_warm.upgrade() {
                    if let Some(page) = this.tab_view().selected_page() {
                        page.invalidate_thumbnail();
                    }
                }
            });
        }

        Self { inner }
    }

    fn load_styles() {
        let provider = CssProvider::new();
        provider.load_from_string(include_str!("../../../resources/css/browser.css"));

        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn open_database(is_test_suite: bool) -> (BrowserDatabase, Option<std::path::PathBuf>) {
        let persistent = if is_test_suite {
            None
        } else {
            let dir = glib::user_data_dir().join("takeweb");
            match std::fs::create_dir_all(&dir) {
                Ok(()) => {
                    let path = dir.join("browser.db");
                    match BrowserDatabase::open(&path) {
                        Ok(db) => Some((db, path)),
                        Err(err) => {
                            log::warn!("browser.db unavailable, using a temporary store: {err}");
                            None
                        }
                    }
                }
                Err(err) => {
                    log::warn!("cannot create {}: {err}", dir.display());
                    None
                }
            }
        };

        let (db, path) = match persistent {
            Some((db, path)) => (db, Some(path)),
            None => (
                BrowserDatabase::in_memory().unwrap_or_else(|err| {
                    log::error!("cannot initialise SQLite: {err}");
                    std::process::exit(1);
                }),
                None,
            ),
        };

        if let Err(err) = db.seed_defaults_once() {
            log::warn!("cannot seed default favorites: {err}");
        }
        (db, path)
    }

    pub fn present(&self) {
        self.inner.window.present();
    }

    pub fn open_tab(&self, url: &str, switch_to: bool) {
        let resolved = BrowserEngine::resolve_url(url);
        self.inner.open_tab(&resolved, None, switch_to);
    }

    pub fn enter_private_mode(&self) {
        self.inner.enter_private_browsing();
    }
}

impl Inner {
    pub(super) fn toast(&self, text: &str) {
        self.toast_overlay.add_toast(adw::Toast::new(text));
    }

    pub(super) fn update_crash_banner(&self, reason: Option<WebProcessTerminationReason>) {
        match reason {
            Some(WebProcessTerminationReason::ExceededMemoryLimit) => {
                self.crash_banner
                    .set_title(crate::i18n::tr("app.crash_oom"));
                self.crash_banner.set_visible(true);
                self.crash_banner.set_revealed(true);
            }
            Some(WebProcessTerminationReason::Crashed) => {
                self.crash_banner
                    .set_title(crate::i18n::tr("app.crash_process"));
                self.crash_banner.set_visible(true);
                self.crash_banner.set_revealed(true);
            }
            Some(WebProcessTerminationReason::TerminatedByApi) | None => {
                self.crash_banner.set_revealed(false);
                self.crash_banner.set_visible(false);
            }
            _ => {
                self.crash_banner
                    .set_title(crate::i18n::tr("app.crash_killed"));
                self.crash_banner.set_visible(true);
                self.crash_banner.set_revealed(true);
            }
        }
    }

    pub(super) fn tab_view(&self) -> &adw::TabView {
        if self.is_private_mode.get() {
            &self.private_tab_view
        } else {
            &self.normal_tab_view
        }
    }

    pub(super) fn current_tab(&self) -> Option<Rc<BrowserTab>> {
        self.tab_view()
            .selected_page()
            .and_then(|page| self.tab_for_page(&page))
    }

    pub(super) fn pages(&self) -> Vec<adw::TabPage> {
        (0..self.tab_view().n_pages())
            .map(|index| self.tab_view().nth_page(index))
            .collect()
    }

    pub(super) fn tab_for_page(&self, page: &adw::TabPage) -> Option<Rc<BrowserTab>> {
        self.tabs
            .borrow()
            .iter()
            .find(|tab| tab.page == *page)
            .cloned()
    }

    pub(super) fn refresh_settings_pages(&self) {
        for tab in self.tabs.borrow().iter() {
            if tab.logical_url().starts_with(SETTINGS_URL) {
                self.push_settings_page_data(&tab.web_view);
            }
        }
    }

    pub(crate) fn is_dangerous_system_dir(p: &std::path::Path) -> bool {
        let dangerous_roots = [
            "/",
            "/bin",
            "/sbin",
            "/usr",
            "/usr/bin",
            "/usr/sbin",
            "/etc",
            "/root",
            "/proc",
            "/sys",
            "/dev",
            "/boot",
            "/lib",
            "/lib64",
            "/var/run",
            "/run",
        ];
        let path_str = p.to_string_lossy();
        dangerous_roots
            .iter()
            .any(|&r| path_str == r || path_str == format!("{r}/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dangerous_system_dir_detection() {
        assert!(BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/")
        ));
        assert!(BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/etc")
        ));
        assert!(BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/bin")
        ));
        assert!(BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/root")
        ));
        assert!(BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/sys")
        ));
        assert!(BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/proc")
        ));
        assert!(BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/var/run")
        ));

        assert!(!BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/home/user/Downloads")
        ));
        assert!(!BrowserWindow::is_dangerous_system_dir(
            std::path::Path::new("/tmp/my_downloads")
        ));
    }
}
