use super::Inner;
use libadwaita as adw;
use std::rc::{Rc, Weak};

pub(super) fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|parsed| {
        parsed
            .host_str()
            .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
    })
}

pub(super) fn is_web_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub(super) fn tab_menu(pinned: bool, muted: bool) -> gio::Menu {
    let menu = gio::Menu::new();

    let closing = gio::Menu::new();
    closing.append(Some(crate::i18n::tr("tab.close")), Some("tab.close"));
    closing.append(
        Some(crate::i18n::tr("tab.close_others")),
        Some("tab.close-others"),
    );
    closing.append(
        Some(crate::i18n::tr("tab.close_right")),
        Some("tab.close-right"),
    );
    menu.append_section(None, &closing);

    let managing = gio::Menu::new();
    managing.append(
        Some(if pinned {
            crate::i18n::tr("tab.unpin")
        } else {
            crate::i18n::tr("tab.pin")
        }),
        Some("tab.pin"),
    );
    managing.append(
        Some(crate::i18n::tr("tab.duplicate")),
        Some("tab.duplicate"),
    );
    managing.append(
        Some(if muted {
            crate::i18n::tr("tab.unmute")
        } else {
            crate::i18n::tr("tab.mute")
        }),
        Some("tab.mute"),
    );
    managing.append(Some(crate::i18n::tr("tab.copy_url")), Some("tab.copy-url"));
    menu.append_section(None, &managing);

    menu
}

/// Returns the window state only when `page` is the tab the user is looking at.
pub(super) fn if_selected(
    weak: &Weak<Inner>,
    page: &glib::WeakRef<adw::TabPage>,
) -> Option<Rc<Inner>> {
    let inner = weak.upgrade()?;
    let page = page.upgrade()?;
    (inner.tab_view().selected_page().as_ref() == Some(&page)).then_some(inner)
}

/// Shows or hides the tab sidebar without the 250 ms width animation.
pub(super) fn set_sidebar_visible(split: &adw::OverlaySplitView, show: bool) {
    split.set_collapsed(!show);
}
