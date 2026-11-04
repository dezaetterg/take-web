use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Image, Label, Orientation, Popover, Separator, Switch};

type ToggleProtectionCallback = Box<dyn Fn(&str, bool)>;
type ClearSiteDataCallback = Box<dyn Fn(&str)>;
type OpenPrivacySettingsCallback = Box<dyn Fn()>;

pub struct ShieldPopover {
    popover: Popover,
    domain_label: Label,
    domain_icon: Image,
    connection_label: Label,
    connection_icon: Image,
    banner_desc: Label,
    protection_switch: Switch,
    protection_title: Label,
    protection_box: GtkBox,
    trackers_box: GtkBox,
    trackers_list_box: GtkBox,
    site_data_box: GtkBox,
    current_url: RefCell<String>,
    current_domain: RefCell<String>,
    on_toggle_protection: RefCell<Option<ToggleProtectionCallback>>,
    on_clear_site_data: RefCell<Option<ClearSiteDataCallback>>,
    on_open_privacy_settings: RefCell<Option<OpenPrivacySettingsCallback>>,
}

impl ShieldPopover {
    pub fn new() -> Rc<Self> {
        let popover = Popover::builder()
            .autohide(true)
            .has_arrow(true)
            .css_classes(["take-shield-popover"])
            .build();

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

        let root = GtkBox::new(Orientation::Vertical, 10);
        root.add_css_class("take-shield-popover-content");

        // --- 1. Header: domain + connection status ---
        let header_row = GtkBox::new(Orientation::Horizontal, 8);
        header_row.add_css_class("take-shield-header-row");

        let domain_cluster = GtkBox::new(Orientation::Horizontal, 6);
        domain_cluster.set_hexpand(true);
        domain_cluster.set_halign(Align::Start);

        let domain_icon = Image::from_icon_name("network-server-symbolic");
        domain_icon.add_css_class("take-shield-domain-icon");
        let domain_label = Label::builder()
            .label("take-web")
            .css_classes(["take-shield-domain-label"])
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        domain_cluster.append(&domain_icon);
        domain_cluster.append(&domain_label);

        let connection_cluster = GtkBox::new(Orientation::Horizontal, 4);
        connection_cluster.set_halign(Align::End);
        let connection_icon = Image::from_icon_name("channel-secure-symbolic");
        connection_icon.add_css_class("take-shield-connection-icon");
        let connection_label = Label::builder()
            .label(crate::i18n::tr("shield.secure_connection"))
            .css_classes(["take-shield-connection-label"])
            .build();
        let connection_chevron = Image::from_icon_name("go-next-symbolic");
        connection_chevron.add_css_class("take-shield-chevron");
        connection_cluster.append(&connection_icon);
        connection_cluster.append(&connection_label);
        connection_cluster.append(&connection_chevron);

        header_row.append(&domain_cluster);
        header_row.append(&connection_cluster);
        root.append(&header_row);

        // --- 2. Purple Shield Banner ---
        let banner_box = GtkBox::new(Orientation::Horizontal, 12);
        banner_box.add_css_class("take-shield-banner");

        let banner_text_col = GtkBox::new(Orientation::Vertical, 4);
        banner_text_col.set_hexpand(true);
        banner_text_col.set_valign(Align::Center);

        let banner_title = Label::builder()
            .label(crate::i18n::tr("shield.on_guard"))
            .css_classes(["take-shield-banner-title"])
            .xalign(0.0)
            .build();

        let banner_desc = Label::builder()
            .label(crate::i18n::tr("shield.protected_subtitle"))
            .css_classes(["take-shield-banner-desc"])
            .wrap(true)
            .xalign(0.0)
            .build();

        banner_text_col.append(&banner_title);
        banner_text_col.append(&banner_desc);

        let shield_art = Image::builder()
            .icon_name("security-high-symbolic")
            .pixel_size(44)
            .css_classes(["take-shield-art-icon"])
            .valign(Align::Center)
            .build();

        banner_box.append(&banner_text_col);
        banner_box.append(&shield_art);
        root.append(&banner_box);

        // --- 3. Enhanced Tracking Protection Switch ---
        let protection_box = GtkBox::new(Orientation::Vertical, 4);
        protection_box.add_css_class("take-shield-protection-section");

        let switch_row = GtkBox::new(Orientation::Horizontal, 8);
        switch_row.set_valign(Align::Center);

        let protection_title = Label::builder()
            .label(crate::i18n::tr("shield.etp_enabled"))
            .css_classes(["take-shield-switch-title"])
            .hexpand(true)
            .wrap(true)
            .xalign(0.0)
            .build();

        let protection_switch = Switch::builder()
            .active(true)
            .valign(Align::Center)
            .css_classes(["take-shield-switch"])
            .build();

        switch_row.append(&protection_title);
        switch_row.append(&protection_switch);

        let protection_subtext = Label::builder()
            .label(crate::i18n::tr("shield.etp_trouble_hint"))
            .css_classes(["take-shield-subtext"])
            .wrap(true)
            .xalign(0.0)
            .build();

        protection_box.append(&switch_row);
        protection_box.append(&protection_subtext);
        root.append(&protection_box);

        // --- Minimalist Blocked Trackers Display ---
        let trackers_box = GtkBox::new(Orientation::Vertical, 4);
        trackers_box.add_css_class("take-shield-trackers-box");

        let trackers_header = Label::builder()
            .label(crate::i18n::tr("shield.blocked_trackers_on_site"))
            .css_classes(["take-shield-trackers-header"])
            .xalign(0.0)
            .build();
        trackers_box.append(&trackers_header);

        let trackers_list_box = GtkBox::new(Orientation::Vertical, 2);
        trackers_box.append(&trackers_list_box);
        root.append(&trackers_box);

        // --- Separator ---
        root.append(&Separator::new(Orientation::Horizontal));

        // --- 4. Clear Cookies & Site Data ---
        let site_data_box = GtkBox::new(Orientation::Horizontal, 8);
        site_data_box.add_css_class("take-shield-action-row");

        let clear_data_btn = Button::builder()
            .css_classes(["flat", "take-shield-action-btn"])
            .hexpand(true)
            .build();

        let clear_data_inner = GtkBox::new(Orientation::Horizontal, 8);
        clear_data_inner.set_hexpand(true);
        let clear_data_label = Label::builder()
            .label(crate::i18n::tr("shield.clear_site_data"))
            .xalign(0.0)
            .hexpand(true)
            .build();
        let clear_data_chevron = Image::from_icon_name("go-next-symbolic");
        clear_data_chevron.add_css_class("take-shield-chevron");
        clear_data_inner.append(&clear_data_label);
        clear_data_inner.append(&clear_data_chevron);
        clear_data_btn.set_child(Some(&clear_data_inner));

        site_data_box.append(&clear_data_btn);
        root.append(&site_data_box);

        // --- Separator ---
        root.append(&Separator::new(Orientation::Horizontal));

        // --- 5. Privacy Settings Link ---
        let privacy_settings_btn = Button::builder()
            .css_classes(["flat", "take-shield-link-btn"])
            .halign(Align::Start)
            .build();
        let privacy_label = Label::builder()
            .label(crate::i18n::tr("shield.privacy_settings"))
            .xalign(0.0)
            .build();
        privacy_settings_btn.set_child(Some(&privacy_label));
        root.append(&privacy_settings_btn);

        popover.set_child(Some(&root));

        let instance = Rc::new(Self {
            popover,
            domain_label,
            domain_icon,
            connection_label,
            connection_icon,
            banner_desc,
            protection_switch,
            protection_title,
            protection_box,
            trackers_box,
            trackers_list_box,
            site_data_box,
            current_url: RefCell::new(String::new()),
            current_domain: RefCell::new(String::new()),
            on_toggle_protection: RefCell::new(None),
            on_clear_site_data: RefCell::new(None),
            on_open_privacy_settings: RefCell::new(None),
        });

        // Wire switch
        {
            let this_weak = Rc::downgrade(&instance);
            instance
                .protection_switch
                .connect_state_set(move |_sw, active| {
                    if let Some(this) = this_weak.upgrade() {
                        let domain = this.current_domain.borrow().clone();
                        if !domain.is_empty() {
                            this.update_switch_label(active);
                            if let Some(ref cb) = *this.on_toggle_protection.borrow() {
                                cb(&domain, active);
                            }
                        }
                    }
                    glib::Propagation::Proceed
                });
        }

        // Wire clear data
        {
            let this_weak = Rc::downgrade(&instance);
            clear_data_btn.connect_clicked(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    let domain = this.current_domain.borrow().clone();
                    this.popover.popdown();
                    if let Some(ref cb) = *this.on_clear_site_data.borrow() {
                        cb(&domain);
                    }
                }
            });
        }

        // Wire privacy settings
        {
            let this_weak = Rc::downgrade(&instance);
            privacy_settings_btn.connect_clicked(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    this.popover.popdown();
                    if let Some(ref cb) = *this.on_open_privacy_settings.borrow() {
                        cb();
                    }
                }
            });
        }

        instance
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }

    pub fn popdown(&self) {
        self.popover.popdown();
    }

    pub fn update_state(
        &self,
        url: &str,
        is_protected: bool,
        blocked_trackers: usize,
        tracker_names: &[String],
    ) {
        *self.current_url.borrow_mut() = url.to_string();

        let is_internal = url.starts_with("take://") || url.starts_with("about:") || url.is_empty();

        if is_internal {
            self.domain_label.set_label("Take Web");
            self.domain_icon.set_icon_name(Some("go-home-symbolic"));
            self.connection_label
                .set_label(crate::i18n::tr("shield.internal_page"));
            self.connection_icon
                .set_icon_name(Some("channel-secure-symbolic"));
            self.connection_icon.remove_css_class("insecure");
            self.connection_icon.add_css_class("secure");
            self.banner_desc
                .set_label(crate::i18n::tr("shield.internal_page_desc"));
            self.protection_box.set_visible(false);
            self.trackers_box.set_visible(false);
            self.site_data_box.set_visible(false);
            *self.current_domain.borrow_mut() = String::new();
            return;
        }

        self.protection_box.set_visible(true);
        self.trackers_box.set_visible(true);
        self.site_data_box.set_visible(true);

        while let Some(child) = self.trackers_list_box.first_child() {
            self.trackers_list_box.remove(&child);
        }

        if tracker_names.is_empty() {
            let row = GtkBox::new(Orientation::Horizontal, 6);
            let ok_icon = Image::from_icon_name("emblem-ok-symbolic");
            ok_icon.add_css_class("take-shield-tracker-icon");
            let ok_label = Label::builder()
                .label(crate::i18n::tr("shield.no_trackers_detected"))
                .css_classes(["take-shield-subtext"])
                .xalign(0.0)
                .build();
            row.append(&ok_icon);
            row.append(&ok_label);
            self.trackers_list_box.append(&row);
        } else {
            for tracker in tracker_names.iter().take(5) {
                let row = GtkBox::new(Orientation::Horizontal, 6);
                row.add_css_class("take-shield-tracker-item");
                let icon = Image::from_icon_name("channel-secure-symbolic");
                icon.add_css_class("take-shield-tracker-icon");
                let name = Label::builder()
                    .label(tracker)
                    .hexpand(true)
                    .xalign(0.0)
                    .ellipsize(gtk4::pango::EllipsizeMode::Middle)
                    .build();
                let badge = Label::builder()
                    .label("✓")
                    .css_classes(["take-shield-tracker-badge"])
                    .build();
                row.append(&icon);
                row.append(&name);
                row.append(&badge);
                self.trackers_list_box.append(&row);
            }
            if tracker_names.len() > 5 {
                let more = Label::builder()
                    .label(format!("+{} more", tracker_names.len() - 5))
                    .css_classes(["take-shield-subtext"])
                    .xalign(0.0)
                    .build();
                self.trackers_list_box.append(&more);
            }
        }

        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.host_str()
                    .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
            })
            .unwrap_or_else(|| url.to_string());

        *self.current_domain.borrow_mut() = domain.clone();
        self.domain_label.set_label(&domain);
        self.domain_icon.set_icon_name(Some("web-browser-symbolic"));

        let is_https = url.starts_with("https://");
        if is_https {
            self.connection_label
                .set_label(crate::i18n::tr("shield.secure_connection"));
            self.connection_icon
                .set_icon_name(Some("channel-secure-symbolic"));
            self.connection_icon.remove_css_class("insecure");
            self.connection_icon.add_css_class("secure");
        } else {
            self.connection_label
                .set_label(crate::i18n::tr("shield.insecure_connection"));
            self.connection_icon
                .set_icon_name(Some("channel-insecure-symbolic"));
            self.connection_icon.remove_css_class("secure");
            self.connection_icon.add_css_class("insecure");
        }

        if blocked_trackers > 0 {
            let count_str = blocked_trackers.to_string();
            self.banner_desc.set_label(&crate::i18n::tr_fmt(
                "shield.blocked_count",
                &[("count", &count_str)],
            ));
        } else {
            self.banner_desc
                .set_label(crate::i18n::tr("shield.protected_subtitle"));
        }

        self.protection_switch.set_state(is_protected);
        self.update_switch_label(is_protected);
    }

    fn update_switch_label(&self, active: bool) {
        if active {
            self.protection_title
                .set_label(crate::i18n::tr("shield.etp_enabled"));
            self.protection_title.remove_css_class("dimmed");
        } else {
            self.protection_title
                .set_label(crate::i18n::tr("shield.etp_disabled"));
            self.protection_title.add_css_class("dimmed");
        }
    }

    pub fn set_on_toggle_protection<F: Fn(&str, bool) + 'static>(&self, f: F) {
        *self.on_toggle_protection.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_clear_site_data<F: Fn(&str) + 'static>(&self, f: F) {
        *self.on_clear_site_data.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_open_privacy_settings<F: Fn() + 'static>(&self, f: F) {
        *self.on_open_privacy_settings.borrow_mut() = Some(Box::new(f));
    }
}
