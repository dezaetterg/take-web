//! Embedded icon theme ensuring 100% consistent, native GNOME-style symbolic icons
//! across all desktop environments (Sway, Hyprland, i3, KDE Plasma, XFCE, etc.)
//! and minimal Linux distributions without external icon dependencies.

use std::fs;
use std::path::PathBuf;

const TAKEWEB_INDEX_THEME: &str = r#"[Icon Theme]
Name=TakeWeb
Comment=TakeWeb Built-in Icon Theme
Inherits=Adwaita,hicolor
Directories=scalable/actions

[scalable/actions]
Size=16
Context=Actions
Type=Scalable
MinSize=8
MaxSize=512
"#;

static EMBEDDED_ICONS: &[(&str, &str)] = &[
    ("action-unavailable-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/action-unavailable-symbolic.svg")),
    ("application-x-executable-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/application-x-executable-symbolic.svg")),
    ("applications-internet-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/applications-internet-symbolic.svg")),
    ("audio-input-microphone-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/audio-input-microphone-symbolic.svg")),
    ("audio-volume-high-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/audio-volume-high-symbolic.svg")),
    ("audio-volume-muted-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/audio-volume-muted-symbolic.svg")),
    ("bookmark-new-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/bookmark-new-symbolic.svg")),
    ("camera-web-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/camera-web-symbolic.svg")),
    ("channel-insecure-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/channel-insecure-symbolic.svg")),
    ("channel-secure-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/channel-secure-symbolic.svg")),
    ("dialog-error-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/dialog-error-symbolic.svg")),
    ("dialog-question-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/dialog-question-symbolic.svg")),
    ("document-open-recent-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/document-open-recent-symbolic.svg")),
    ("document-open-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/document-open-symbolic.svg")),
    ("document-print-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/document-print-symbolic.svg")),
    ("drive-harddisk-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/drive-harddisk-symbolic.svg")),
    ("edit-delete-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/edit-delete-symbolic.svg")),
    ("edit-find-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/edit-find-symbolic.svg")),
    ("emblem-default-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/emblem-default-symbolic.svg")),
    ("emblem-ok-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/emblem-ok-symbolic.svg")),
    ("find-location-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/find-location-symbolic.svg")),
    ("folder-download-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/folder-download-symbolic.svg")),
    ("folder-open-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/folder-open-symbolic.svg")),
    ("folder-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/folder-symbolic.svg")),
    ("go-down-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/go-down-symbolic.svg")),
    ("go-home-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/go-home-symbolic.svg")),
    ("go-next-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/go-next-symbolic.svg")),
    ("go-previous-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/go-previous-symbolic.svg")),
    ("go-up-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/go-up-symbolic.svg")),
    ("help-about-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/help-about-symbolic.svg")),
    ("input-mouse-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/input-mouse-symbolic.svg")),
    ("list-add-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/list-add-symbolic.svg")),
    ("media-optical-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/media-optical-symbolic.svg")),
    ("network-server-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/network-server-symbolic.svg")),
    ("non-starred-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/non-starred-symbolic.svg")),
    ("object-select-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/object-select-symbolic.svg")),
    ("open-book-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/open-book-symbolic.svg")),
    ("open-menu-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/open-menu-symbolic.svg")),
    ("preferences-desktop-locale-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/preferences-desktop-locale-symbolic.svg")),
    ("preferences-system-notifications-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/preferences-system-notifications-symbolic.svg")),
    ("preferences-system-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/preferences-system-symbolic.svg")),
    ("process-stop-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/process-stop-symbolic.svg")),
    ("security-high-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/security-high-symbolic.svg")),
    ("sidebar-show-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/sidebar-show-symbolic.svg")),
    ("starred-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/starred-symbolic.svg")),
    ("system-search-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/system-search-symbolic.svg")),
    ("tab-new-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/tab-new-symbolic.svg")),
    ("user-trash-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/user-trash-symbolic.svg")),
    ("view-conceal-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-conceal-symbolic.svg")),
    ("view-dual-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-dual-symbolic.svg")),
    ("view-fullscreen-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-fullscreen-symbolic.svg")),
    ("view-grid-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-grid-symbolic.svg")),
    ("view-list-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-list-symbolic.svg")),
    ("view-more-horizontal-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-more-horizontal-symbolic.svg")),
    ("view-paged-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-paged-symbolic.svg")),
    ("view-pin-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-pin-symbolic.svg")),
    ("view-refresh-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/view-refresh-symbolic.svg")),
    ("weather-clear-night-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/weather-clear-night-symbolic.svg")),
    ("web-browser-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/web-browser-symbolic.svg")),
    ("window-close-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/window-close-symbolic.svg")),
    ("window-new-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/window-new-symbolic.svg")),
    ("zoom-in-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/zoom-in-symbolic.svg")),
    ("zoom-out-symbolic.svg", include_str!("../../resources/icons/TakeWeb/scalable/actions/zoom-out-symbolic.svg")),
];

