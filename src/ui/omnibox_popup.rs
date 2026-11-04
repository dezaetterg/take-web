use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, EventControllerMotion, GestureClick, Image, Label, Orientation, Popover,
    ScrolledWindow, Widget,
};

use crate::storage::{OmniboxItemType, OmniboxResult};

pub type OnSelectCallback = Box<dyn Fn(String)>;

pub struct OmniboxPopup {
    popover: Popover,
    parent: Widget,
    scrolled: ScrolledWindow,
    list_box: GtkBox,
    results: RefCell<Vec<OmniboxResult>>,
    row_widgets: RefCell<Vec<GtkBox>>,
    selected_index: Cell<Option<usize>>,
    on_select: RefCell<Option<OnSelectCallback>>,
}

impl OmniboxPopup {
    pub fn new(parent: &impl IsA<Widget>) -> Rc<Self> {
        let parent_widget = parent.as_ref().clone();
        let popover = Popover::new();
        popover.set_parent(&parent_widget);
        popover.set_has_arrow(false);
        popover.set_autohide(false);
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_offset(0, 0);
        popover.add_css_class("take-omnibox-popover");

        let list_box = GtkBox::new(Orientation::Vertical, 1);
        list_box.set_hexpand(true);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .max_content_height(250)
            .propagate_natural_height(true)
            .propagate_natural_width(false)
            .child(&list_box)
            .build();

        popover.set_child(Some(&scrolled));

        let weak_parent = parent_widget.downgrade();
        popover.connect_closed(move |_| {
            if let Some(p) = weak_parent.upgrade() {
                p.remove_css_class("take-capsule-attached");
            }
        });

        Rc::new(Self {
            popover,
            parent: parent_widget,
            scrolled,
            list_box,
            results: RefCell::new(Vec::new()),
            row_widgets: RefCell::new(Vec::new()),
            selected_index: Cell::new(None),
            on_select: RefCell::new(None),
        })
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }

    pub fn is_visible(&self) -> bool {
        self.popover.is_visible()
    }

    pub fn close(&self) {
        self.highlight_index(None);
        self.parent.remove_css_class("take-capsule-attached");
        self.popover.set_offset(0, 0);
        self.popover.popdown();
    }

