use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, EventControllerFocus, MenuButton, Orientation, ProgressBar,
    ToggleButton,
};
use libadwaita as adw;

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
    pub app_title_label: gtk4::Label,
    pub reader_button: Button,
    pub translate_button: MenuButton,
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
        container.add_css_class("take-headerbar");

        let sidebar_toggle = ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text(crate::i18n::tr("header.sidebar_toggle"))
            .css_classes(["take-nav-button"])
            .active(true)
            .build();
        container.pack_start(&sidebar_toggle);

        let nav_box = GtkBox::new(Orientation::Horizontal, 2);
        nav_box.add_css_class("take-nav-cluster");

        let back_button = Self::nav_button("go-previous-symbolic", crate::i18n::tr("header.back"));
        back_button.set_sensitive(false);
        let forward_button =
            Self::nav_button("go-next-symbolic", crate::i18n::tr("header.forward"));
        forward_button.set_sensitive(false);
        let reload_button =
            Self::nav_button("view-refresh-symbolic", crate::i18n::tr("header.reload"));

        nav_box.append(&back_button);
        nav_box.append(&forward_button);
        nav_box.append(&reload_button);
        container.pack_start(&nav_box);

        let security_button = MenuButton::builder()
            .icon_name("system-search-symbolic")
            .always_show_arrow(false)
            .tooltip_text(crate::i18n::tr("header.shield_tooltip"))
            .css_classes(["take-security-button"])
            .build();

        let address_entry = Entry::builder()
            .placeholder_text(crate::i18n::tr("header.address_placeholder"))
            .hexpand(true)
            .css_classes(["take-address-entry"])
            .build();

        let reader_button = Button::builder()
            .icon_name("open-book-symbolic")
            .tooltip_text(crate::i18n::tr("reader_mode_tooltip"))
            .css_classes(["take-capsule-button", "take-reader-button"])
            .build();

        let translate_button = MenuButton::builder()
            .icon_name("preferences-desktop-locale-symbolic")
            .always_show_arrow(false)
            .tooltip_text(crate::i18n::tr("translate_page_tooltip"))
            .css_classes(["take-capsule-button"])
            .build();

        let bookmark_button = Button::builder()
            .icon_name("non-starred-symbolic")
            .tooltip_text(crate::i18n::tr("header.bookmark_tooltip"))
            .css_classes(["take-capsule-button"])
            .build();

        let reading_list_button = Button::builder()
            .icon_name("bookmark-new-symbolic")
            .tooltip_text(crate::i18n::tr("header.reading_list_tooltip"))
            .css_classes(["take-capsule-button"])
            .build();

        let address_capsule = GtkBox::new(Orientation::Horizontal, 4);
        address_capsule.add_css_class("take-address-capsule");
        address_capsule.append(&security_button);
        address_capsule.append(&address_entry);
        address_capsule.append(&reader_button);
        address_capsule.append(&translate_button);
        address_capsule.append(&bookmark_button);
        address_capsule.append(&reading_list_button);

        let app_title_label = gtk4::Label::builder()
            .label("Take Web")
            .css_classes(["take-app-mode-title"])
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(60)
            .visible(false)
            .build();

        let title_box = GtkBox::new(Orientation::Vertical, 0);
        title_box.set_valign(gtk4::Align::Center);
        title_box.append(&address_capsule);
        title_box.append(&app_title_label);

        let clamp = adw::Clamp::builder()
            .maximum_size(720)
            .tightening_threshold(720)
            .child(&title_box)
            .build();
        container.set_title_widget(Some(&clamp));

        let burger_button = MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text(crate::i18n::tr("header.menu_tooltip"))
            .css_classes(["take-nav-button", "take-burger-button"])
            .build();
        container.pack_end(&burger_button);

        let downloads_button = MenuButton::builder()
            .icon_name("folder-download-symbolic")
            .tooltip_text(crate::i18n::tr("header.downloads_tooltip"))
            .css_classes(["take-nav-button", "take-downloads-button"])
            .visible(false)
            .build();
        container.pack_end(&downloads_button);

        let tab_overview_button = Button::builder()
            .icon_name("view-grid-symbolic")
            .tooltip_text(crate::i18n::tr("header.tabs_overview_tooltip"))
            .css_classes(["take-nav-button", "take-tab-overview-button"])
            .build();
        container.pack_end(&tab_overview_button);

        let new_tab_button = Button::builder()
            .icon_name("tab-new-symbolic")
            .tooltip_text(crate::i18n::tr("header.new_tab_tooltip"))
            .css_classes(["take-nav-button", "take-new-tab-button"])
            .build();

        let progress_bar = ProgressBar::new();
        progress_bar.add_css_class("take-progress");
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
            app_title_label,
            reader_button,
            translate_button,
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
            .css_classes(["take-nav-button"])
            .build()
    }

    fn connect_address_focus(&self) {
        let focus = EventControllerFocus::new();

        let state = self.address_state.clone();
        let entry = self.address_entry.clone();
        focus.connect_enter(move |_| {
            state.editing.set(true);
            let raw = state.full_url.borrow();
            let editing_text = if raw.as_str() == "take://start" {
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
        if url.is_empty() || url == "take://start" {
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
                .set_tooltip_text(Some(crate::i18n::tr("header.edit_bookmark_tooltip")));
        } else {
            self.bookmark_button.set_icon_name("non-starred-symbolic");
            self.bookmark_button.remove_css_class("active");
            self.bookmark_button
                .set_tooltip_text(Some(crate::i18n::tr("header.bookmark_tooltip")));
        }
    }

    pub fn set_loading(&self, loading: bool) {
        let (icon, tooltip) = if loading {
            (
                "process-stop-symbolic",
                crate::i18n::tr("header.stop_loading"),
            )
        } else {
            ("view-refresh-symbolic", crate::i18n::tr("header.reload"))
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
