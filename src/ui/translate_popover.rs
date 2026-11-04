use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DropDown, Label, Orientation, Popover, ProgressBar, Separator,
    StringList, Switch,
};

use crate::browser::translator::{
    format_bytes, is_model_pair_ready, resolve_translation_plan, DownloadProgress,
    SUPPORTED_SOURCE_LANGS, SUPPORTED_TARGET_LANGS,
};
use crate::i18n::tr;

type TranslateActionCallback = Box<dyn Fn(&str, &str)>;
type RestoreActionCallback = Box<dyn Fn()>;
type CancelDownloadCallback = Box<dyn Fn()>;
type DeleteModelCallback = Box<dyn Fn(&str, &str)>;
type ToggleAutoTranslateCallback = Box<dyn Fn(&str, &str, bool)>;
type CheckAutoTranslateCallback = Box<dyn Fn(&str) -> bool>;

pub struct TranslatePopover {
    popover: Popover,
    source_dropdown: DropDown,
    target_dropdown: DropDown,
    model_info_label: Label,
    delete_model_btn: Button,
    download_box: GtkBox,
    progress_bar: ProgressBar,
    progress_label: Label,
    cancel_btn: Button,
    auto_switch: Switch,
    translate_btn: Button,
    restore_btn: Button,
    is_translated: RefCell<bool>,
    is_downloading: RefCell<bool>,
    detected_source_lang: RefCell<Option<String>>,
    is_updating_auto_switch: Cell<bool>,
    check_auto_translate_fn: RefCell<Option<CheckAutoTranslateCallback>>,
    on_translate: RefCell<Option<TranslateActionCallback>>,
    on_restore: RefCell<Option<RestoreActionCallback>>,
    on_cancel_download: RefCell<Option<CancelDownloadCallback>>,
    on_delete_model: RefCell<Option<DeleteModelCallback>>,
    on_toggle_auto: RefCell<Option<ToggleAutoTranslateCallback>>,
}