    pub fn set_on_select<F: Fn(String) + 'static>(&self, callback: F) {
        *self.on_select.borrow_mut() = Some(Box::new(callback));
    }

    pub fn selected_url(&self) -> Option<String> {
        self.selected_index
            .get()
            .and_then(|idx| self.results.borrow().get(idx).map(|r| r.url.clone()))
    }

    pub fn select_next(self: &Rc<Self>) -> Option<String> {
        let count = self.results.borrow().len();
        if count == 0 {
            return None;
        }
        let next = match self.selected_index.get() {
            None => 0,
            Some(i) if i + 1 < count => i + 1,
            Some(_) => 0,
        };
        self.highlight_index(Some(next));
        self.results.borrow().get(next).map(|r| r.url.clone())
    }

    pub fn select_prev(self: &Rc<Self>) -> Option<String> {
        let count = self.results.borrow().len();
        if count == 0 {
            return None;
        }
        let prev = match self.selected_index.get() {
            None => count.saturating_sub(1),
            Some(0) => {
                self.highlight_index(None);
                return None;
            }
            Some(i) => i.saturating_sub(1),
        };
        self.highlight_index(Some(prev));
        self.results.borrow().get(prev).map(|r| r.url.clone())
    }

    fn highlight_index(&self, index: Option<usize>) {
        self.selected_index.set(index);
        let rows = self.row_widgets.borrow();
        for (i, row) in rows.iter().enumerate() {
            if Some(i) == index {
                row.add_css_class("selected");
            } else {
                row.remove_css_class("selected");
            }
        }
    }

    fn update_geometry(&self) {
        let (outer_x, outer_y, outer_w, outer_h) =
            if let Some(bounds) = self.parent.compute_bounds(&self.parent) {
                let w = bounds.width().round() as i32;
                let h = bounds.height().round() as i32;
                let x = bounds.x().round() as i32;
                let y = bounds.y().round() as i32;
                if w > 50 {
                    (x, y, w, if (10..100).contains(&h) { h } else { 34 })
                } else {
                    let pw = self.parent.width();
                    let ph = self.parent.height();
                    (
                        -13,
                        -1,
                        if pw > 50 { pw + 18 } else { 720 },
                        if ph > 10 { ph } else { 34 },
                    )
                }
            } else {
                let pw = self.parent.width();
                let ph = self.parent.height();
                (
                    -13,
                    -1,
                    if pw > 50 { pw + 18 } else { 720 },
                    if ph > 10 { ph } else { 34 },
                )
            };

        // Outer width strictly matches the search bar capsule outer width.
        // The popover > contents card has border: 1px solid (2px total horizontal border),
        // so the inner ScrolledWindow content width is (outer_w - 2).
        let target_content_w = (outer_w - 2).max(280);

        self.scrolled.set_max_content_width(target_content_w);
        self.scrolled.set_min_content_width(target_content_w);
        self.scrolled.set_width_request(target_content_w);

        let target_y = outer_y + outer_h;
        let rect = gtk4::gdk::Rectangle::new(outer_x, target_y, outer_w, 1);
        self.popover.set_pointing_to(Some(&rect));
        self.popover.set_offset(0, 0);
    }

    pub fn show_results(self: &Rc<Self>, mut results: Vec<OmniboxResult>) {
        // Enforce max 5 suggestions for clean, compact display
        results.truncate(5);

        // Clear old widgets
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
        self.row_widgets.borrow_mut().clear();
        self.selected_index.set(None);

        if results.is_empty() {
            *self.results.borrow_mut() = Vec::new();
            self.close();
            return;
        }

        // Lock popover geometry to the exact width and position of the address capsule
        self.update_geometry();

        *self.results.borrow_mut() = results.clone();

        for (idx, item) in results.into_iter().enumerate() {
            let row = GtkBox::new(Orientation::Horizontal, 8);
            row.add_css_class("take-omnibox-row");
            row.set_hexpand(true);

            // Icon (compact 16px)
            let icon_name = match item.item_type {
                OmniboxItemType::Bookmark => "starred-symbolic",
                OmniboxItemType::History => "document-open-recent-symbolic",
            };
            let icon = Image::from_icon_name(icon_name);
            icon.set_pixel_size(16);
            icon.add_css_class(match item.item_type {
                OmniboxItemType::Bookmark => "take-omnibox-icon-bookmark",
                OmniboxItemType::History => "take-omnibox-icon-history",
            });
            row.append(&icon);

            // Text (Title & decoded clean URL)
            let text_box = GtkBox::new(Orientation::Vertical, 1);
            text_box.set_hexpand(true);

            let title_label = Label::builder()
                .label(&item.title)
                .css_classes(["take-omnibox-title"])
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .halign(gtk4::Align::Start)
                .build();
            text_box.append(&title_label);

            let clean_url = glib::Uri::unescape_string(&item.url, None)
                .map(|s| s.to_string())
                .unwrap_or_else(|| item.url.clone());

            let url_label = Label::builder()
                .label(&clean_url)
                .css_classes(["take-omnibox-url"])
                .ellipsize(gtk4::pango::EllipsizeMode::Middle)
                .halign(gtk4::Align::Start)
                .build();
            text_box.append(&url_label);

            row.append(&text_box);

            // Compact badge
            let badge = Label::builder()
                .label(match item.item_type {
                    OmniboxItemType::Bookmark => crate::i18n::tr("omnibox.badge_bookmark"),
                    OmniboxItemType::History => crate::i18n::tr("omnibox.badge_history"),
                })
                .css_classes([
                    "take-omnibox-badge",
                    match item.item_type {
                        OmniboxItemType::Bookmark => "bookmark",
                        OmniboxItemType::History => "history",
                    },
                ])
                .halign(gtk4::Align::End)
                .valign(gtk4::Align::Center)
                .build();
            row.append(&badge);

            // Hover motion controller
            let weak_self = Rc::downgrade(self);
            let motion = EventControllerMotion::new();
            motion.connect_enter(move |_, _, _| {
                if let Some(this) = weak_self.upgrade() {
                    this.highlight_index(Some(idx));
                }
            });
            row.add_controller(motion);

            // Click gesture
            let weak_self = Rc::downgrade(self);
            let target_url = item.url.clone();
            let click = GestureClick::new();
            click.connect_pressed(move |_, _, _, _| {
                if let Some(this) = weak_self.upgrade() {
                    this.close();
                    if let Some(cb) = this.on_select.borrow().as_ref() {
                        cb(target_url.clone());
                    }
                }
            });
            row.add_controller(click);

            self.list_box.append(&row);
            self.row_widgets.borrow_mut().push(row);
        }

        self.parent.add_css_class("take-capsule-attached");
        self.popover.popup();
    }
}