/// Ensures all 63 symbolic icons are extracted to the user data directory
/// if not already present, guaranteeing 100% standalone reliability on minimal systems.
pub fn ensure_icon_theme_extracted() -> PathBuf {
    let user_icons = glib::user_data_dir().join("takeweb/icons");
    let takeweb_theme = user_icons.join("TakeWeb");
    let actions_dir = takeweb_theme.join("scalable/actions");

    // If already fully populated with 63 icons and index.theme, return early
    if actions_dir.exists() {
        if let Ok(entries) = fs::read_dir(&actions_dir) {
            let count = entries.filter_map(|e| e.ok()).count();
            if count >= EMBEDDED_ICONS.len() && takeweb_theme.join("index.theme").exists() {
                return user_icons;
            }
        }
    }

    let _ = fs::create_dir_all(&actions_dir);
    let _ = fs::write(takeweb_theme.join("index.theme"), TAKEWEB_INDEX_THEME);

    for (name, content) in EMBEDDED_ICONS {
        let target = actions_dir.join(name);
        if !target.exists() {
            let _ = fs::write(target, content);
        }
    }

    user_icons
}

/// Configures the GTK4 IconTheme and process-local GtkSettings so that TakeWeb
/// displays identical, crisp GNOME-style symbolic icons on any Linux desktop or WM.
pub fn setup_icon_theme(display: &gtk4::gdk::Display) {
    let extracted_dir = ensure_icon_theme_extracted();
    let icon_theme = gtk4::IconTheme::for_display(display);

    // Register icon search paths in prioritized order:
    // 1. User data cache (extracted embedded icons)
    icon_theme.add_search_path(&extracted_dir);
    // 2. Local dev and relative directory
    icon_theme.add_search_path("resources/icons");
    // 3. System installed location
    icon_theme.add_search_path("/usr/share/takeweb/icons");
    icon_theme.add_search_path("/usr/share/icons");

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            icon_theme.add_search_path(parent.join("resources/icons"));
            icon_theme.add_search_path(parent.join("../resources/icons"));
            icon_theme.add_search_path(parent.join("../../resources/icons"));
            icon_theme.add_search_path(parent.join("../share/takeweb/icons"));
            icon_theme.add_search_path(parent.join("../share/icons"));
        }
    }

    // Explicitly set the application icon theme to "TakeWeb"
    let settings = gtk4::Settings::for_display(display);
    settings.set_gtk_icon_theme_name(Some("TakeWeb"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_icons_count_and_content() {
        assert_eq!(
            EMBEDDED_ICONS.len(),
            63,
            "Must embed all 63 required symbolic icons"
        );
        for (name, svg) in EMBEDDED_ICONS {
            assert!(
                name.ends_with(".svg"),
                "Icon filename must end with .svg: {name}"
            );
            assert!(
                svg.contains("<svg"),
                "Icon content must contain <svg tag: {name}"
            );
            assert!(
                svg.contains("</svg>"),
                "Icon content must contain </svg> closing tag: {name}"
            );
        }
    }

    #[test]
    fn test_ensure_icon_theme_extracted() {
        let path = ensure_icon_theme_extracted();
        let actions = path.join("TakeWeb/scalable/actions");
        assert!(actions.exists());
        assert!(path.join("TakeWeb/index.theme").exists());
        assert!(actions.join("sidebar-show-symbolic.svg").exists());
        assert!(actions.join("open-book-symbolic.svg").exists());
    }
}
