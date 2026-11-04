use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::WebView;

/// Duration of inactivity before a background tab is suspended.
pub const INACTIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// Periodic interval for checking tab activity.
pub const POLL_INTERVAL_SECONDS: u32 = 5;

struct TrackedTab {
    view: glib::WeakRef<WebView>,
    last_active: Instant,
    is_background: bool,
    is_suspended: bool,
    was_muted_by_manager: bool,
}

pub struct MemoryManager {
    tabs: Rc<RefCell<HashMap<u64, TrackedTab>>>,
    active_tab: Rc<RefCell<Option<u64>>>,
    timer_source: RefCell<Option<glib::SourceId>>,
    low_memory_mode: std::cell::Cell<bool>,
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
                is_background: !is_active,
                is_suspended: false,
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

            if current_tab.is_suspended {
                Self::resume_tab_internal(current_tab, id);
            }
        }

        *self.active_tab.borrow_mut() = Some(id);
    }

    pub fn set_low_memory_mode(&self, low: bool) {
        self.low_memory_mode.set(low);
        log::info!("MemoryManager: low_memory_mode set to {low}");
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

    /// Checks all background tabs and suspends any inactive for longer than the effective timeout.
    /// Under normal conditions: 30s. Under low memory or when background tab count > 4: 10s.
    pub fn check_inactive_tabs(&self) {
        let mut tabs = self.tabs.borrow_mut();
        let now = Instant::now();

        // Remove dead weak refs if any
        tabs.retain(|_, tab| tab.view.upgrade().is_some());

        let bg_count = tabs.values().filter(|t| t.is_background).count();
        let timeout = self.effective_timeout(bg_count);

        for (id, tab) in tabs.iter_mut() {
            if tab.is_background
                && !tab.is_suspended
                && now.duration_since(tab.last_active) >= timeout
            {
                Self::suspend_tab_internal(tab, *id);
            }
        }
    }

    /// Suspends rendering and mutes audio for the given tab, purging in-RAM cache textures.
    fn suspend_tab_internal(tab: &mut TrackedTab, id: u64) {
        let Some(view) = tab.view.upgrade() else {
            return;
        };

        if !view.is_muted() {
            tab.was_muted_by_manager = true;
            view.set_is_muted(true);
        }

        view.set_visible(false);

        // Clear in-RAM decoded image and font cache for inactive tabs
        if let Some(session) = view.network_session() {
            if let Some(dm) = session.website_data_manager() {
                dm.clear(
                    webkit6::WebsiteDataTypes::MEMORY_CACHE,
                    glib::TimeSpan::from_seconds(0),
                    None::<&gtk4::gio::Cancellable>,
                    |_| {},
                );
            }
        }

        let script = "try{document.querySelectorAll('video,audio').forEach(e=>{if(!e.paused){e.dataset.__mem_paused='1';e.pause();}})}catch(e){};if(window.gc){try{window.gc();}catch(_){}}";
        view.evaluate_javascript(script, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
        tab.is_suspended = true;

        log::info!("MemoryManager: Tab id={id} inactive -> suspended");
    }

    /// Resumes rendering and unmutes the given tab.
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

        let script = "try{document.querySelectorAll('video,audio').forEach(e=>{if(e.dataset.__mem_paused==='1'){delete e.dataset.__mem_paused;e.play().catch(()=>{});}})}catch(e){}";
        view.evaluate_javascript(script, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
        tab.is_suspended = false;

        log::info!("MemoryManager: Tab id={id} resumed");
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
    fn test_inactivity_duration_constant() {
        assert_eq!(INACTIVE_TIMEOUT, Duration::from_secs(30));
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
}
