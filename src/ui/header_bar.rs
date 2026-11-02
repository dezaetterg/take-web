use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, EventControllerFocus, MenuButton, Orientation, ProgressBar,
    ToggleButton,
};
use libadwaita as adw;

const ADDRESS_PLACEHOLDER: &str = "Поиск или веб-адрес";

/// The address entry shows a short host while idle and the full URL while editing,
/// so the real URL has to live outside the widget.
struct AddressState {
    full_url: RefCell<String>,
    editing: Cell<bool>,
}

#[derive(Clone)]
pub struct BrowserHeaderBar {
    pub container: adw::HeaderBar,
    pub back_button: Button,
    pub forward_button: Button,
    pub reload_button: Button,
    pub sidebar_toggle: ToggleButton,
    pub address_entry: Entry,
    pub address_capsule: GtkBox,
    pub bookmark_button: Button,
    pub reading_list_button: Button,
    pub new_tab_button: Button,
    pub tab_overview_button: Button,
    pub burger_button: MenuButton,
    pub downloads_button: MenuButton,
    pub progress_bar: ProgressBar,
    pub security_button: MenuButton,
    address_state: Rc<AddressState>,
}

impl BrowserHeaderBar {
    pub fn new() -> Self {
        let container = adw::HeaderBar::new();
        container.add_css_class("echo-headerbar");

        let sidebar_toggle = ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Боковая панель (Ctrl+Shift+S)")
            .css_classes(["echo-nav-button"])
            .active(true)
            .build();
        container.pack_start(&sidebar_toggle);

        let nav_box = GtkBox::new(Orientation::Horizontal, 2);
        nav_box.add_css_class("echo-nav-cluster");

        let back_button = Self::nav_button("go-previous-symbolic", "Назад (Alt+←)");
        back_button.set_sensitive(false);
        let forward_button = Self::nav_button("go-next-symbolic", "Вперёд (Alt+→)");
        forward_button.set_sensitive(false);
        let reload_button = Self::nav_button("view-refresh-symbolic", "Обновить (Ctrl+R)");

        nav_box.append(&back_button);
        nav_box.append(&forward_button);
        nav_box.append(&reload_button);
        container.pack_start(&nav_box);

        let security_button = MenuButton::builder()
            .icon_name("system-search-symbolic")
            .always_show_arrow(false)
            .tooltip_text("Защита Take Shield и параметры сайта")
            .css_classes(["echo-security-button"])
            .build();

        let address_entry = Entry::builder()
            .placeholder_text(ADDRESS_PLACEHOLDER)
            .hexpand(true)
            .css_classes(["echo-address-entry"])
            .build();

        let bookmark_button = Button::builder()
            .icon_name("non-starred-symbolic")
            .tooltip_text("Добавить в закладки (Ctrl+D)")
            .css_classes(["echo-capsule-button"])
            .build();

        let reading_list_button = Button::builder()
            .icon_name("bookmark-new-symbolic")
            .tooltip_text("Добавить в список для чтения (Ctrl+Shift+D)")
            .css_classes(["echo-capsule-button"])
            .build();

        let address_capsule = GtkBox::new(Orientation::Horizontal, 4);
        address_capsule.add_css_class("echo-address-capsule");
        address_capsule.append(&security_button);
        address_capsule.append(&address_entry);
        address_capsule.append(&bookmark_button);
        address_capsule.append(&reading_list_button);

        let clamp = adw::Clamp::builder()
            .maximum_size(720)
            .tightening_threshold(720)
            .child(&address_capsule)
            .build();
        container.set_title_widget(Some(&clamp));

        let burger_button = MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Главное меню")
            .css_classes(["echo-nav-button", "echo-burger-button"])
            .build();
        container.pack_end(&burger_button);

        let downloads_button = MenuButton::builder()
            .icon_name("folder-download-symbolic")
            .tooltip_text("Загрузки (Ctrl+J)")
            .css_classes(["echo-nav-button", "echo-downloads-button"])
            .visible(false)
            .build();
        container.pack_end(&downloads_button);

        let tab_overview_button = Button::builder()
            .icon_name("view-grid-symbolic")
            .tooltip_text("Обзор вкладок (Ctrl+Shift+O)")
            .css_classes(["echo-nav-button", "echo-tab-overview-button"])
            .build();
        container.pack_end(&tab_overview_button);

        let new_tab_button = Button::builder()
            .icon_name("tab-new-symbolic")
            .tooltip_text("Новая вкладка (Ctrl+T)")
            .css_classes(["echo-nav-button", "echo-new-tab-button"])
            .build();

        let progress_bar = ProgressBar::new();
        progress_bar.add_css_class("echo-progress");
        progress_bar.set_visible(false);
        progress_bar.set_valign(gtk4::Align::Start);

        let address_state = Rc::new(AddressState {
            full_url: RefCell::new(String::new()),
            editing: Cell::new(false),
        });

        let header = Self {
            container,
            back_button,
            forward_button,
            reload_button,
            sidebar_toggle,
            address_entry,
            address_capsule,
            bookmark_button,
            reading_list_button,
            new_tab_button,
            tab_overview_button,
            burger_button,
            downloads_button,
            progress_bar,
            security_button,
            address_state,
        };
        header.connect_address_focus();
        header
    }

