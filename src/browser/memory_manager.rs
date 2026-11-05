use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::WebView;

/// Duration of inactivity before a background tab enters Light Suspend.
pub const INACTIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// Duration after entering Light Suspend before a background tab enters Deep Hibernation.
pub const HIBERNATION_TIMEOUT: Duration = Duration::from_secs(120);
/// Accelerated hibernation timeout under low memory or with many background tabs.
pub const FAST_HIBERNATION_TIMEOUT: Duration = Duration::from_secs(30);
/// Periodic interval for checking tab activity and host memory.
pub const POLL_INTERVAL_SECONDS: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabSuspensionState {
    Active,
    LightSuspended,
    Hibernated {
        saved_url: String,
        scroll_x: i64,
        scroll_y: i64,
    },
}

struct TrackedTab {
    view: glib::WeakRef<WebView>,
    last_active: Instant,
    last_suspended: Option<Instant>,
    is_background: bool,
    state: TabSuspensionState,
    was_muted_by_manager: bool,
}

impl TrackedTab {
    pub fn is_suspended(&self) -> bool {
        matches!(
            self.state,
            TabSuspensionState::LightSuspended | TabSuspensionState::Hibernated { .. }
        )
    }

    #[allow(dead_code)]
    pub fn is_hibernated(&self) -> bool {
        matches!(self.state, TabSuspensionState::Hibernated { .. })
    }
}

pub type LowMemoryCallback = Rc<dyn Fn(bool)>;

pub struct MemoryManager {
    tabs: Rc<RefCell<HashMap<u64, TrackedTab>>>,
    active_tab: Rc<RefCell<Option<u64>>>,
    timer_source: RefCell<Option<glib::SourceId>>,
    low_memory_mode: std::cell::Cell<bool>,
    low_mem_callback: RefCell<Option<LowMemoryCallback>>,
}

impl MemoryManager {
    /// Creates and initializes the MemoryManager with an automatic background polling loop.
    pub fn new() -> Rc<Self> {
        let tabs = Rc::new(RefCell::new(HashMap::new()));
        let active_tab = Rc::new(RefCell::new(None));

        let mgr = Rc::new(Self {
            tabs,
            active_tab,
            timer_source: RefCell::new(None),
            low_memory_mode: std::cell::Cell::new(false),
            low_mem_callback: RefCell::new(None),
        });

        mgr.start_timer();
        mgr
    }

