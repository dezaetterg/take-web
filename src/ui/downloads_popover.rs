use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Image, Label, Orientation, Popover, ProgressBar, ScrolledWindow,
};

use crate::browser::download_manager::{DownloadManager, DownloadStatus};

pub struct DownloadsPopover {
    popover: Popover,
    list_box: GtkBox,
    manager: Rc<DownloadManager>,
}

impl DownloadsPopover {
    pub fn new(manager: Rc<DownloadManager>) -> Rc<Self> {
        let popover = Popover::new();
        popover.set_autohide(true);
        popover.add_css_class("take-downloads-popover");

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
        root.set_size_request(340, -1);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);

        // Header with title and "Open Folder" button
        let header = GtkBox::new(Orientation::Horizontal, 8);
        header.set_hexpand(true);

        let title = Label::builder()
            .label(crate::i18n::tr("downloads.title"))
            .css_classes(["take-downloads-title"])
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        header.append(&title);

        let open_folder_btn = Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text(crate::i18n::tr("downloads.open_folder"))
            .css_classes(["take-nav-button", "take-open-folder-btn"])
            .build();
        open_folder_btn.connect_clicked(|_| {
            let dir = DownloadManager::default_download_dir();
            let uri = format!("file://{}", dir.to_string_lossy());
            let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
        });
        header.append(&open_folder_btn);

        root.append(&header);

        // Scrolled list of downloads
        let list_box = GtkBox::new(Orientation::Vertical, 6);
        list_box.set_hexpand(true);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .max_content_height(380)
            .propagate_natural_height(true)
            .child(&list_box)
            .build();

        root.append(&scrolled);
        popover.set_child(Some(&root));

        let popover_rc = Rc::new(Self {
            popover,
            list_box,
            manager,
        });

        popover_rc.refresh();

        // Listen for download updates and refresh popover UI
        let weak = Rc::downgrade(&popover_rc);
        popover_rc.manager.add_listener(move || {
            if let Some(this) = weak.upgrade() {
                this.refresh();
            }
        });

        popover_rc
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }

    pub fn popup(&self) {
        self.popover.popup();
    }

    pub fn popdown(&self) {
        self.popover.popdown();
    }

    pub fn refresh(&self) {
        // Clear existing rows
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let items = self.manager.items();
        if items.is_empty() {
            let empty_box = GtkBox::new(Orientation::Vertical, 8);
            empty_box.set_halign(gtk4::Align::Center);
            empty_box.set_margin_top(24);
            empty_box.set_margin_bottom(24);

            let empty_icon = Image::from_icon_name("folder-download-symbolic");
            empty_icon.set_pixel_size(32);
            empty_icon.add_css_class("dim-label");
            empty_box.append(&empty_icon);

            let empty_label = Label::new(Some(crate::i18n::tr("downloads.empty")));
            empty_label.add_css_class("dim-label");
            empty_box.append(&empty_label);

            self.list_box.append(&empty_box);
            return;
        }

        // Render each download card (newest first)
        for item_rc in items.iter().rev() {
            let item = item_rc.borrow();
            let card = GtkBox::new(Orientation::Horizontal, 8);
            card.add_css_class("take-download-card");

            let icon = Image::from_icon_name(match item.status {
                DownloadStatus::Finished => "emblem-default-symbolic",
                DownloadStatus::Failed => "dialog-error-symbolic",
                DownloadStatus::Cancelled => "action-unavailable-symbolic",
                DownloadStatus::Downloading => "folder-download-symbolic",
            });
            icon.set_pixel_size(24);
            card.append(&icon);

            let info_box = GtkBox::new(Orientation::Vertical, 3);
            info_box.set_hexpand(true);

            let name_label = Label::builder()
                .label(&item.filename)
                .css_classes(["take-download-filename"])
                .ellipsize(gtk4::pango::EllipsizeMode::Middle)
                .halign(gtk4::Align::Start)
                .build();
            info_box.append(&name_label);

            if item.status == DownloadStatus::Downloading {
                let progress = ProgressBar::builder()
                    .fraction(item.progress)
                    .css_classes(["take-download-progress"])
                    .build();
                info_box.append(&progress);
            }

            let status_text = match item.status {
                DownloadStatus::Downloading => {
                    let rec = Self::format_bytes(item.received_bytes);
                    if item.total_bytes > 0 {
                        let total = Self::format_bytes(item.total_bytes);
                        format!("{rec} {} {total}", crate::i18n::tr("downloads.of"))
                    } else {
                        rec
                    }
                }
                DownloadStatus::Finished => crate::i18n::tr("downloads.finished").to_string(),
                DownloadStatus::Failed => item
                    .error_message
                    .clone()
                    .unwrap_or_else(|| crate::i18n::tr("downloads.error").to_string()),
                DownloadStatus::Cancelled => crate::i18n::tr("downloads.cancelled").to_string(),
            };

            let status_label = Label::builder()
                .label(&status_text)
                .css_classes(["take-download-status"])
                .halign(gtk4::Align::Start)
                .build();
            info_box.append(&status_label);

            card.append(&info_box);

            // Action button
            match item.status {
                DownloadStatus::Downloading => {
                    let cancel_btn = Button::builder()
                        .icon_name("process-stop-symbolic")
                        .tooltip_text(crate::i18n::tr("downloads.cancel"))
                        .css_classes(["take-nav-button", "take-download-action-btn"])
                        .build();
                    let id = item.id;
                    let mgr = self.manager.clone();
                    cancel_btn.connect_clicked(move |_| {
                        mgr.cancel_download(id);
                    });
                    card.append(&cancel_btn);
                }
                DownloadStatus::Finished => {
                    let folder_btn = Button::builder()
                        .icon_name("folder-open-symbolic")
                        .tooltip_text(crate::i18n::tr("downloads.show_in_folder"))
                        .css_classes(["take-nav-button", "take-download-action-btn"])
                        .build();
                    let dest_folder = item
                        .destination
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(DownloadManager::default_download_dir);
                    folder_btn.connect_clicked(move |_| {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(&dest_folder)
                            .spawn();
                    });
                    card.append(&folder_btn);

                    let open_btn = Button::builder()
                        .icon_name("document-open-symbolic")
                        .tooltip_text(crate::i18n::tr("downloads.open_file"))
                        .css_classes(["take-nav-button", "take-download-action-btn"])
                        .build();
                    let dest = item.destination.clone();
                    open_btn.connect_clicked(move |_| {
                        let uri = format!("file://{}", dest.to_string_lossy());
                        let _ = gio::AppInfo::launch_default_for_uri(
                            &uri,
                            None::<&gio::AppLaunchContext>,
                        );
                    });
                    card.append(&open_btn);
                }
                _ => {}
            }

            self.list_box.append(&card);
        }
    }

    fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;

        if bytes >= GB {
            format!(
                "{:.1} {}",
                bytes as f64 / GB as f64,
                crate::i18n::tr("app.unit_gb")
            )
        } else if bytes >= MB {
            format!(
                "{:.1} {}",
                bytes as f64 / MB as f64,
                crate::i18n::tr("app.unit_mb")
            )
        } else if bytes >= KB {
            format!(
                "{:.1} {}",
                bytes as f64 / KB as f64,
                crate::i18n::tr("app.unit_kb")
            )
        } else {
            format!("{bytes} {}", crate::i18n::tr("app.unit_b"))
        }
    }
}
