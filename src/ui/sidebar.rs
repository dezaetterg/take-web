use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::gdk::Rectangle;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Expander, GestureClick, Image, Label, Orientation, Popover,
    ScrolledWindow, Spinner,
};
use libadwaita as adw;

use crate::storage::models::{Bookmark, ReadingItem};

#[derive(Clone)]
pub enum SidebarAction {
    NewTab,
    NewGroup,
    AddBookmark,
    SelectTab(adw::TabPage),
    CloseTab(adw::TabPage),
    TabContextMenu(adw::TabPage, Rectangle),
    CycleTabGroup(Vec<adw::TabPage>),
    CloseDuplicateTab(Vec<adw::TabPage>),
    SelectGroup(Option<String>),
    RenameGroup(String),
    DeleteGroup(String),
    CloseGroupTabs(String),
    OpenUrl(String),
    OpenUrlInNewTab(String),
    OpenReadingItem {
        id: i64,
        url: String,
        offline_path: Option<String>,
    },
    ToggleRead {
        id: i64,
        read: bool,
    },
    DeleteReadingItem(i64),
    DeleteBookmark(i64),
    ClosePrivateSession,
}

pub type ActionHandler = Rc<dyn Fn(SidebarAction)>;

pub struct TabEntry {
    pub page: adw::TabPage,
    pub selected: bool,
    pub url: String,
}

pub struct GroupEntry {
    pub id: String,
    pub name: String,
    pub tab_count: usize,
}

#[derive(Clone)]
pub struct BrowserSidebar {
    pub container: ScrolledWindow,
    tabs_box: GtkBox,
    tabs_label: Label,
    private_box: GtkBox,
    groups_section: GtkBox,
    reading_section: GtkBox,
    bookmarks_section: GtkBox,
    groups_box: GtkBox,
    reading_box: GtkBox,
    bookmarks_box: GtkBox,
    handler: Rc<RefCell<Option<ActionHandler>>>,
}

impl BrowserSidebar {
    pub fn new() -> Self {
        let handler: Rc<RefCell<Option<ActionHandler>>> = Rc::new(RefCell::new(None));

        let root = GtkBox::new(Orientation::Vertical, 4);
        root.add_css_class("take-sidebar");

        let (_tabs_section, tabs_box, tabs_label) = Self::section(
            &root,
            crate::i18n::tr("sidebar.open_tabs"),
            Some((
                "tab-new-symbolic",
                crate::i18n::tr("tab.new_tab_title"),
                SidebarAction::NewTab,
            )),
            &handler,
        );

        let private_box = GtkBox::new(Orientation::Vertical, 6);
        private_box.add_css_class("take-sidebar-private-box");
        private_box.set_visible(false);

        let close_session_btn = Button::builder()
            .icon_name("window-close-symbolic")
            .label(crate::i18n::tr("sidebar.end_session"))
            .halign(gtk4::Align::Center)
            .css_classes(["flat", "take-sidebar-close-btn"])
            .tooltip_text(crate::i18n::tr("sidebar.end_session_tooltip"))
            .build();
        let h_close = handler.clone();
        close_session_btn.connect_clicked(move |_| {
            Self::dispatch(&h_close, SidebarAction::ClosePrivateSession);
        });
        private_box.append(&close_session_btn);
        root.append(&private_box);

        let (groups_section, groups_box, _) = Self::section(
            &root,
            crate::i18n::tr("sidebar.tab_groups"),
            Some((
                "list-add-symbolic",
                crate::i18n::tr("tab_groups.new_group"),
                SidebarAction::NewGroup,
            )),
            &handler,
        );
        let (reading_section, reading_box, _) = Self::section(
            &root,
            crate::i18n::tr("sidebar.reading_list"),
            None,
            &handler,
        );
        let (bookmarks_section, bookmarks_box, _) = Self::section(
            &root,
            crate::i18n::tr("sidebar.bookmarks"),
            Some((
                "bookmark-new-symbolic",
                crate::i18n::tr("sidebar.add_bookmark"),
                SidebarAction::AddBookmark,
            )),
            &handler,
        );

        let container = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&root)
            .min_content_width(220)
            .css_classes(["take-sidebar-container"])
            .build();