    fn nav_button(icon: &str, tooltip: &str) -> Button {
        Button::builder()
            .icon_name(icon)
            .tooltip_text(tooltip)
            .css_classes(["echo-nav-button"])
            .build()
    }

    fn connect_address_focus(&self) {
        let focus = EventControllerFocus::new();

        let state = self.address_state.clone();
        let entry = self.address_entry.clone();
        focus.connect_enter(move |_| {
            state.editing.set(true);
            let raw = state.full_url.borrow();
            let editing_text = if raw.as_str() == "take://start" || raw.as_str() == "echo://start" {
                ""
            } else {
                raw.as_str()
            };
            entry.set_text(editing_text);
            let entry = entry.clone();
            glib::idle_add_local_once(move || entry.select_region(0, -1));
        });

        let state = self.address_state.clone();
        let entry = self.address_entry.clone();
        focus.connect_leave(move |_| {
            state.editing.set(false);
            entry.set_text(&Self::display_text(&state.full_url.borrow()));
        });

        self.address_entry.add_controller(focus);
    }

    pub fn is_editing(&self) -> bool {
        self.address_state.editing.get()
    }

    /// Host without `www.` for web pages, nothing for the start page, the raw URL otherwise.
    fn display_text(url: &str) -> String {
        if url.is_empty() || url == "take://start" || url == "echo://start" {
            return String::new();
        }
        match url::Url::parse(url) {
            Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => parsed
                .host_str()
                .map(|host| host.strip_prefix("www.").unwrap_or(host).to_string())
                .unwrap_or_else(|| url.to_string()),
            _ => url.to_string(),
        }
    }

    pub fn set_address(&self, logical_url: &str) {
        *self.address_state.full_url.borrow_mut() = logical_url.to_string();
        if !self.address_state.editing.get() {
            self.address_entry
                .set_text(&Self::display_text(logical_url));
        }
        self.update_security_icon(logical_url);
    }

    fn update_security_icon(&self, url: &str) {
        self.security_button.remove_css_class("insecure");
        self.security_button.remove_css_class("neutral");

        if url.starts_with("https://") {
            self.security_button
                .set_icon_name("channel-secure-symbolic");
        } else if url.starts_with("http://") {
            self.security_button
                .set_icon_name("channel-insecure-symbolic");
            self.security_button.add_css_class("insecure");
        } else {
            self.security_button.set_icon_name("system-search-symbolic");
            self.security_button.add_css_class("neutral");
        }
    }

    pub fn focus_address(&self) {
        self.address_entry.grab_focus();
    }

    pub fn set_bookmarked(&self, bookmarked: bool) {
        if bookmarked {
            self.bookmark_button.set_icon_name("starred-symbolic");
            self.bookmark_button.add_css_class("active");
            self.bookmark_button
                .set_tooltip_text(Some("Изменить закладку (Ctrl+D)"));
        } else {
            self.bookmark_button.set_icon_name("non-starred-symbolic");
            self.bookmark_button.remove_css_class("active");
            self.bookmark_button
                .set_tooltip_text(Some("Добавить в закладки (Ctrl+D)"));
        }
    }

    pub fn set_loading(&self, loading: bool) {
        let (icon, tooltip) = if loading {
            ("process-stop-symbolic", "Остановить загрузку")
        } else {
            ("view-refresh-symbolic", "Обновить (Ctrl+R)")
        };
        self.reload_button.set_icon_name(icon);
        self.reload_button.set_tooltip_text(Some(tooltip));
    }

    pub fn set_progress(&self, fraction: f64) {
        self.progress_bar.set_fraction(fraction);
        self.progress_bar
            .set_visible(fraction > 0.0 && fraction < 1.0);
    }
}
