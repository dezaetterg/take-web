use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, CheckButton, Image, Label, Orientation, Revealer,
    RevealerTransitionType,
};

use crate::browser::permissions::PermissionRequestInfo;

type DecisionCallback = Box<dyn FnOnce(bool, bool)>;

pub struct PermissionBar {
    revealer: Revealer,
    icon: Image,
    label: Label,
    remember_check: CheckButton,
    pending: RefCell<VecDeque<(PermissionRequestInfo, DecisionCallback)>>,
    active_callback: RefCell<Option<DecisionCallback>>,
}

impl PermissionBar {
    pub fn new() -> Rc<Self> {
        let container = GtkBox::new(Orientation::Horizontal, 10);
        container.add_css_class("take-permission-bar");

        let icon = Image::from_icon_name("dialog-question-symbolic");
        icon.add_css_class("take-permission-icon");
        icon.set_pixel_size(20);
        container.append(&icon);

        let label = Label::new(None);
        label.add_css_class("take-permission-label");
        label.set_use_markup(true);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        label.set_max_width_chars(65);
        label.set_xalign(0.0);
        container.append(&label);

        let remember_check = CheckButton::with_label(crate::i18n::tr("permission_bar.remember"));
        remember_check.add_css_class("take-permission-check");
        remember_check.set_active(true);
        container.append(&remember_check);

        let deny_btn = Button::builder()
            .label(crate::i18n::tr("permission_bar.deny"))
            .css_classes(["take-permission-deny-btn", "destructive-action"])
            .build();
        container.append(&deny_btn);

        let allow_btn = Button::builder()
            .label(crate::i18n::tr("permission_bar.allow"))
            .css_classes(["take-permission-allow-btn", "suggested-action"])
            .build();
        container.append(&allow_btn);

        let close_btn = Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat"])
            .tooltip_text(crate::i18n::tr("permission_bar.close_tooltip"))
            .build();
        container.append(&close_btn);

        let revealer = Revealer::new();
        revealer.set_transition_type(RevealerTransitionType::SlideDown);
        revealer.set_transition_duration(200);
        revealer.set_halign(Align::Center);
        revealer.set_valign(Align::Start);
        revealer.set_margin_top(8);
        revealer.set_child(Some(&container));
        revealer.set_reveal_child(false);

        let bar = Rc::new(Self {
            revealer,
            icon,
            label,
            remember_check,
            pending: RefCell::new(VecDeque::new()),
            active_callback: RefCell::new(None),
        });

        {
            let weak = Rc::downgrade(&bar);
            allow_btn.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.finish_current(true);
                }
            });
        }

        {
            let weak = Rc::downgrade(&bar);
            deny_btn.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.finish_current(false);
                }
            });
        }

        {
            let weak = Rc::downgrade(&bar);
            close_btn.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    // Closing denies without remembering
                    this.remember_check.set_active(false);
                    this.finish_current(false);
                }
            });
        }

        bar
    }

    pub fn widget(&self) -> &Revealer {
        &self.revealer
    }

    /// Queues or displays a permission prompt for the user.
    pub fn prompt<F>(self: &Rc<Self>, info: PermissionRequestInfo, callback: F)
    where
        F: FnOnce(bool, bool) + 'static,
    {
        if self.active_callback.borrow().is_none() {
            self.show_prompt(info, Box::new(callback));
        } else {
            self.pending
                .borrow_mut()
                .push_back((info, Box::new(callback)));
        }
    }

    fn show_prompt(&self, info: PermissionRequestInfo, callback: DecisionCallback) {
        *self.active_callback.borrow_mut() = Some(callback);

        self.icon.set_icon_name(Some(info.icon_name));

        let escaped_origin = glib::markup_escape_text(&info.origin);
        let detail_text = match info.extra_details {
            Some(details) => format!(" ({})", glib::markup_escape_text(&details)),
            None => String::new(),
        };

        let markup = crate::i18n::tr_fmt(
            "permission_bar.prompt",
            &[
                ("domain", &escaped_origin),
                ("permission", &info.description),
                ("details", &detail_text),
            ],
        );
        self.label.set_markup(&markup);
        self.remember_check.set_active(true);
        self.revealer.set_reveal_child(true);
    }

    fn finish_current(&self, allowed: bool) {
        let remember = self.remember_check.is_active();
        if let Some(cb) = self.active_callback.borrow_mut().take() {
            cb(allowed, remember);
        }

        if let Some((next_info, next_cb)) = self.pending.borrow_mut().pop_front() {
            self.show_prompt(next_info, next_cb);
        } else {
            self.revealer.set_reveal_child(false);
        }
    }

    /// Clears any pending requests and hides the bar (e.g. when navigating away).
    pub fn dismiss_all(&self) {
        if let Some(cb) = self.active_callback.borrow_mut().take() {
            cb(false, false);
        }
        for (_, cb) in self.pending.borrow_mut().drain(..) {
            cb(false, false);
        }
        self.revealer.set_reveal_child(false);
    }
}