        Self {
            container,
            tabs_box,
            tabs_label,
            private_box,
            groups_section,
            reading_section,
            bookmarks_section,
            groups_box,
            reading_box,
            bookmarks_box,
            handler,
        }
    }

    pub fn set_anonymous_mode(&self, anonymous: bool) {
        self.groups_section.set_visible(!anonymous);
        self.reading_section.set_visible(!anonymous);
        self.bookmarks_section.set_visible(!anonymous);
        self.private_box.set_visible(anonymous);
        if anonymous {
            self.tabs_label
                .set_text(crate::i18n::tr("tab_overview.private_tabs"));
        } else {
            self.tabs_label
                .set_text(crate::i18n::tr("sidebar.open_tabs"));
        }
    }

    pub fn set_handler(&self, handler: ActionHandler) {
        *self.handler.borrow_mut() = Some(handler);
    }

    fn dispatch(handler: &Rc<RefCell<Option<ActionHandler>>>, action: SidebarAction) {
        let callback = handler.borrow().clone();
        if let Some(callback) = callback {
            callback(action);
        }
    }

    fn section(
        root: &GtkBox,
        title: &str,
        action: Option<(&str, &str, SidebarAction)>,
        handler: &Rc<RefCell<Option<ActionHandler>>>,
    ) -> (GtkBox, GtkBox, Label) {
        let section_box = GtkBox::new(Orientation::Vertical, 2);

        let header = GtkBox::new(Orientation::Horizontal, 4);
        header.add_css_class("take-sidebar-section-header");

        let label = Label::new(Some(title));
        label.add_css_class("take-sidebar-section-title");
        label.set_xalign(0.0);
        label.set_hexpand(true);
        header.append(&label);

        if let Some((icon, tooltip, action)) = action {
            let button = Button::builder()
                .icon_name(icon)
                .tooltip_text(tooltip)
                .css_classes(["flat", "take-section-action"])
                .build();
            let handler = handler.clone();
            button.connect_clicked(move |_| Self::dispatch(&handler, action.clone()));
            header.append(&button);
        }

        let items = GtkBox::new(Orientation::Vertical, 2);
        items.add_css_class("take-sidebar-items");

        section_box.append(&header);
        section_box.append(&items);
        root.append(&section_box);

        (section_box, items, label)
    }

    fn clear(list: &GtkBox) {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
    }

    fn empty_state(list: &GtkBox, text: &str) {
        let label = Label::new(Some(text));
        label.add_css_class("take-sidebar-empty");
        label.set_xalign(0.0);
        label.set_wrap(true);
        list.append(&label);
    }

    fn row_action_button(icon: &str, tooltip: &str) -> Button {
        Button::builder()
            .icon_name(icon)
            .tooltip_text(tooltip)
            .valign(Align::Center)
            .css_classes(["flat", "circular", "take-row-action"])
            .build()
    }

    pub fn render_tabs(&self, entries: &[TabEntry]) {
        Self::clear(&self.tabs_box);

        struct GroupedTab {
            title: String,
            url: String,
            pages: Vec<adw::TabPage>,
            has_selected: bool,
            is_loading: bool,
            is_pinned: bool,
            icon: Option<gtk4::gio::Icon>,
        }

        let mut groups: Vec<GroupedTab> = Vec::new();
        for entry in entries {
            let title = entry.page.title().to_string();
            let url = entry.url.clone();
            if let Some(existing) = groups.iter_mut().find(|g| g.url == url && g.title == title) {
                if entry.selected {
                    existing.has_selected = true;
                }
                if entry.page.is_loading() {
                    existing.is_loading = true;
                }
                if entry.page.is_pinned() {
                    existing.is_pinned = true;
                }
                existing.pages.push(entry.page.clone());
            } else {
                groups.push(GroupedTab {
                    title,
                    url,
                    has_selected: entry.selected,
                    is_loading: entry.page.is_loading(),
                    is_pinned: entry.page.is_pinned(),
                    icon: entry.page.icon(),
                    pages: vec![entry.page.clone()],
                });
            }
        }

        for group in groups {
            let is_duplicate = group.pages.len() > 1;
            let first_page = group.pages[0].clone();

            let row = GtkBox::new(Orientation::Horizontal, 8);
            row.add_css_class("take-sidebar-item");
            if group.has_selected {
                row.add_css_class("active");
            }

            let content = GtkBox::new(Orientation::Horizontal, 8);
            content.set_hexpand(true);

            if group.is_loading {
                let spinner = Spinner::new();
                spinner.set_spinning(true);
                spinner.add_css_class("take-row-icon");
                content.append(&spinner);
            } else {
                let icon = match &group.icon {
                    Some(gicon) => Image::from_gicon(gicon),
                    None => Image::from_icon_name("web-browser-symbolic"),
                };
                icon.add_css_class("take-row-icon");
                content.append(&icon);
            }

            let title_label = Label::new(Some(&group.title));
            title_label.set_xalign(0.0);
            title_label.set_hexpand(true);
            title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            content.append(&title_label);

            if is_duplicate {
                let counter = Label::new(Some(&group.pages.len().to_string()));
                counter.add_css_class("take-duplicate-badge");
                let count_str = group.pages.len().to_string();
                counter.set_tooltip_text(Some(&crate::i18n::tr_fmt(
                    "sidebar.duplicate_tabs_count",
                    &[("count", &count_str)],
                )));
                content.append(&counter);
            }

            if group.is_pinned {
                let pin = Image::from_icon_name("view-pin-symbolic");
                pin.add_css_class("take-pin-mark");
                content.append(&pin);
            }

            let click = GestureClick::new();
            click.set_button(0);
            let handler = self.handler.clone();
            let parent = self.container.clone();
            let row_for_click = row.clone();
            let pages = group.pages.clone();
            let context_page = first_page.clone();

            click.connect_pressed(move |gesture, _, _, _| match gesture.current_button() {
                1 => {
                    if pages.len() > 1 {
                        Self::dispatch(&handler, SidebarAction::CycleTabGroup(pages.clone()));
                    } else {
                        Self::dispatch(&handler, SidebarAction::SelectTab(pages[0].clone()));
                    }
                }
                2 => {
                    if pages.len() > 1 {
                        Self::dispatch(&handler, SidebarAction::CloseDuplicateTab(pages.clone()));
                    } else {
                        Self::dispatch(&handler, SidebarAction::CloseTab(pages[0].clone()));
                    }
                }
                3 => {
                    if let Some(rect) = Self::anchor_rect(&row_for_click, &parent) {
                        Self::dispatch(
                            &handler,
                            SidebarAction::TabContextMenu(context_page.clone(), rect),
                        );
                    }
                }
                _ => {}
            });
            content.add_controller(click);
            row.append(&content);

            let close_tooltip = if is_duplicate {
                crate::i18n::tr("sidebar.close_duplicate_tab")
            } else {
                crate::i18n::tr("tab.close")
            };
            let close = Self::row_action_button("window-close-symbolic", close_tooltip);
            let handler = self.handler.clone();
            let close_pages = group.pages.clone();
            close.connect_clicked(move |_| {
                if close_pages.len() > 1 {
                    Self::dispatch(
                        &handler,
                        SidebarAction::CloseDuplicateTab(close_pages.clone()),
                    );
                } else {
                    Self::dispatch(&handler, SidebarAction::CloseTab(close_pages[0].clone()));
                }
            });
            row.append(&close);

            self.tabs_box.append(&row);
        }
    }

    pub fn render_groups(&self, groups: &[GroupEntry], active: Option<&str>) {
        Self::clear(&self.groups_box);

        let ungrouped = GtkBox::new(Orientation::Horizontal, 8);
        ungrouped.add_css_class("take-sidebar-item");
        if active.is_none() {
            ungrouped.add_css_class("active");
        }
        let all_icon = Image::from_icon_name("view-list-symbolic");
        all_icon.add_css_class("take-row-icon");
        ungrouped.append(&all_icon);
        let all_label = Label::new(Some(crate::i18n::tr("tab_groups.all_tabs_no_group")));
        all_label.set_xalign(0.0);
        all_label.set_hexpand(true);
        all_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        ungrouped.append(&all_label);
        self.connect_row_click(&ungrouped, SidebarAction::SelectGroup(None), None);
        self.groups_box.append(&ungrouped);

        for group in groups {
            let row = GtkBox::new(Orientation::Horizontal, 8);
            row.add_css_class("take-sidebar-item");
            if active == Some(group.id.as_str()) {
                row.add_css_class("active");
            }

            let content = GtkBox::new(Orientation::Horizontal, 8);
            content.set_hexpand(true);

            let icon = Image::from_icon_name("folder-symbolic");
            icon.add_css_class("take-row-icon");
            content.append(&icon);

            let name = Label::new(Some(&group.name));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            content.append(&name);

            let count = Label::new(Some(&group.tab_count.to_string()));
            count.add_css_class("take-count-badge");
            content.append(&count);

            self.connect_row_click(
                &content,
                SidebarAction::SelectGroup(Some(group.id.clone())),
                Some(group.id.clone()),
            );
            row.append(&content);

            let more = Self::row_action_button(
                "view-more-horizontal-symbolic",
                crate::i18n::tr("sidebar.group_actions"),
            );
            let handler = self.handler.clone();
            let parent = self.container.clone();
            let group_id = group.id.clone();
            let row_for_menu = row.clone();
            more.connect_clicked(move |_| {
                Self::popup_group_menu(&parent, &row_for_menu, &group_id, &handler);
            });
            row.append(&more);

            self.groups_box.append(&row);
        }
    }

    fn connect_row_click(&self, row: &GtkBox, primary: SidebarAction, menu_group: Option<String>) {
        let click = GestureClick::new();
        click.set_button(0);
        let handler = self.handler.clone();
        let parent = self.container.clone();
        let row_for_menu = row.clone();
        click.connect_pressed(move |gesture, _, _, _| match gesture.current_button() {
            1 => Self::dispatch(&handler, primary.clone()),
            3 => {
                if let Some(group_id) = &menu_group {
                    Self::popup_group_menu(&parent, &row_for_menu, group_id, &handler);
                }
            }
            _ => {}
        });
        row.add_controller(click);
    }

    pub fn render_reading_list(&self, items: &[ReadingItem]) {
        Self::clear(&self.reading_box);

        if items.is_empty() {
            Self::empty_state(
                &self.reading_box,
                crate::i18n::tr("sidebar.reading_list_empty"),
            );
            return;
        }

        for item in items {
            let row = GtkBox::new(Orientation::Horizontal, 8);
            row.add_css_class("take-reading-card");
            if item.is_read {
                row.add_css_class("read");
            }

            let content = GtkBox::new(Orientation::Horizontal, 8);
            content.set_hexpand(true);
            content.append(
                &crate::browser::favicon::FaviconManager::create_favicon_widget(
                    &item.url,
                    &item.title,
                ),
            );

            let text = GtkBox::new(Orientation::Vertical, 2);
            text.set_hexpand(true);

            let title = Label::new(Some(&item.title));
            title.add_css_class("take-reading-title");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            text.append(&title);

            let domain = Label::new(Some(&item.domain));
            domain.add_css_class("take-reading-domain");
            domain.set_xalign(0.0);
            domain.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            text.append(&domain);

            if let Some(preview) = item.preview_text.as_deref().filter(|p| !p.is_empty()) {
                let preview_label = Label::new(Some(preview));
                preview_label.add_css_class("take-reading-preview");
                preview_label.set_xalign(0.0);
                preview_label.set_wrap(true);
                preview_label.set_lines(2);
                preview_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                preview_label.set_max_width_chars(28);
                text.append(&preview_label);
            }
            content.append(&text);

            let click = GestureClick::new();
            click.set_button(0);
            let handler = self.handler.clone();
            let url = item.url.clone();
            let offline_path = item.offline_path.clone();
            let id = item.id;
            click.connect_pressed(move |gesture, _, _, _| match gesture.current_button() {
                1 => Self::dispatch(
                    &handler,
                    SidebarAction::OpenReadingItem {
                        id,
                        url: url.clone(),
                        offline_path: offline_path.clone(),
                    },
                ),
                2 => Self::dispatch(&handler, SidebarAction::OpenUrlInNewTab(url.clone())),
                _ => {}
            });
            content.add_controller(click);
            row.append(&content);

            let actions = GtkBox::new(Orientation::Vertical, 0);
            actions.set_valign(Align::Start);
            let toggle = Self::row_action_button(
                "object-select-symbolic",
                if item.is_read {
                    crate::i18n::tr("sidebar.mark_unread")
                } else {
                    crate::i18n::tr("sidebar.mark_read")
                },
            );
            let handler = self.handler.clone();
            let (id, read) = (item.id, item.is_read);
            toggle.connect_clicked(move |_| {
                Self::dispatch(&handler, SidebarAction::ToggleRead { id, read: !read });
            });
            let remove = Self::row_action_button(
                "edit-delete-symbolic",
                crate::i18n::tr("sidebar.remove_from_list"),
            );
            let handler = self.handler.clone();
            remove.connect_clicked(move |_| {
                Self::dispatch(&handler, SidebarAction::DeleteReadingItem(id));
            });
            actions.append(&toggle);
            actions.append(&remove);
            row.append(&actions);

            self.reading_box.append(&row);
        }
    }

    pub fn render_bookmarks(&self, bookmarks: &[Bookmark]) {
        Self::clear(&self.bookmarks_box);

        if bookmarks.is_empty() {
            Self::empty_state(
                &self.bookmarks_box,
                crate::i18n::tr("sidebar.bookmarks_empty"),
            );
            return;
        }

        let mut by_folder: BTreeMap<&str, Vec<&Bookmark>> = BTreeMap::new();
        let mut loose: Vec<&Bookmark> = Vec::new();
        for bookmark in bookmarks {
            match bookmark.folder.as_deref().filter(|f| !f.is_empty()) {
                Some(folder) => by_folder.entry(folder).or_default().push(bookmark),
                None => loose.push(bookmark),
            }
        }

        // Favorites feed the start page, so they stay on top regardless of alphabetical order.
        let mut folders: Vec<(&str, Vec<&Bookmark>)> = by_folder.into_iter().collect();
        folders.sort_by_key(|(name, _)| *name != crate::storage::database::FAVORITES_FOLDER);

        for (name, entries) in folders {
            let content = GtkBox::new(Orientation::Vertical, 2);
            content.add_css_class("take-folder-content");
            for bookmark in entries {
                content.append(&self.bookmark_row(bookmark));
            }

            let title = Label::new(Some(name));
            title.add_css_class("take-folder-title");
            let folder_icon = Image::from_icon_name("folder-symbolic");
            let label_box = GtkBox::new(Orientation::Horizontal, 6);
            label_box.append(&folder_icon);
            label_box.append(&title);

            let expander = Expander::new(None);
            expander.set_label_widget(Some(&label_box));
            expander.set_child(Some(&content));
            expander.set_expanded(name == crate::storage::database::FAVORITES_FOLDER);
            expander.add_css_class("take-folder");
            self.bookmarks_box.append(&expander);
        }

        for bookmark in loose {
            self.bookmarks_box.append(&self.bookmark_row(bookmark));
        }
    }

    fn bookmark_row(&self, bookmark: &Bookmark) -> GtkBox {
        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.add_css_class("take-sidebar-item");

        let content = GtkBox::new(Orientation::Horizontal, 8);
        content.set_hexpand(true);
        content.append(
            &crate::browser::favicon::FaviconManager::create_favicon_widget(
                &bookmark.url,
                &bookmark.title,
            ),
        );

        let title = Label::new(Some(&bookmark.title));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_tooltip_text(Some(&bookmark.url));
        content.append(&title);

        let click = GestureClick::new();
        click.set_button(0);
        let handler = self.handler.clone();
        let url = bookmark.url.clone();
        click.connect_pressed(move |gesture, _, _, _| match gesture.current_button() {
            1 => Self::dispatch(&handler, SidebarAction::OpenUrl(url.clone())),
            2 => Self::dispatch(&handler, SidebarAction::OpenUrlInNewTab(url.clone())),
            _ => {}
        });
        content.add_controller(click);
        row.append(&content);

        let remove = Self::row_action_button(
            "edit-delete-symbolic",
            crate::i18n::tr("sidebar.remove_bookmark"),
        );
        let handler = self.handler.clone();
        let id = bookmark.id;
        remove
            .connect_clicked(move |_| Self::dispatch(&handler, SidebarAction::DeleteBookmark(id)));
        row.append(&remove);

        row
    }

    fn anchor_rect(row: &GtkBox, parent: &ScrolledWindow) -> Option<Rectangle> {
        row.compute_bounds(parent).map(|bounds| {
            Rectangle::new(
                bounds.x() as i32,
                bounds.y() as i32,
                bounds.width() as i32,
                bounds.height() as i32,
            )
        })
    }

    /// The popover hangs off the scrolled window, which outlives list rebuilds;
    /// parenting it to a row would leave it dangling when the list is re-rendered.
    fn popup_group_menu(
        parent: &ScrolledWindow,
        row: &GtkBox,
        group_id: &str,
        handler: &Rc<RefCell<Option<ActionHandler>>>,
    ) {
        let Some(rect) = Self::anchor_rect(row, parent) else {
            return;
        };

        let entries = [
            (
                crate::i18n::tr("tab_groups.rename"),
                false,
                SidebarAction::RenameGroup(group_id.to_string()),
            ),
            (
                crate::i18n::tr("sidebar.close_all_group_tabs"),
                false,
                SidebarAction::CloseGroupTabs(group_id.to_string()),
            ),
            (
                crate::i18n::tr("sidebar.delete_group"),
                true,
                SidebarAction::DeleteGroup(group_id.to_string()),
            ),
        ];

        let menu = GtkBox::new(Orientation::Vertical, 2);
        menu.add_css_class("take-context-menu");

        let popover = Popover::new();
        popover.set_autohide(true);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&rect));
        popover.set_child(Some(&menu));
        popover.set_parent(parent);

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

        for (label, destructive, action) in entries {
            let text = Label::new(Some(label));
            text.set_xalign(0.0);
            let button = Button::builder()
                .child(&text)
                .css_classes(["flat", "take-menu-item"])
                .build();
            if destructive {
                button.add_css_class("destructive");
            }
            let handler = handler.clone();
            let popover_for_click = popover.clone();
            button.connect_clicked(move |_| {
                popover_for_click.popdown();
                Self::dispatch(&handler, action.clone());
            });
            menu.append(&button);
        }

        popover.connect_closed(|popover| {
            let popover = popover.clone();
            glib::idle_add_local_once(move || {
                if popover.parent().is_some() {
                    popover.unparent();
                }
            });
        });
        popover.popup();
    }
}
