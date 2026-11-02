mod actions;
mod omnibox;
mod session;
mod tab_controller;

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::gdk::{Display, Key};
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{CssProvider, EventControllerKey, Label, Overlay};
use libadwaita as adw;
use webkit6::prelude::*;
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
}

pub struct BrowserWindow {
    pub(crate) inner: Rc<Inner>,
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
        Self::new_internal(app, is_test_suite, start_urls, start_private, false)
    }

    pub fn new_empty(app: &adw::Application, is_test_suite: bool, start_private: bool) -> Self {
        Self::new_internal(app, is_test_suite, &[], start_private, true)
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
        tab_bar.add_css_class("echo-tab-bar");

        let progress = header.progress_bar.clone();
        progress.set_can_target(false);

        let status_label = Label::new(None);
        status_label.add_css_class("echo-status-pill");
        status_label.set_halign(gtk4::Align::Start);
        status_label.set_valign(gtk4::Align::End);
        status_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        status_label.set_max_width_chars(80);
        status_label.set_can_target(false);
        status_label.set_visible(false);

        let find_bar = FindBar::new();
        let permission_bar = PermissionBar::new();
        let crash_banner = adw::Banner::new("Веб-страница завершила работу из-за сбоя рендерера");
        crash_banner.set_button_label(Some("Перезагрузить страницу"));
        crash_banner.set_revealed(false);
        crash_banner.set_visible(false);
        crash_banner.add_css_class("echo-crash-banner");

        let restore_banner = adw::Banner::new("Предыдущая сессия завершилась некорректно");
        restore_banner.set_button_label(Some("Восстановить вкладки"));
        restore_banner.set_revealed(false);
        restore_banner.set_visible(false);
        restore_banner.add_css_class("echo-restore-banner");

        let content_overlay = Overlay::new();
        content_overlay.set_child(Some(&normal_tab_view));
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
            header.sidebar_toggle.connect_toggled(move |btn| {
                set_sidebar_visible(&split, btn.is_active());
            });
        }

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&split_view));

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.set_top_bar_style(adw::ToolbarStyle::Flat);
        toolbar_view.add_top_bar(&header.container);
        toolbar_view.add_top_bar(&tab_bar);
        toolbar_view.add_top_bar(&restore_banner);
        toolbar_view.add_top_bar(&crash_banner);
        toolbar_view.set_content(Some(&toast_overlay));

        let tab_overview = adw::TabOverview::new();
        tab_overview.set_view(Some(&normal_tab_view));
        tab_overview.set_enable_search(true);
        tab_overview.set_enable_new_tab(false);
        tab_overview.set_child(Some(&toolbar_view));

        let overview_dock = Rc::new(TabOverviewDock::new());

        let overview_overlay = Overlay::new();
        overview_overlay.set_child(Some(&tab_overview));
        overview_overlay.add_overlay(&overview_dock.container);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(if is_test_suite {
                "Echo Test Suite"
            } else {
                "Take Web"
            })
            .default_width(1280)
            .default_height(840)
            .content(&overview_overlay)
            .icon_name("takeweb")
            .build();
        window.add_css_class("echo-browser-window");

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
        });

        // Connect Shield Popover actions
        {
            let weak_shield = Rc::downgrade(&inner);
            inner
                .shield_popover
                .set_on_toggle_protection(move |domain, active| {
                    if let Some(this) = weak_shield.upgrade() {
                        let is_exception = !active;
                        let private = this.is_private_mode.get()
                            || this.current_tab().is_some_and(|t| t.is_private);
                        if !private {
                            if let Err(e) = this.db.set_protection_exception(domain, is_exception) {
                                log::warn!("failed to set protection exception: {e}");
                            }
                        }
                        if let Some(tab) = this.current_tab() {
                            if let Some(ucm) = tab.web_view.user_content_manager() {
                                if is_exception {
                                    ucm.remove_filter_by_id("echo_default_filters");
                                } else if let Some(ref filter) =
                                    *this.engine.borrow().content_filter().borrow()
                                {
                                    ucm.add_filter(filter);
                                }
                            }
                            tab.web_view.reload();
                        }
                    }
                });
        }
        {
            let weak_clear = Rc::downgrade(&inner);
            inner.shield_popover.set_on_clear_site_data(move |domain| {
                if let Some(this) = weak_clear.upgrade() {
                    let domain_owned = domain.to_string();
                    if let Some(tab) = this.current_tab() {
                        let js = "try { localStorage.clear(); sessionStorage.clear(); } catch(e){}";
                        tab.web_view.evaluate_javascript(
                            js,
                            None,
                            None,
                            None::<&gio::Cancellable>,
                            |_| {},
                        );

                        let engine = this.engine.borrow();
                        let session = engine.network_manager().session();
                        if let Some(dm) = session.website_data_manager() {
                            let view = tab.web_view.clone();
                            let dm_clone = dm.clone();
                            dm.fetch(
                                webkit6::WebsiteDataTypes::ALL,
                                None::<&gio::Cancellable>,
                                move |res| {
                                    if let Ok(data_list) = res {
                                        let matched: Vec<&webkit6::WebsiteData> = data_list
                                            .iter()
                                            .filter(|d| {
                                                d.name()
                                                    .map(|n| {
                                                        n.contains(&domain_owned)
                                                            || domain_owned.contains(n.as_str())
                                                    })
                                                    .unwrap_or(false)
                                            })
                                            .collect();
                                        if !matched.is_empty() {
                                            let (tx, rx) = tokio::sync::oneshot::channel();
                                            let view_clone = view.clone();
                                            glib::spawn_future_local(async move {
                                                if rx.await.is_ok() {
                                                    view_clone.reload();
                                                }
                                            });
                                            dm_clone.remove(
                                                webkit6::WebsiteDataTypes::ALL,
                                                &matched,
                                                None::<&gio::Cancellable>,
                                                move |_| {
                                                    let _ = tx.send(());
                                                },
                                            );
                                        } else {
                                            view.reload();
                                        }
                                    } else {
                                        view.reload();
                                    }
                                },
                            );
                        } else {
                            tab.web_view.reload();
                        }
                        this.toast(&format!("Данные для {domain} очищены"));
                    }
                }
            });
        }
        {
            let weak_priv = Rc::downgrade(&inner);
            inner.shield_popover.set_on_open_privacy_settings(move || {
                if let Some(this) = weak_priv.upgrade() {
                    this.open_tab(TRACKERS_URL, None, true);
                }
            });
        }

        // Connect restore banner action
        {
            let weak_restore = Rc::downgrade(&inner);
            inner.restore_banner.connect_button_clicked(move |banner| {
                if let Some(this) = weak_restore.upgrade() {
                    banner.set_revealed(false);
                    banner.set_visible(false);
                    if let Some(session) = this.last_crashed_session.borrow().clone() {
                        this.restore_session(&session);
                        this.toast("Сессия успешно восстановлена");
                    }
                }
            });
        }

        // Connect crash banner reload button
        {
            let weak_crash = Rc::downgrade(&inner);
            inner.crash_banner.connect_button_clicked(move |banner| {
                if let Some(this) = weak_crash.upgrade() {
                    banner.set_revealed(false);
                    banner.set_visible(false);
                    if let Some(tab) = this.current_tab() {
                        tab.clear_crashed();
                        log::info!("Reloading crashed tab #{}", tab.id);
                        tab.web_view.reload();
                    }
                }
            });
        }

        // Setup system lock & sleep listener for private session
        {
            let weak_inner = Rc::downgrade(&inner);
            let guard = Rc::clone(&session_guard);
            PrivateSessionGuard::setup_system_lock_listener(move || {
                guard.set_locked(true);
                if let Some(this) = weak_inner.upgrade() {
                    if this.is_private_mode.get() && guard.has_password() {
                        log::info!("System locked or sleeping: auto-exiting private mode");
                        this.exit_private_browsing();
                    }
                }
            });
        }

        // Connect Tab Overview & Floating Dock
        {
            inner.wire_tab_overview_signals(&inner.tab_overview.borrow());

            let weak_normal = Rc::downgrade(&inner);
            inner
                .overview_dock
                .normal_tabs_button
                .connect_clicked(move |_| {
                    if let Some(this) = weak_normal.upgrade() {
                        this.exit_private_browsing();
                    }
                });

            let weak_priv = Rc::downgrade(&inner);
            inner
                .overview_dock
                .private_tabs_button
                .connect_clicked(move |_| {
                    if let Some(this) = weak_priv.upgrade() {
                        this.enter_private_browsing();
                    }
                });
        }

        // Initialize history popover
        let history_popover = {
            let weak = Rc::downgrade(&inner);
            HistoryPopover::new(
                Rc::new({
                    let weak = weak.clone();
                    move |url| {
                        if let Some(this) = weak.upgrade() {
                            this.open_in_current_tab(url);
                        }
                    }
                }),
                Rc::new({
                    let weak = weak.clone();
                    move |url| {
                        if let Some(this) = weak.upgrade() {
                            this.open_tab(url, None, false);
                        }
                    }
                }),
                Rc::new({
                    let weak = weak.clone();
                    move |query| {
                        if let Some(this) = weak.upgrade() {
                            this.load_history_for_popover(query)
                        } else {
                            Vec::new()
                        }
                    }
                }),
                Rc::new({
                    let weak = weak.clone();
                    move || {
                        if let Some(this) = weak.upgrade() {
                            this.clear_history_from_popover();
                        }
                    }
                }),
            )
        };
        history_popover
            .popover()
            .set_parent(&inner.header.burger_button);
        *inner.history_popover.borrow_mut() = Some(history_popover);

        // Initialize app burger menu
        let app_menu = {
            let weak = Rc::downgrade(&inner);
            AppMenu::new(Rc::new(move |action| {
                if let Some(this) = weak.upgrade() {
                    this.handle_app_menu_action(action);
                }
            }))
        };
        inner
            .header
            .burger_button
            .set_popover(Some(&app_menu.popover));
        *inner.app_menu.borrow_mut() = Some(app_menu);

        // Window-level key events (Escape for fullscreen, Alt/Super to dismiss open popovers on switch)
        let win_key_ctrl = EventControllerKey::new();
        win_key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak = Rc::downgrade(&inner);
        win_key_ctrl.connect_key_pressed(move |_, keyval, _, modifiers| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            // Alt-Tab, Super (app switcher) or Alt key down closes floating popovers
            if keyval == Key::Alt_L
                || keyval == Key::Alt_R
                || keyval == Key::Super_L
                || keyval == Key::Super_R
                || (keyval == Key::Tab && modifiers.contains(gtk4::gdk::ModifierType::ALT_MASK))
            {
                this.close_all_popovers();
            }

            if keyval == Key::Escape {
                this.close_all_popovers();
                if this.window.is_fullscreen() {
                    this.leave_fullscreen();
                    if let Some(tab) = this.current_tab() {
                        tab.web_view.evaluate_javascript(
                            "if (document.fullscreenElement) { document.exitFullscreen(); }",
                            None::<&str>,
                            None::<&str>,
                            None::<&gio::Cancellable>,
                            |_| (),
                        );
                    }
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        inner.window.add_controller(win_key_ctrl);

        let weak_active = Rc::downgrade(&inner);
        inner.window.connect_realize(move |win| {
            if let Some(surface) = win.surface() {
                if let Ok(toplevel) = surface.downcast::<gtk4::gdk::Toplevel>() {
                    let was_focused = std::cell::Cell::new(false);
                    let weak = weak_active.clone();
                    toplevel.connect_state_notify(move |tl| {
                        let is_focused = tl.state().contains(gtk4::gdk::ToplevelState::FOCUSED);
                        if was_focused.get() && !is_focused {
                            if let Some(this) = weak.upgrade() {
                                this.close_all_popovers();
                            }
                        }
                        was_focused.set(is_focused);
                    });
                }
            }
        });

        let weak_unmap = Rc::downgrade(&inner);
        inner.window.connect_unmap(move |_| {
            if let Some(this) = weak_unmap.upgrade() {
                this.close_all_popovers();
            }
        });

        inner.connect_tab_view(&tab_bar);
        inner.connect_header();
        inner.connect_sidebar();
        inner.register_tab_actions();
        inner.register_window_actions(app);

        let weak_tracker = Rc::downgrade(&inner);
        inner
            .engine
            .borrow()
            .setup_tracker_block_listener(move |domain, rule| {
                if let Some(this) = weak_tracker.upgrade() {
                    this.record_tracker_blocked(domain, rule);
                    this.privacy_stats_changed();
                }
            });

        let weak_fp = Rc::downgrade(&inner);
        inner
            .engine
            .borrow()
            .setup_fingerprint_listener(move |vector, count| {
                if let Some(this) = weak_fp.upgrade() {
                    this.record_fingerprint_defended(vector, count);
                    this.privacy_stats_changed();
                }
            });

        let weak_settings = Rc::downgrade(&inner);
        inner
            .engine
            .borrow()
            .setup_settings_change_listener(move |key, value| {
                if let Some(this) = weak_settings.upgrade() {
                    this.on_setting_changed(key, value);
                }
            });

        // Load & apply saved theme setting
        let saved_theme = inner
            .db
            .get_setting("theme")
            .unwrap_or(None)
            .unwrap_or_else(|| "dark".to_string());
        inner.apply_theme_setting(&saved_theme);

        let weak_theme = Rc::downgrade(&inner);
        adw::StyleManager::default().connect_dark_notify(move |mgr| {
            if let Some(this) = weak_theme.upgrade() {
                let is_dark = mgr.is_dark();
                let theme_str = if is_dark { "dark" } else { "light" };
                let _ = this.db.set_setting("theme", theme_str);
                this.sync_theme_classes(is_dark);
            }
        });

        // Load and apply persisted browser settings
        if let Ok(Some(v)) = inner.db.get_setting("omnibox_suggestions") {
            inner.omnibox_suggestions_enabled.set(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("open_tabs_related") {
            inner.open_tabs_related.set(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("sidebar_default_visible") {
            let show = v != "false";
            inner.sidebar_default_visible.set(show);
            set_sidebar_visible(&inner.split_view, show);
        }
        if let Ok(Some(v)) = inner.db.get_setting("close_window_with_last_tab") {
            inner.close_window_with_last_tab.set(v != "false");
        }
        if let Ok(Some(dir)) = inner.db.get_setting("download_dir") {
            inner
                .download_manager
                .set_download_dir(Some(std::path::PathBuf::from(dir)));
        }
        if let Ok(Some(ask)) = inner.db.get_setting("ask_download_location") {
            inner.download_manager.set_ask_destination(ask == "true");
        }

        // Apply WebKit engine settings
        {
            let engine = inner.engine.borrow();
            if let Ok(Some(v)) = inner.db.get_setting("hardware_acceleration") {
                engine.set_hardware_acceleration(v != "false");
            }
            if let Ok(Some(v)) = inner.db.get_setting("smooth_scrolling") {
                engine.set_smooth_scrolling(v != "false");
            }
            if let Ok(Some(v)) = inner.db.get_setting("enable_javascript") {
                engine.set_javascript_enabled(v != "false");
            }
            if let Ok(Some(v)) = inner.db.get_setting("block_autoplay") {
                engine.set_block_autoplay(v == "true");
            }
            if let Ok(Some(v)) = inner.db.get_setting("webrtc_protection") {
                engine.set_webrtc_protection(v == "true");
            }
            if let Ok(Some(v)) = inner.db.get_setting("shield_adblock") {
                engine.set_adblock_enabled(v != "false");
            }
            if let Ok(Some(v)) = inner.db.get_setting("shield_fingerprint") {
                engine.set_anti_fingerprint(v != "false");
            }
            if let Ok(Some(v)) = inner.db.get_setting("shield_itp") {
                engine.network_manager().set_itp_enabled(v != "false");
            }
            if let Ok(Some(v)) = inner.db.get_setting("shield_https_only") {
                engine.set_https_only(v != "false");
            }
            if let Ok(Some(v)) = inner.db.get_setting("cookie_policy") {
                engine.network_manager().set_cookie_policy(&v);
            }
        }

        if !is_empty {
            let test_url = std::env::var("ECHO_TEST_URL").ok();
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
                    _ => START_URL.to_string(),
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
            let dir = glib::user_data_dir().join("echo-browser");
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

pub(super) fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|parsed| {
        parsed
            .host_str()
            .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
    })
}

pub(super) fn is_web_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub(super) fn tab_menu(pinned: bool, muted: bool) -> gio::Menu {
    let menu = gio::Menu::new();

    let closing = gio::Menu::new();
    closing.append(Some("Закрыть"), Some("tab.close"));
    closing.append(Some("Закрыть остальные"), Some("tab.close-others"));
    closing.append(Some("Закрыть вкладки справа"), Some("tab.close-right"));
    menu.append_section(None, &closing);

    let managing = gio::Menu::new();
    managing.append(
        Some(if pinned {
            "Открепить"
        } else {
            "Закрепить"
        }),
        Some("tab.pin"),
    );
    managing.append(Some("Дублировать"), Some("tab.duplicate"));
    managing.append(
        Some(if muted {
            "Включить звук"
        } else {
            "Отключить звук"
        }),
        Some("tab.mute"),
    );
    managing.append(Some("Скопировать адрес"), Some("tab.copy-url"));
    menu.append_section(None, &managing);

    menu
}

/// Returns the window state only when `page` is the tab the user is looking at.
pub(super) fn if_selected(
    weak: &Weak<Inner>,
    page: &glib::WeakRef<adw::TabPage>,
) -> Option<Rc<Inner>> {
    let inner = weak.upgrade()?;
    let page = page.upgrade()?;
    (inner.tab_view().selected_page().as_ref() == Some(&page)).then_some(inner)
}

/// Shows or hides the tab sidebar without the 250 ms width animation.
pub(super) fn set_sidebar_visible(split: &adw::OverlaySplitView, show: bool) {
    split.set_collapsed(!show);
}

impl Inner {
    pub(super) fn toast(&self, text: &str) {
        self.toast_overlay.add_toast(adw::Toast::new(text));
    }

    pub(super) fn update_crash_banner(&self, reason: Option<WebProcessTerminationReason>) {
        match reason {
            Some(WebProcessTerminationReason::ExceededMemoryLimit) => {
                self.crash_banner
                    .set_title("Страница была закрыта из-за нехватки оперативной памяти");
                self.crash_banner.set_visible(true);
                self.crash_banner.set_revealed(true);
            }
            Some(WebProcessTerminationReason::Crashed) => {
                self.crash_banner
                    .set_title("Процесс веб-страницы неожиданно завершил работу");
                self.crash_banner.set_visible(true);
                self.crash_banner.set_revealed(true);
            }
            Some(WebProcessTerminationReason::TerminatedByApi) | None => {
                self.crash_banner.set_revealed(false);
                self.crash_banner.set_visible(false);
            }
            _ => {
                self.crash_banner
                    .set_title("Процесс веб-страницы был принудительно остановлен");
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
