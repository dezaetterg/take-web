use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Inner, START_URL};
use crate::browser::session_manager::{SavedTab, SessionManager, SessionState};

pub(crate) const MAX_CLOSED_TABS: usize = 25;

/// `(title, url, is_pinned)`, the same shape the storage layer persists.
pub(crate) type TabRecord = (String, String, bool);

pub(crate) struct ClosedTab {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) pinned: bool,
}

impl Inner {
    pub(super) fn save_current_session(&self, clean_shutdown: bool) {
        let Some(path) = self.session_path.as_ref() else {
            return;
        };

        let tabs = self
            .snapshot_open_tabs()
            .into_iter()
            .map(|(title, url, pinned)| SavedTab { title, url, pinned })
            .collect::<Vec<_>>();

        if tabs.is_empty() && !clean_shutdown {
            return;
        }

        let selected_page = self.normal_tab_view.selected_page();
        let selected_index = selected_page.as_ref().and_then(|sel| {
            (0..self.normal_tab_view.n_pages())
                .position(|i| &self.normal_tab_view.nth_page(i) == sel)
        });

        let active_group = self.active_group.borrow().clone();
        let saved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let state = SessionState {
            tabs,
            selected_index,
            active_group,
            clean_shutdown,
            saved_at,
        };

        if let Err(err) = SessionManager::save_session(path, &state) {
            log::warn!("cannot save session to {}: {err}", path.display());
        }
    }

    pub(super) fn restore_session(self: &Rc<Self>, state: &SessionState) {
        if state.tabs.is_empty() {
            return;
        }

        let outgoing: Vec<_> = (0..self.normal_tab_view.n_pages())
            .map(|i| self.normal_tab_view.nth_page(i))
            .collect();
        let mut opened_pages = Vec::new();

        self.switching_group.set(true);
        for saved in &state.tabs {
            let tab = self.open_tab(&saved.url, None, false);
            if !saved.title.is_empty() && saved.url != START_URL {
                tab.page.set_title(&saved.title);
            }
            if saved.pinned {
                self.normal_tab_view.set_page_pinned(&tab.page, true);
            }
            opened_pages.push(tab.page.clone());
        }

        let target_page = state
            .selected_index
            .and_then(|idx| opened_pages.get(idx))
            .or_else(|| opened_pages.first());

        if let Some(page) = target_page {
            self.normal_tab_view.set_selected_page(page);
        }

        for page in &outgoing {
            self.normal_tab_view.close_page(page);
        }
        self.switching_group.set(false);

        *self.active_group.borrow_mut() = state.active_group.clone();
        self.sync_chrome();
        self.refresh_tabs_and_groups();
        self.save_current_session(false);
    }

    pub(super) fn reopen_closed_tab(self: &Rc<Self>) {
        let closed = self.closed_tabs.borrow_mut().pop();
        match closed {
            Some(closed) => {
                let tab = self.open_tab(&closed.url, None, true);
                tab.page.set_title(&closed.title);
                if closed.pinned {
                    self.tab_view().set_page_pinned(&tab.page, true);
                }
            }
            None => self.toast("Нет закрытых вкладок для восстановления"),
        }
    }
}
