pub mod app_menu;
pub mod downloads_popover;
pub mod find_bar;
pub mod header_bar;
pub mod history_popover;
pub mod omnibox_popup;
pub mod permission_bar;
pub mod private_lock_dialog;
pub mod shield_popover;
pub mod sidebar;
pub mod tab;
pub mod tab_overview_dock;
pub mod window;

#[allow(unused_imports)]
pub use permission_bar::PermissionBar;
pub use window::BrowserWindow;
