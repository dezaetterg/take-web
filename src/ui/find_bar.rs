use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk::{Key, ModifierType};
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Entry, EventControllerKey, Image, Label, Orientation};
use webkit6::prelude::*;
use webkit6::{FindController, FindOptions, WebView};

const MAX_MATCH_COUNT: u32 = 1000;

pub struct FindBar {
    container: GtkBox,
    entry: Entry,
    counter_label: Label,
    prev_button: Button,
    next_button: Button,
    close_button: Button,
    current_controller: RefCell<Option<FindController>>,
    active_view: RefCell<Option<glib::WeakRef<WebView>>>,
    current_match: Cell<u32>,
    total_matches: Cell<u32>,
}

impl FindBar {
    pub fn new() -> Rc<Self> {
        let container = GtkBox::new(Orientation::Horizontal, 4);
        container.add_css_class("echo-find-bar");
        container.set_halign(gtk4::Align::End);
        container.set_valign(gtk4::Align::Start);
        container.set_margin_top(12);
        container.set_margin_end(18);
        container.set_visible(false);

        let search_icon = Image::from_icon_name("edit-find-symbolic");
        search_icon.add_css_class("echo-find-icon");
        container.append(&search_icon);

        let entry = Entry::builder()
            .placeholder_text("Поиск по странице...")
            .css_classes(["echo-find-entry"])
            .max_width_chars(20)
            .width_chars(15)
            .build();
        container.append(&entry);

        let counter_label = Label::new(None);
        counter_label.add_css_class("echo-find-counter");
        container.append(&counter_label);

        let prev_button = Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text("Предыдущее (Shift+Enter)")
            .css_classes(["echo-nav-button", "echo-find-btn"])
            .build();
        container.append(&prev_button);

        let next_button = Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text("Следующее (Enter)")
            .css_classes(["echo-nav-button", "echo-find-btn"])
            .build();
        container.append(&next_button);

        let close_button = Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Закрыть (Esc)")
            .css_classes(["echo-nav-button", "echo-find-btn", "echo-find-close"])
            .build();
        container.append(&close_button);

        let bar = Rc::new(Self {
            container,
            entry,
            counter_label,
            prev_button,
            next_button,
            close_button,
            current_controller: RefCell::new(None),
            active_view: RefCell::new(None),
            current_match: Cell::new(0),
            total_matches: Cell::new(0),
        });

        bar.setup_signals();
        bar
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }

    pub fn is_visible(&self) -> bool {
        self.container.is_visible()
    }

    /// Shows the find bar for the given web view and focuses the search input.
    pub fn show_for_view(self: &Rc<Self>, view: &WebView) {
        self.attach_controller(view);
        self.container.set_visible(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);

        let text = self.entry.text();
        if !text.is_empty() {
            self.execute_search(&text);
        }
    }

    /// Updates the target view (e.g. after tab switch) while find bar is open.
    pub fn update_view(self: &Rc<Self>, view: Option<&WebView>) {
        if let Some(v) = view {
            self.attach_controller(v);
            let text = self.entry.text();
            if !text.is_empty() {
                self.execute_search(&text);
            }
        } else {
            self.close();
        }
    }

    /// Closes find bar, stops highlighting in WebKit, and restores focus.
    pub fn close(&self) {
        if let Some(ctrl) = self.current_controller.borrow().as_ref() {
            ctrl.search_finish();
        }
        self.container.set_visible(false);
        self.counter_label.set_text("");
        self.current_match.set(0);
        self.total_matches.set(0);

        if let Some(weak_view) = self.active_view.borrow().as_ref() {
            if let Some(view) = weak_view.upgrade() {
                view.grab_focus();
            }
        }
    }

