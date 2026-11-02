use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Image, Label, Orientation, Popover, Separator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenuAction {
    NewTab,
    NewWindow,
    RestoreSession,
    OpenBookmarks,
    OpenHistory,
    OpenDownloads,
    OpenReadingList,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleFullscreen,
    FindInPage,
    Print,
    ToggleTheme,
    OpenSettings,
    OpenPrivacyReport,
    OpenAbout,
}

pub type AppMenuCallback = Rc<dyn Fn(AppMenuAction)>;

pub struct AppMenu {
    pub popover: Popover,
    zoom_label: Label,
}

impl AppMenu {
    pub fn new(on_action: AppMenuCallback) -> Rc<Self> {
        let popover = Popover::new();
        popover.set_autohide(true);
        popover.add_css_class("echo-app-menu-popover");

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

        let root = GtkBox::new(Orientation::Vertical, 2);
        root.set_size_request(280, -1);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);

        // Section 1: Tabs & Windows
        root.append(&Self::menu_item(
            "tab-new-symbolic",
            "Новая вкладка",
            Some("Ctrl+T"),
            AppMenuAction::NewTab,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "window-new-symbolic",
            "Новое окно",
            Some("Ctrl+N"),
            AppMenuAction::NewWindow,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "view-refresh-symbolic",
            "Восстановить сессию",
            None,
            AppMenuAction::RestoreSession,
            &on_action,
            &popover,
        ));

        root.append(&Self::separator());

        // Section 2: Library
        root.append(&Self::menu_item(
            "view-paged-symbolic",
            "Закладки",
            Some("Ctrl+B"),
            AppMenuAction::OpenBookmarks,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "document-open-recent-symbolic",
            "История",
            Some("Ctrl+H"),
            AppMenuAction::OpenHistory,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "folder-download-symbolic",
            "Загрузки",
            Some("Ctrl+J"),
            AppMenuAction::OpenDownloads,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "bookmark-new-symbolic",
            "Список для чтения",
            Some("Ctrl+Shift+D"),
            AppMenuAction::OpenReadingList,
            &on_action,
            &popover,
        ));

        root.append(&Self::separator());

        // Section 3: Zoom & View
        let (zoom_box, zoom_label) = Self::zoom_row(&on_action, &popover);
        root.append(&zoom_box);

        root.append(&Self::separator());

        // Section 4: Tools
        root.append(&Self::menu_item(
            "edit-find-symbolic",
            "Найти на странице…",
            Some("Ctrl+F"),
            AppMenuAction::FindInPage,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "document-print-symbolic",
            "Печать…",
            Some("Ctrl+P"),
            AppMenuAction::Print,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "security-high-symbolic",
            "Отчет о трекерах и защите",
            None,
            AppMenuAction::OpenPrivacyReport,
            &on_action,
            &popover,
        ));

        root.append(&Self::separator());

        // Section 5: Settings & About
        root.append(&Self::menu_item(
            "weather-clear-night-symbolic",
            "Сменить тему",
            None,
            AppMenuAction::ToggleTheme,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "preferences-system-symbolic",
            "Настройки",
            Some("Ctrl+,"),
            AppMenuAction::OpenSettings,
            &on_action,
            &popover,
        ));
        root.append(&Self::menu_item(
            "help-about-symbolic",
            "О браузере Take Web",
            None,
            AppMenuAction::OpenAbout,
            &on_action,
            &popover,
        ));

        popover.set_child(Some(&root));

        Rc::new(Self {
            popover,
            zoom_label,
        })
    }

    fn separator() -> Separator {
        let sep = Separator::new(Orientation::Horizontal);
        sep.set_margin_top(4);
        sep.set_margin_bottom(4);
        sep.add_css_class("echo-menu-separator");
        sep
    }

    fn menu_item(
        icon: &str,
        title: &str,
        accel: Option<&str>,
        action: AppMenuAction,
        on_action: &AppMenuCallback,
        popover: &Popover,
    ) -> Button {
        let content = GtkBox::new(Orientation::Horizontal, 10);
        content.set_hexpand(true);

        let img = Image::from_icon_name(icon);
        img.add_css_class("echo-menu-icon");
        content.append(&img);

        let label = Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["echo-menu-label"])
            .build();
        content.append(&label);

        if let Some(acc) = accel {
            let accel_label = Label::builder()
                .label(acc)
                .xalign(1.0)
                .css_classes(["echo-menu-accel", "dim-label"])
                .build();
            content.append(&accel_label);
        }

        let button = Button::builder()
            .child(&content)
            .css_classes(["flat", "echo-app-menu-row"])
            .build();

        let cb = on_action.clone();
        let pop = popover.clone();
        button.connect_clicked(move |_| {
            pop.popdown();
            cb(action);
        });

        button
    }

    fn zoom_row(on_action: &AppMenuCallback, popover: &Popover) -> (GtkBox, Label) {
        let root = GtkBox::new(Orientation::Horizontal, 6);
        root.add_css_class("echo-menu-zoom-row");
        root.set_margin_start(8);
        root.set_margin_end(8);
        root.set_margin_top(3);
        root.set_margin_bottom(3);

        let title = Label::builder()
            .label("Масштаб")
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["echo-menu-label"])
            .build();
        root.append(&title);

        let controls = GtkBox::new(Orientation::Horizontal, 2);
        controls.add_css_class("echo-menu-zoom-controls");

        let zoom_out_btn = Button::builder()
            .icon_name("zoom-out-symbolic")
            .tooltip_text("Уменьшить (Ctrl+-)")
            .css_classes(["flat", "circular", "echo-menu-zoom-btn"])
            .build();
        let cb = on_action.clone();
        zoom_out_btn.connect_clicked(move |_| {
            cb(AppMenuAction::ZoomOut);
        });
        controls.append(&zoom_out_btn);

        let zoom_label = Label::builder()
            .label("100%")
            .css_classes(["echo-menu-zoom-label"])
            .width_chars(5)
            .build();

        let reset_btn = Button::builder()
            .child(&zoom_label)
            .tooltip_text("Сбросить масштаб (Ctrl+0)")
            .css_classes(["flat", "echo-menu-zoom-reset-btn"])
            .build();
        let cb = on_action.clone();
        reset_btn.connect_clicked(move |_| {
            cb(AppMenuAction::ZoomReset);
        });
        controls.append(&reset_btn);

        let zoom_in_btn = Button::builder()
            .icon_name("zoom-in-symbolic")
            .tooltip_text("Увеличить (Ctrl++)")
            .css_classes(["flat", "circular", "echo-menu-zoom-btn"])
            .build();
        let cb = on_action.clone();
        zoom_in_btn.connect_clicked(move |_| {
            cb(AppMenuAction::ZoomIn);
        });
        controls.append(&zoom_in_btn);

        let fs_btn = Button::builder()
            .icon_name("view-fullscreen-symbolic")
            .tooltip_text("Полноэкранный режим (F11)")
            .css_classes(["flat", "circular", "echo-menu-fullscreen-btn"])
            .build();
        let cb = on_action.clone();
        let pop = popover.clone();
        fs_btn.connect_clicked(move |_| {
            pop.popdown();
            cb(AppMenuAction::ToggleFullscreen);
        });
        controls.append(&fs_btn);

        root.append(&controls);
        (root, zoom_label)
    }

    pub fn set_zoom_text(&self, text: &str) {
        self.zoom_label.set_label(text);
    }
}
