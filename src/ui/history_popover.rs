use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, GestureClick, Image, Label, Orientation, Popover, ScrolledWindow,
};

use crate::storage::models::HistoryEntry;

pub type HistoryNavigateCallback = Rc<dyn Fn(&str)>;
pub type HistoryLoadCallback = Rc<dyn Fn(&str) -> Vec<HistoryEntry>>;
pub type HistoryClearCallback = Rc<dyn Fn()>;

pub struct HistoryPopover {
    popover: Popover,
    list_box: GtkBox,
    search_entry: Entry,
    on_navigate: HistoryNavigateCallback,
    on_navigate_new_tab: HistoryNavigateCallback,
    on_load: HistoryLoadCallback,
    on_clear: HistoryClearCallback,
}

impl HistoryPopover {
    pub fn new(
        on_navigate: HistoryNavigateCallback,
        on_navigate_new_tab: HistoryNavigateCallback,
        on_load: HistoryLoadCallback,
        on_clear: HistoryClearCallback,
    ) -> Rc<Self> {
        let popover = Popover::new();
        popover.set_autohide(true);
        popover.add_css_class("echo-history-popover");

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

        let root = GtkBox::new(Orientation::Vertical, 8);
        root.set_size_request(380, -1);
        root.set_margin_top(10);
        root.set_margin_bottom(10);
        root.set_margin_start(10);
        root.set_margin_end(10);

        // Header: Title and "Очистить" button
        let header = GtkBox::new(Orientation::Horizontal, 8);
        header.set_hexpand(true);

        let title = Label::builder()
            .label("История")
            .css_classes(["echo-history-popover-title"])
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        header.append(&title);

        let clear_btn = Button::builder()
            .label("Очистить")
            .icon_name("user-trash-symbolic")
            .tooltip_text("Очистить историю посещений")
            .css_classes(["echo-clear-history-button"])
            .build();
        header.append(&clear_btn);

        root.append(&header);

        // Search field
        let search_entry = Entry::builder()
            .placeholder_text("Поиск в истории...")
            .css_classes(["echo-history-search-entry"])
            .hexpand(true)
            .build();
        root.append(&search_entry);

        // Scrolled list
        let list_box = GtkBox::new(Orientation::Vertical, 4);
        list_box.set_hexpand(true);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .max_content_height(420)
            .propagate_natural_height(true)
            .child(&list_box)
            .build();
        root.append(&scrolled);

        popover.set_child(Some(&root));

        let popover_rc = Rc::new(Self {
            popover,
            list_box,
            search_entry,
            on_navigate,
            on_navigate_new_tab,
            on_load,
            on_clear,
        });

        // Wire clear button
        let weak = Rc::downgrade(&popover_rc);
        clear_btn.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                (this.on_clear)();
                this.refresh();
            }
        });

        // Wire search input
        let weak = Rc::downgrade(&popover_rc);
        popover_rc.search_entry.connect_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.refresh();
            }
        });

        // Refresh on popover open
        let weak = Rc::downgrade(&popover_rc);
        popover_rc.popover.connect_show(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search_entry.set_text("");
                this.refresh();
            }
        });

        popover_rc.refresh();
        popover_rc
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }

    pub fn popup(&self) {
        self.refresh();
        self.popover.popup();
    }

    pub fn popdown(&self) {
        self.popover.popdown();
    }

    pub fn refresh(&self) {
        // Clear list box children
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let query = self.search_entry.text();
        let entries = (self.on_load)(query.as_str());

        if entries.is_empty() {
            let empty_box = GtkBox::new(Orientation::Vertical, 6);
            empty_box.set_valign(gtk4::Align::Center);
            empty_box.set_halign(gtk4::Align::Center);
            empty_box.set_margin_top(32);
            empty_box.set_margin_bottom(32);

            let empty_label = Label::builder()
                .label(if query.is_empty() {
                    "История посещений пуста"
                } else {
                    "Ничего не найдено"
                })
                .css_classes(["echo-history-empty-label"])
                .build();
            empty_box.append(&empty_label);
            self.list_box.append(&empty_box);
            return;
        }

        for entry in entries {
            let row = self.create_history_row(entry);
            self.list_box.append(&row);
        }
    }

    fn create_history_row(&self, entry: HistoryEntry) -> GtkBox {
        let row = GtkBox::new(Orientation::Horizontal, 10);
        row.add_css_class("echo-history-row");

        let icon = Image::from_icon_name("web-browser-symbolic");
        icon.add_css_class("echo-history-icon");
        row.append(&icon);

        let text_col = GtkBox::new(Orientation::Vertical, 2);
        text_col.set_hexpand(true);

        let title_label = Label::builder()
            .label(&entry.title)
            .css_classes(["echo-history-item-title"])
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(32)
            .build();
        text_col.append(&title_label);

        let url_label = Label::builder()
            .label(&entry.url)
            .css_classes(["echo-history-item-url"])
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(36)
            .build();
        text_col.append(&url_label);

        row.append(&text_col);

        let time_str = Self::format_time(entry.visit_time);
        let time_label = Label::builder()
            .label(&time_str)
            .css_classes(["echo-history-item-time"])
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::Center)
            .build();
        row.append(&time_label);

        // Click gesture to navigate (left click: current tab, middle click: new tab)
        let click = GestureClick::new();
        click.set_button(0);
        let on_navigate = Rc::clone(&self.on_navigate);
        let on_navigate_new_tab = Rc::clone(&self.on_navigate_new_tab);
        let url = entry.url;
        let popover_weak = self.popover.downgrade();
        click.connect_released(move |gesture, _, _, _| {
            match gesture.current_button() {
                1 => on_navigate(&url),
                2 => on_navigate_new_tab(&url),
                _ => return,
            }
            if let Some(p) = popover_weak.upgrade() {
                p.popdown();
            }
        });
        row.add_controller(click);

        row
    }

    fn format_time(ts: i64) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let diff = now.saturating_sub(ts);
        if diff < 60 {
            "Только что".to_string()
        } else if diff < 3600 {
            format!("{} мин", diff / 60)
        } else if diff < 86400 {
            format!("{} ч", diff / 3600)
        } else {
            format!("{} дн", diff / 86400)
        }
    }
}