impl TranslatePopover {
    pub fn new() -> Rc<Self> {
        let popover = Popover::builder()
            .autohide(true)
            .has_arrow(true)
            .css_classes(["take-translate-popover"])
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

        let root = GtkBox::new(Orientation::Vertical, 6);
        root.add_css_class("take-translate-popover-content");

        // --- 1. Header: Clean title + concise subtitle without duplicates or badges ---
        let header_row = GtkBox::new(Orientation::Horizontal, 6);
        header_row.add_css_class("take-translate-header-row");

        let title_col = GtkBox::new(Orientation::Vertical, 1);
        title_col.set_hexpand(true);

        let title_label = Label::builder()
            .label(tr("translate_title"))
            .css_classes(["take-translate-title"])
            .xalign(0.0)
            .build();

        let subtitle_label = Label::builder()
            .label(tr("translate_subtitle"))
            .css_classes(["take-translate-subtitle"])
            .xalign(0.0)
            .build();

        title_col.append(&title_label);
        title_col.append(&subtitle_label);
        header_row.append(&title_col);
        root.append(&header_row);

        // --- 2. Source & Target Language Selectors ---
        let lang_grid = GtkBox::new(Orientation::Vertical, 5);
        lang_grid.add_css_class("take-translate-lang-section");

        let source_row = GtkBox::new(Orientation::Horizontal, 6);
        source_row.set_valign(Align::Center);
        let source_label = Label::builder()
            .label(tr("translate_source_lang"))
            .css_classes(["take-translate-field-label"])
            .xalign(0.0)
            .width_chars(6)
            .build();

        let source_strings: Vec<&str> = SUPPORTED_SOURCE_LANGS
            .iter()
            .map(|(_, label_key)| tr(label_key))
            .collect();
        let source_model = StringList::new(&source_strings);
        let source_dropdown = DropDown::builder()
            .model(&source_model)
            .selected(0)
            .hexpand(true)
            .css_classes(["take-translate-dropdown"])
            .build();

        source_row.append(&source_label);
        source_row.append(&source_dropdown);

        let target_row = GtkBox::new(Orientation::Horizontal, 6);
        target_row.set_valign(Align::Center);
        let target_label = Label::builder()
            .label(tr("translate_target_lang"))
            .css_classes(["take-translate-field-label"])
            .xalign(0.0)
            .width_chars(6)
            .build();

        let target_strings: Vec<&str> = SUPPORTED_TARGET_LANGS
            .iter()
            .map(|(_, label_key)| tr(label_key))
            .collect();
        let target_model = StringList::new(&target_strings);
        let target_dropdown = DropDown::builder()
            .model(&target_model)
            .selected(0)
            .hexpand(true)
            .css_classes(["take-translate-dropdown"])
            .build();

        target_row.append(&target_label);
        target_row.append(&target_dropdown);

        lang_grid.append(&source_row);
        lang_grid.append(&target_row);
        root.append(&lang_grid);

        // --- 3. Model Cache Info Row + Delete Model Button ---
        let model_row = GtkBox::new(Orientation::Horizontal, 6);
        model_row.set_valign(Align::Center);
        model_row.add_css_class("take-translate-model-row");

        let model_info_label = Label::builder()
            .label("")
            .css_classes(["take-translate-hint-label"])
            .xalign(0.0)
            .hexpand(true)
            .wrap(true)
            .build();

        let delete_model_btn = Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(tr("translate_action_delete_model"))
            .css_classes(["flat", "circular", "take-translate-delete-btn"])
            .visible(false)
            .build();

        model_row.append(&model_info_label);
        model_row.append(&delete_model_btn);
        root.append(&model_row);

        // --- 4. Download Progress Bar + Cancel Button ---
        let download_box = GtkBox::new(Orientation::Vertical, 4);
        download_box.add_css_class("take-translate-download-box");
        download_box.set_visible(false);

        let progress_bar = ProgressBar::builder()
            .fraction(0.0)
            .show_text(false)
            .css_classes(["take-translate-progress-bar"])
            .build();

        let dl_bottom_row = GtkBox::new(Orientation::Horizontal, 6);
        dl_bottom_row.set_valign(Align::Center);

        let progress_label = Label::builder()
            .label("0%")
            .css_classes(["take-translate-hint-label"])
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();

        let cancel_btn = Button::builder()
            .label(tr("translate_action_cancel"))
            .css_classes(["flat", "take-translate-secondary-btn"])
            .build();

        dl_bottom_row.append(&progress_label);
        dl_bottom_row.append(&cancel_btn);

        download_box.append(&progress_bar);
        download_box.append(&dl_bottom_row);
        root.append(&download_box);

        // --- 5. Action Buttons (Translate / Show Original) ---
        let actions_row = GtkBox::new(Orientation::Horizontal, 6);
        actions_row.add_css_class("take-translate-actions-row");

        let restore_btn = Button::builder()
            .label(tr("translate_action_restore"))
            .css_classes(["flat", "take-translate-secondary-btn"])
            .hexpand(true)
            .sensitive(false)
            .build();

        let translate_btn = Button::builder()
            .label(tr("translate_action_translate"))
            .css_classes(["take-translate-primary-btn"])
            .hexpand(true)
            .build();

        actions_row.append(&restore_btn);
        actions_row.append(&translate_btn);
        root.append(&actions_row);

        root.append(&Separator::new(Orientation::Horizontal));

        // --- 6. Auto-translate toggle ---
        let auto_row = GtkBox::new(Orientation::Horizontal, 6);
        auto_row.set_valign(Align::Center);
        auto_row.add_css_class("take-translate-auto-row");

        let auto_label = Label::builder()
            .label(tr("translate_auto_switch"))
            .css_classes(["take-translate-auto-label"])
            .hexpand(true)
            .wrap(true)
            .xalign(0.0)
            .build();

        let auto_switch = Switch::builder()
            .active(false)
            .valign(Align::Center)
            .css_classes(["take-translate-switch"])
            .build();

        auto_row.append(&auto_label);
        auto_row.append(&auto_switch);
        root.append(&auto_row);

        popover.set_child(Some(&root));

        let instance = Rc::new(Self {
            popover,
            source_dropdown,
            target_dropdown,
            model_info_label,
            delete_model_btn,
            download_box,
            progress_bar,
            progress_label,
            cancel_btn,
            auto_switch,
            translate_btn,
            restore_btn,
            is_translated: RefCell::new(false),
            is_downloading: RefCell::new(false),
            detected_source_lang: RefCell::new(None),
            is_updating_auto_switch: Cell::new(false),
            check_auto_translate_fn: RefCell::new(None),
            on_translate: RefCell::new(None),
            on_restore: RefCell::new(None),
            on_cancel_download: RefCell::new(None),
            on_delete_model: RefCell::new(None),
            on_toggle_auto: RefCell::new(None),
        });

        instance.update_model_status_ui();

        {
            let this_weak = Rc::downgrade(&instance);
            instance.source_dropdown.connect_selected_notify(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    this.update_model_status_ui();
                }
            });
        }

        {
            let this_weak = Rc::downgrade(&instance);
            instance.target_dropdown.connect_selected_notify(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    this.update_model_status_ui();
                }
            });
        }

        {
            let this_weak = Rc::downgrade(&instance);
            instance.translate_btn.connect_clicked(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    let src_code = this.selected_source_code();
                    let tgt_code = this.selected_target_code();
                    if let Some(ref cb) = *this.on_translate.borrow() {
                        cb(src_code, tgt_code);
                    }
                }
            });
        }

        {
            let this_weak = Rc::downgrade(&instance);
            instance.restore_btn.connect_clicked(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    *this.is_translated.borrow_mut() = false;
                    this.restore_btn.set_sensitive(false);
                    if let Some(ref cb) = *this.on_restore.borrow() {
                        cb();
                    }
                }
            });
        }

        {
            let this_weak = Rc::downgrade(&instance);
            instance.cancel_btn.connect_clicked(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    *this.is_downloading.borrow_mut() = false;
                    this.download_box.set_visible(false);
                    this.translate_btn.set_sensitive(true);
                    if let Some(ref cb) = *this.on_cancel_download.borrow() {
                        cb();
                    }
                }
            });
        }

        {
            let this_weak = Rc::downgrade(&instance);
            instance.delete_model_btn.connect_clicked(move |_| {
                if let Some(this) = this_weak.upgrade() {
                    let src_code = this.selected_source_code();
                    let tgt_code = this.selected_target_code();
                    if let Some(ref cb) = *this.on_delete_model.borrow() {
                        cb(src_code, tgt_code);
                    }
                    this.update_model_status_ui();
                }
            });
        }

        {
            let this_weak = Rc::downgrade(&instance);
            instance.auto_switch.connect_state_set(move |_, active| {
                if let Some(this) = this_weak.upgrade() {
                    if !this.is_updating_auto_switch.get() {
                        let src = this.effective_source_code();
                        let tgt = this.selected_target_code();
                        if let Some(ref cb) = *this.on_toggle_auto.borrow() {
                            cb(&src, tgt, active);
                        }
                    }
                }
                glib::Propagation::Proceed
            });
        }

        instance
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }

    pub fn popup(&self) {
        self.update_model_status_ui();
        self.popover.popup();
    }

    pub fn popdown(&self) {
        self.popover.popdown();
    }

    pub fn set_check_auto_translate_fn<F: Fn(&str) -> bool + 'static>(&self, f: F) {
        *self.check_auto_translate_fn.borrow_mut() = Some(Box::new(f));
        self.sync_auto_switch();
    }

    pub fn set_detected_language(&self, lang: Option<&str>) {
        *self.detected_source_lang.borrow_mut() = lang.map(|s| s.to_string());
        self.update_model_status_ui();
    }

    pub fn effective_source_code(&self) -> String {
        let selected = self.selected_source_code();
        if selected == "auto" {
            if let Some(ref d) = *self.detected_source_lang.borrow() {
                if !d.is_empty() {
                    return d.clone();
                }
            }
            return "en".to_string();
        }
        selected.to_string()
    }

    pub fn sync_auto_switch(&self) {
        let src = self.effective_source_code();
        if let Some(ref checker) = *self.check_auto_translate_fn.borrow() {
            let is_on = checker(&src);
            self.is_updating_auto_switch.set(true);
            self.auto_switch.set_active(is_on);
            self.is_updating_auto_switch.set(false);
        }
    }

    pub fn selected_source_code(&self) -> &'static str {
        let idx = self.source_dropdown.selected() as usize;
        SUPPORTED_SOURCE_LANGS
            .get(idx)
            .map(|(code, _)| *code)
            .unwrap_or("auto")
    }

    pub fn selected_target_code(&self) -> &'static str {
        let idx = self.target_dropdown.selected() as usize;
        SUPPORTED_TARGET_LANGS
            .get(idx)
            .map(|(code, _)| *code)
            .unwrap_or("ru")
    }

    pub fn update_model_status_ui(&self) {
        let src = self.selected_source_code();
        let tgt = self.selected_target_code();

        if let Some(plan) = resolve_translation_plan(src, tgt) {
            let pairs_str = plan.pairs.join(" + ");
            let pivot_prefix = if plan.is_pivot {
                format!("{} • ", tr("translate_pivot_note"))
            } else {
                String::new()
            };

            let any_installed = plan.pairs.iter().any(|p| is_model_pair_ready(p));
            self.delete_model_btn.set_visible(any_installed);

            if plan.is_ready_on_disk() {
                self.model_info_label.set_label(&format!(
                    "{pivot_prefix}{} ({pairs_str}, {})",
                    tr("translate_model_cached_badge"),
                    format_bytes(plan.total_bytes)
                ));
            } else {
                self.model_info_label.set_label(&format!(
                    "{pivot_prefix}{} ({pairs_str}, ~{})",
                    tr("translate_model_download_needed"),
                    format_bytes(plan.missing_bytes)
                ));
            }
        } else {
            self.model_info_label.set_label("");
            self.delete_model_btn.set_visible(false);
        }

        self.sync_auto_switch();
    }

    pub fn show_download_progress(&self, progress: &DownloadProgress) {
        *self.is_downloading.borrow_mut() = true;
        self.download_box.set_visible(true);
        self.translate_btn.set_sensitive(false);
        self.progress_bar.set_fraction(progress.fraction);
        let pct = (progress.fraction * 100.0).round() as u32;
        self.progress_label.set_label(&format!(
            "{} / {} ({pct}%) • {}",
            format_bytes(progress.downloaded_bytes),
            format_bytes(progress.total_bytes),
            progress.current_file
        ));
    }

    pub fn mark_loading_wasm(&self) {
        *self.is_downloading.borrow_mut() = false;
        self.download_box.set_visible(false);
        self.translate_btn.set_sensitive(true);
        self.restore_btn.set_sensitive(true);
        self.update_model_status_ui();
    }

    pub fn show_download_error(&self, err_msg: &str) {
        *self.is_downloading.borrow_mut() = false;
        self.download_box.set_visible(false);
        self.translate_btn.set_sensitive(true);
        self.model_info_label.set_label(err_msg);
        self.update_model_status_ui();
    }

    pub fn mark_translated(&self) {
        *self.is_downloading.borrow_mut() = false;
        *self.is_translated.borrow_mut() = true;
        self.download_box.set_visible(false);
        self.translate_btn.set_sensitive(true);
        self.restore_btn.set_sensitive(true);
        self.update_model_status_ui();
    }

    pub fn reset_for_navigation(&self) {
        *self.is_translated.borrow_mut() = false;
        self.restore_btn.set_sensitive(false);
        self.update_model_status_ui();
    }

    pub fn set_on_translate<F: Fn(&str, &str) + 'static>(&self, f: F) {
        *self.on_translate.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_restore<F: Fn() + 'static>(&self, f: F) {
        *self.on_restore.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_cancel_download<F: Fn() + 'static>(&self, f: F) {
        *self.on_cancel_download.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_delete_model<F: Fn(&str, &str) + 'static>(&self, f: F) {
        *self.on_delete_model.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_toggle_auto<F: Fn(&str, &str, bool) + 'static>(&self, f: F) {
        *self.on_toggle_auto.borrow_mut() = Some(Box::new(f));
    }
}