    /// Starts the GLib periodic timer on the main loop.
    fn start_timer(self: &Rc<Self>) {
        let weak_mgr = Rc::downgrade(self);
        let source_id = glib::timeout_add_seconds_local(POLL_INTERVAL_SECONDS, move || {
            if let Some(mgr) = weak_mgr.upgrade() {
                mgr.check_inactive_tabs();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *self.timer_source.borrow_mut() = Some(source_id);
    }

    /// Registers a newly opened tab.
    pub fn register_tab(&self, id: u64, view: &WebView, is_active: bool) {
        let mut tabs = self.tabs.borrow_mut();
        tabs.insert(
            id,
            TrackedTab {
                view: view.downgrade(),
                last_active: Instant::now(),
                last_suspended: None,
                is_background: !is_active,
                state: TabSuspensionState::Active,
                was_muted_by_manager: false,
            },
        );

        if is_active {
            *self.active_tab.borrow_mut() = Some(id);
        }

        log::debug!(
            "MemoryManager: Registered tab id={} (active={})",
            id,
            is_active
        );
    }

    /// Unregisters a closed tab.
    pub fn unregister_tab(&self, id: u64) {
        let mut tabs = self.tabs.borrow_mut();
        tabs.remove(&id);
        let mut active = self.active_tab.borrow_mut();
        if *active == Some(id) {
            *active = None;
        }
        log::debug!("MemoryManager: Unregistered tab id={}", id);
    }

    /// Called when the user switches to tab `id`.
    pub fn switch_to_tab(&self, id: u64) {
        let prev_active = *self.active_tab.borrow();
        let now = Instant::now();

        let mut tabs = self.tabs.borrow_mut();

        // 1. Mark previous active tab as background
        if let Some(prev_id) = prev_active {
            if prev_id != id {
                if let Some(prev_tab) = tabs.get_mut(&prev_id) {
                    prev_tab.is_background = true;
                    prev_tab.last_active = now;
                    log::debug!(
                        "MemoryManager: Tab id={} moved to background at {:?}",
                        prev_id,
                        now
                    );
                }
            }
        }

        // 2. Wake up new active tab
        if let Some(current_tab) = tabs.get_mut(&id) {
            current_tab.is_background = false;
            current_tab.last_active = now;

            if current_tab.is_suspended() {
                Self::resume_tab_internal(current_tab, id);
            }
        }

        *self.active_tab.borrow_mut() = Some(id);
    }

    pub fn set_low_memory_mode(&self, low: bool) {
        self.low_memory_mode.set(low);
        if let Some(cb) = self.low_mem_callback.borrow().as_ref() {
            cb(low);
        }
        log::info!("MemoryManager: low_memory_mode set to {low}");
    }

    pub fn set_on_low_memory_mode_changed<F: Fn(bool) + 'static>(&self, f: F) {
        *self.low_mem_callback.borrow_mut() = Some(Rc::new(f));
    }

    #[cfg(test)]
    pub fn is_low_memory_mode(&self) -> bool {
        self.low_memory_mode.get()
    }

    pub fn effective_timeout(&self, bg_count: usize) -> Duration {
        if self.low_memory_mode.get() || bg_count > 4 {
            Duration::from_secs(10)
        } else {
            INACTIVE_TIMEOUT
        }
    }

    pub fn effective_hibernation_timeout(&self, bg_count: usize) -> Duration {
        if self.low_memory_mode.get() || bg_count > 4 {
            FAST_HIBERNATION_TIMEOUT
        } else {
            HIBERNATION_TIMEOUT
        }
    }

    /// Checks all background tabs and advances them through the two-tier lifecycle:
    /// Active -> Light Suspended (after 30s/10s) -> Deep Hibernation (after 120s/30s).
    pub fn check_inactive_tabs(&self) {
        // Dynamic real-time host memory check
        if let Some(ratio) = crate::browser::performance::detect_meminfo_ratio() {
            let is_low = crate::browser::performance::low_memory(ratio);
            if self.low_memory_mode.get() != is_low {
                self.set_low_memory_mode(is_low);
                log::info!(
                    "MemoryManager: Dynamic memory pressure update: low_memory={is_low} (ratio={:.2})",
                    ratio
                );
            }
        }

        let mut tabs = self.tabs.borrow_mut();
        let now = Instant::now();

        // Remove dead weak refs if any
        tabs.retain(|_, tab| tab.view.upgrade().is_some());

        let bg_count = tabs.values().filter(|t| t.is_background).count();
        let light_timeout = self.effective_timeout(bg_count);
        let hibernate_timeout = self.effective_hibernation_timeout(bg_count);

        for (id, tab) in tabs.iter_mut() {
            if !tab.is_background {
                continue;
            }

            match tab.state {
                TabSuspensionState::Active => {
                    if now.duration_since(tab.last_active) >= light_timeout {
                        Self::suspend_tab_internal(tab, *id);
                    }
                }
                TabSuspensionState::LightSuspended => {
                    let suspended_time = tab.last_suspended.unwrap_or(tab.last_active);
                    if now.duration_since(suspended_time) >= hibernate_timeout {
                        Self::hibernate_tab_internal(tab, *id);
                    }
                }
                TabSuspensionState::Hibernated { .. } => {}
            }
        }
    }

    /// Suspends rendering and mutes audio for the given tab, pausing HTML5 media and timers.
    fn suspend_tab_internal(tab: &mut TrackedTab, id: u64) {
        let Some(view) = tab.view.upgrade() else {
            return;
        };

        if view.is_playing_audio() {
            log::debug!("MemoryManager: Tab id={id} is playing audio, sparing from suspension");
            return;
        }

        if !view.is_muted() {
            tab.was_muted_by_manager = true;
            view.set_is_muted(true);
        }

        view.set_visible(false);

        let script = "try{document.querySelectorAll('video,audio').forEach(e=>{if(!e.paused){e.dataset.__mem_paused='1';e.pause();}})}catch(e){};if(window.gc){try{window.gc();}catch(_){}}";
        view.evaluate_javascript(script, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
        tab.state = TabSuspensionState::LightSuspended;
        tab.last_suspended = Some(Instant::now());

        log::info!("MemoryManager: Tab id={id} inactive -> Light Suspended");
    }

    /// Deep Hibernation: unloads the DOM by navigating to about:blank while preserving URL.
    /// Immediately calls glibc malloc_trim to return free memory arenas back to Linux.
    fn hibernate_tab_internal(tab: &mut TrackedTab, id: u64) {
        let Some(view) = tab.view.upgrade() else {
            return;
        };

        let Some(uri) = view.uri() else {
            return;
        };
        let uri_str = uri.to_string();

        // Do not hibernate internal pages (take://), blank tabs, or audio streams
        if uri_str.starts_with("take://") || uri_str == "about:blank" || uri_str.is_empty() {
            return;
        }
        if view.is_playing_audio() {
            return;
        }

        tab.state = TabSuspensionState::Hibernated {
            saved_url: uri_str.clone(),
            scroll_x: 0,
            scroll_y: 0,
        };

        view.load_uri("about:blank");
        crate::browser::performance::trim_process_memory();

        log::info!("MemoryManager: Tab id={id} ({uri_str}) -> Deep Hibernation (DOM unloaded)");
    }

    /// Resumes rendering and restores hibernated content or unfreezes light-suspended state.
    fn resume_tab_internal(tab: &mut TrackedTab, id: u64) {
        let Some(view) = tab.view.upgrade() else {
            return;
        };

        view.set_visible(true);
        view.queue_draw();

        if tab.was_muted_by_manager {
            view.set_is_muted(false);
            tab.was_muted_by_manager = false;
        }

        match &tab.state {
            TabSuspensionState::Hibernated {
                saved_url,
                scroll_x,
                scroll_y,
            } => {
                let url = saved_url.clone();
                let sx = *scroll_x;
                let sy = *scroll_y;
                log::info!("MemoryManager: Restoring hibernated tab id={id} -> reloading {url}");
                view.load_uri(&url);
                if sx > 0 || sy > 0 {
                    let js = format!("window.scrollTo({sx}, {sy});");
                    view.connect_load_changed(move |v, event| {
                        if event == webkit6::LoadEvent::Finished {
                            v.evaluate_javascript(
                                &js,
                                None,
                                None,
                                None::<&gtk4::gio::Cancellable>,
                                |_| {},
                            );
                        }
                    });
                }
            }
            TabSuspensionState::LightSuspended => {
                let script = "try{document.querySelectorAll('video,audio').forEach(e=>{if(e.dataset.__mem_paused==='1'){delete e.dataset.__mem_paused;e.play().catch(()=>{});}})}catch(e){}";
                view.evaluate_javascript(
                    script,
                    None,
                    None,
                    None::<&gtk4::gio::Cancellable>,
                    |_| {},
                );
                log::info!("MemoryManager: Tab id={id} resumed from Light Suspend");
            }
            TabSuspensionState::Active => {}
        }

        tab.state = TabSuspensionState::Active;
        tab.last_suspended = None;
    }

    /// Suspends all background tabs immediately.
    #[allow(dead_code)]
    pub fn suspend_all_background(&self) {
        let mut tabs = self.tabs.borrow_mut();
        for (id, tab) in tabs.iter_mut() {
            if tab.is_background && tab.state == TabSuspensionState::Active {
                Self::suspend_tab_internal(tab, *id);
            }
        }
    }

    /// Resumes all suspended and hibernated tabs.
    #[allow(dead_code)]
    pub fn resume_all(&self) {
        let mut tabs = self.tabs.borrow_mut();
        for (id, tab) in tabs.iter_mut() {
            if tab.is_suspended() {
                Self::resume_tab_internal(tab, *id);
            }
        }
    }

    /// Returns list of tab IDs that are currently suspended or hibernated.
    #[allow(dead_code)]
    pub fn suspended_tab_ids(&self) -> Vec<u64> {
        self.tabs
            .borrow()
            .iter()
            .filter(|(_, tab)| tab.is_suspended())
            .map(|(id, _)| *id)
            .collect()
    }

    #[allow(dead_code)]
    pub fn is_tab_suspended(&self, id: u64) -> bool {
        self.tabs
            .borrow()
            .get(&id)
            .map(|t| t.is_suspended())
            .unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn is_tab_hibernated(&self, id: u64) -> bool {
        self.tabs
            .borrow()
            .get(&id)
            .map(|t| t.is_hibernated())
            .unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn tab_count(&self) -> usize {
        self.tabs.borrow().len()
    }

    #[allow(dead_code)]
    pub fn suspended_count(&self) -> usize {
        self.tabs
            .borrow()
            .values()
            .filter(|t| t.is_suspended())
            .count()
    }

    #[allow(dead_code)]
    pub fn hibernated_count(&self) -> usize {
        self.tabs
            .borrow()
            .values()
            .filter(|t| t.is_hibernated())
            .count()
    }

    #[allow(dead_code)]
    pub fn active_tab_id(&self) -> Option<u64> {
        *self.active_tab.borrow()
    }

    /// Reset auto-mute state if user explicitly mutes/unmutes tab.
    pub fn on_user_muted_toggle(&self, id: u64) {
        let mut tabs = self.tabs.borrow_mut();
        if let Some(tab) = tabs.get_mut(&id) {
            tab.was_muted_by_manager = false;
        }
    }
}

impl Drop for MemoryManager {
    fn drop(&mut self) {
        if let Some(source_id) = self.timer_source.borrow_mut().take() {
            source_id.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inactivity_duration_constants() {
        assert_eq!(INACTIVE_TIMEOUT, Duration::from_secs(30));
        assert_eq!(HIBERNATION_TIMEOUT, Duration::from_secs(120));
        assert_eq!(FAST_HIBERNATION_TIMEOUT, Duration::from_secs(30));
    }

    #[test]
    fn test_effective_timeout() {
        let tabs = Rc::new(RefCell::new(HashMap::new()));
        let active_tab = Rc::new(RefCell::new(None));
        let mgr = MemoryManager {
            tabs,
            active_tab,
            timer_source: RefCell::new(None),
            low_memory_mode: std::cell::Cell::new(false),
            low_mem_callback: RefCell::new(None),
        };

        // Normal mode with <= 4 tabs: 30s
        assert_eq!(mgr.effective_timeout(2), Duration::from_secs(30));
        assert_eq!(mgr.effective_timeout(4), Duration::from_secs(30));

        // Normal mode with > 4 tabs: 10s
        assert_eq!(mgr.effective_timeout(5), Duration::from_secs(10));

        // Low memory mode: always 10s
        mgr.set_low_memory_mode(true);
        assert!(mgr.is_low_memory_mode());
        assert_eq!(mgr.effective_timeout(1), Duration::from_secs(10));
        assert_eq!(mgr.effective_timeout(6), Duration::from_secs(10));
    }

    #[test]
    fn test_effective_hibernation_timeout() {
        let tabs = Rc::new(RefCell::new(HashMap::new()));
        let active_tab = Rc::new(RefCell::new(None));
        let mgr = MemoryManager {
            tabs,
            active_tab,
            timer_source: RefCell::new(None),
            low_memory_mode: std::cell::Cell::new(false),
            low_mem_callback: RefCell::new(None),
        };

        // Normal mode with <= 4 tabs: 120s
        assert_eq!(
            mgr.effective_hibernation_timeout(2),
            Duration::from_secs(120)
        );
        assert_eq!(
            mgr.effective_hibernation_timeout(4),
            Duration::from_secs(120)
        );

        // Normal mode with > 4 tabs: 30s
        assert_eq!(
            mgr.effective_hibernation_timeout(5),
            Duration::from_secs(30)
        );

        // Low memory mode: always 30s
        mgr.set_low_memory_mode(true);
        assert_eq!(
            mgr.effective_hibernation_timeout(1),
            Duration::from_secs(30)
        );
        assert_eq!(
            mgr.effective_hibernation_timeout(6),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn test_tab_suspension_state_predicates() {
        let tab_active = TrackedTab {
            view: glib::WeakRef::new(),
            last_active: Instant::now(),
            last_suspended: None,
            is_background: false,
            state: TabSuspensionState::Active,
            was_muted_by_manager: false,
        };
        assert!(!tab_active.is_suspended());
        assert!(!tab_active.is_hibernated());

        let tab_light = TrackedTab {
            view: glib::WeakRef::new(),
            last_active: Instant::now(),
            last_suspended: Some(Instant::now()),
            is_background: true,
            state: TabSuspensionState::LightSuspended,
            was_muted_by_manager: true,
        };
        assert!(tab_light.is_suspended());
        assert!(!tab_light.is_hibernated());

        let tab_hibernated = TrackedTab {
            view: glib::WeakRef::new(),
            last_active: Instant::now(),
            last_suspended: Some(Instant::now()),
            is_background: true,
            state: TabSuspensionState::Hibernated {
                saved_url: "https://example.com".into(),
                scroll_x: 0,
                scroll_y: 100,
            },
            was_muted_by_manager: true,
        };
        assert!(tab_hibernated.is_suspended());
        assert!(tab_hibernated.is_hibernated());
    }
}
