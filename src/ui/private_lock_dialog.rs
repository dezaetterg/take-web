use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation, PasswordEntry, Widget};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::rc::Rc;

pub fn show_setup_password_dialog<F, G>(parent: &impl IsA<Widget>, on_set: F, on_skip: G)
where
    F: Fn(String) + 'static,
    G: Fn() + 'static,
{
    let dialog = adw::AlertDialog::new(
        Some(crate::i18n::tr("private_lock.dialog_title")),
        Some(crate::i18n::tr("private_lock.dialog_message")),
    );
    dialog.add_css_class("take-password-dialog");
    if parent.has_css_class("light-mode") || !adw::StyleManager::default().is_dark() {
        dialog.add_css_class("light-mode");
    }

    let form = GtkBox::new(Orientation::Vertical, 8);
    let entry = PasswordEntry::builder()
        .show_peek_icon(true)
        .placeholder_text(crate::i18n::tr("private_lock.password_optional"))
        .activates_default(true)
        .build();
    form.append(&entry);

    dialog.add_response("cancel", crate::i18n::tr("private_lock.cancel"));
    dialog.add_response("skip", crate::i18n::tr("private_lock.skip"));
    dialog.add_response("set", crate::i18n::tr("private_lock.set_password"));
    dialog.set_response_appearance("set", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("set"));
    dialog.set_close_response("cancel");
    dialog.set_extra_child(Some(&form));

    let on_set = Rc::new(on_set);
    let on_skip = Rc::new(on_skip);

    dialog.connect_response(None, move |_, response| match response {
        "set" => {
            let password = entry.text().trim().to_string();
            if password.is_empty() {
                on_skip();
            } else {
                on_set(password);
            }
        }
        "skip" => {
            on_skip();
        }
        _ => {}
    });

    dialog.present(Some(parent));
}

pub fn show_unlock_password_dialog<C, U, W>(
    parent: &impl IsA<Widget>,
    check_fn: C,
    on_unlock: U,
    on_wipe: W,
) where
    C: Fn(&str) -> bool + 'static,
    U: Fn() + 'static,
    W: Fn() + 'static,
{
    show_unlock_password_dialog_internal(
        parent,
        Rc::new(check_fn),
        Rc::new(on_unlock),
        Rc::new(on_wipe),
        None,
    );
}

fn show_unlock_password_dialog_internal(
    parent: &impl IsA<Widget>,
    check_fn: Rc<dyn Fn(&str) -> bool>,
    on_unlock: Rc<dyn Fn()>,
    on_wipe: Rc<dyn Fn()>,
    error_msg: Option<&str>,
) {
    let dialog = adw::AlertDialog::new(
        Some(crate::i18n::tr("private_lock.unlock_title")),
        Some(crate::i18n::tr("private_lock.unlock_message")),
    );
    dialog.add_css_class("take-password-dialog");
    if parent.has_css_class("light-mode") || !adw::StyleManager::default().is_dark() {
        dialog.add_css_class("light-mode");
    }

    let form = GtkBox::new(Orientation::Vertical, 8);
    let entry = PasswordEntry::builder()
        .show_peek_icon(true)
        .placeholder_text(crate::i18n::tr("private_lock.password_required"))
        .activates_default(true)
        .build();
    form.append(&entry);

    if let Some(msg) = error_msg {
        let err_label = Label::new(Some(msg));
        err_label.add_css_class("error");
        form.append(&err_label);
    }

    dialog.add_response("cancel", crate::i18n::tr("private_lock.cancel"));
    dialog.add_response("wipe", crate::i18n::tr("private_lock.forgot_password"));
    dialog.set_response_appearance("wipe", adw::ResponseAppearance::Destructive);
    dialog.add_response("unlock", crate::i18n::tr("private_lock.unlock"));
    dialog.set_response_appearance("unlock", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("unlock"));
    dialog.set_close_response("cancel");
    dialog.set_extra_child(Some(&form));

    let parent_clone = parent.clone();
    dialog.connect_response(None, move |_, response| match response {
        "unlock" => {
            let password = entry.text();
            if check_fn(&password) {
                on_unlock();
            } else {
                let p = parent_clone.clone();
                let c = Rc::clone(&check_fn);
                let u = Rc::clone(&on_unlock);
                let w = Rc::clone(&on_wipe);
                glib::idle_add_local_once(move || {
                    show_unlock_password_dialog_internal(
                        &p,
                        c,
                        u,
                        w,
                        Some(crate::i18n::tr("private_lock.wrong_password_desc")),
                    );
                });
            }
        }
        "wipe" => {
            on_wipe();
        }
        _ => {}
    });

    dialog.present(Some(parent));
}