    fn attach_controller(self: &Rc<Self>, view: &WebView) {
        *self.active_view.borrow_mut() = Some(view.downgrade());

        let Some(controller) = view.find_controller() else {
            return;
        };

        let weak_self = Rc::downgrade(self);
        controller.connect_found_text(move |_, match_count| {
            if let Some(this) = weak_self.upgrade() {
                if this.total_matches.get() == 0 && match_count > 0 {
                    this.total_matches.set(match_count);
                    if this.current_match.get() == 0 {
                        this.current_match.set(1);
                    }
                    this.update_counter_display();
                }
            }
        });

        let weak_self = Rc::downgrade(self);
        controller.connect_counted_matches(move |_, total| {
            if let Some(this) = weak_self.upgrade() {
                this.total_matches.set(total);
                if total > 0 {
                    if this.current_match.get() == 0 {
                        this.current_match.set(1);
                    }
                } else {
                    this.current_match.set(0);
                }
                this.update_counter_display();
            }
        });

        let weak_self = Rc::downgrade(self);
        controller.connect_failed_to_find_text(move |_| {
            if let Some(this) = weak_self.upgrade() {
                this.current_match.set(0);
                this.total_matches.set(0);
                this.counter_label.set_text("0 из 0");
            }
        });

        *self.current_controller.borrow_mut() = Some(controller);
    }

    fn execute_search(&self, text: &str) {
        if let Some(controller) = self.current_controller.borrow().as_ref() {
            if text.is_empty() {
                controller.search_finish();
                self.counter_label.set_text("");
                self.current_match.set(0);
                self.total_matches.set(0);
                return;
            }

            self.current_match.set(0);
            self.total_matches.set(0);
            let options = (FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND).bits();
            controller.search(text, options, MAX_MATCH_COUNT);
            controller.count_matches(text, options, MAX_MATCH_COUNT);
        }
    }

    fn search_next(&self) {
        if let Some(controller) = self.current_controller.borrow().as_ref() {
            let total = self.total_matches.get();
            if total > 0 {
                let cur = self.current_match.get();
                let next = if cur >= total { 1 } else { cur + 1 };
                self.current_match.set(next);
                self.update_counter_display();
            }
            controller.search_next();
        }
    }

    fn search_prev(&self) {
        if let Some(controller) = self.current_controller.borrow().as_ref() {
            let total = self.total_matches.get();
            if total > 0 {
                let cur = self.current_match.get();
                let prev = if cur <= 1 { total } else { cur - 1 };
                self.current_match.set(prev);
                self.update_counter_display();
            }
            controller.search_previous();
        }
    }

    fn update_counter_display(&self) {
        let current = self.current_match.get();
        let total = self.total_matches.get();
        if total > 0 {
            let display_curr = if current == 0 { 1 } else { current };
            self.counter_label
                .set_text(&format!("{display_curr} из {total}"));
        } else {
            self.counter_label.set_text("0 из 0");
        }
    }

    fn setup_signals(self: &Rc<Self>) {
        // Text changed in search entry
        let weak = Rc::downgrade(self);
        self.entry.connect_changed(move |entry| {
            if let Some(this) = weak.upgrade() {
                this.execute_search(&entry.text());
            }
        });

        // Next / Prev buttons
        let weak = Rc::downgrade(self);
        self.next_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search_next();
            }
        });

        let weak = Rc::downgrade(self);
        self.prev_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search_prev();
            }
        });

        // Close button
        let weak = Rc::downgrade(self);
        self.close_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.close();
            }
        });

        // Keyboard handler for Esc, Enter, Shift+Enter
        let key_controller = EventControllerKey::new();
        let weak = Rc::downgrade(self);
        key_controller.connect_key_pressed(move |_, key, _, state| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };

            match key {
                Key::Escape => {
                    this.close();
                    glib::Propagation::Stop
                }
                Key::Return | Key::KP_Enter => {
                    if state.contains(ModifierType::SHIFT_MASK) {
                        this.search_prev();
                    } else {
                        this.search_next();
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.entry.add_controller(key_controller);
    }
}
