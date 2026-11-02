// TODO(post-beta): prune the remaining unused public API surface, then drop this allow.
#![allow(dead_code)]

mod browser;
mod privacy;
mod storage;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use ui::BrowserWindow;

const APP_ID: &str = "org.echo_linux.TakeWeb";

fn main() -> glib::ExitCode {
    glib::set_prgname(Some("takeweb"));
    glib::set_application_name("Take Web");

    if std::env::var_os("ECHO_GRAPHICS_REEXEC").is_none() {
        let cfg = browser::performance::calculate_graphics_config();
        if !cfg.is_empty() {
            let exe = std::env::current_exe().expect("current_exe");
            let mut cmd = std::process::Command::new(exe);
            cmd.env("ECHO_GRAPHICS_REEXEC", "1");
            for (k, v) in &cfg {
                cmd.env(k, v);
            }
            cmd.args(std::env::args_os().skip(1));
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                let err = cmd.exec();
                eprintln!("graphics re-exec failed: {err}");
                std::process::exit(1);
            }
            #[cfg(not(unix))]
            {
                let status = cmd.status().expect("re-exec failed");
                std::process::exit(status.code().unwrap_or(1));
            }
        }
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // MUST run before GTK/WebKit init
    browser::performance::apply_tuning();
    browser::performance::configure_graphics_environment();

    let args: Vec<String> = std::env::args().collect();
    let is_test_suite = args.iter().any(|arg| arg == "--test-suite");
    let is_private = args.iter().any(|arg| arg == "--private" || arg == "-p");
    let is_new_window = args.iter().any(|arg| arg == "--new-window");
    let cli_urls: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .cloned()
        .collect();

    let gtk_args: Vec<String> = args
        .into_iter()
        .filter(|arg| {
            arg != "--test-suite" && arg != "--private" && arg != "-p" && arg != "--new-window"
        })
        .collect();

    let mut flags = gtk4::gio::ApplicationFlags::HANDLES_OPEN;
    if is_test_suite || is_new_window {
        flags |= gtk4::gio::ApplicationFlags::NON_UNIQUE;
    }

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();

    app.connect_startup(|_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);
            icon_theme.add_search_path("resources/icons");
            if let Ok(dir) = std::env::current_exe() {
                if let Some(parent) = dir.parent() {
                    icon_theme.add_search_path(parent.join("resources/icons"));
                    icon_theme.add_search_path(parent.join("../resources/icons"));
                    icon_theme.add_search_path(parent.join("../../resources/icons"));
                }
            }
        }
        gtk4::Window::set_default_icon_name("takeweb");
    });

    // The window's signal handlers only hold weak references to its state,
    // so something has to own it for as long as the application runs.
    let browser: Rc<RefCell<Option<BrowserWindow>>> = Rc::new(RefCell::new(None));

    let browser_activate = Rc::clone(&browser);
    let urls_activate = cli_urls.clone();
    app.connect_activate(move |app| {
        let mut slot = browser_activate.borrow_mut();
        if let Some(existing) = slot.as_ref() {
            if is_private {
                existing.enter_private_mode();
            }
            existing.present();
            return;
        }

        let window = BrowserWindow::new(app, is_test_suite, &urls_activate, is_private);
        window.present();
        *slot = Some(window);
    });

    let browser_open = Rc::clone(&browser);
    app.connect_open(move |app, files, _hint| {
        let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
        let mut slot = browser_open.borrow_mut();
        if let Some(existing) = slot.as_ref() {
            for (i, uri) in uris.iter().enumerate() {
                existing.open_tab(uri, i == 0);
            }
            if is_private {
                existing.enter_private_mode();
            }
            existing.present();
            return;
        }

        let window = BrowserWindow::new(app, is_test_suite, &uris, is_private);
        window.present();
        *slot = Some(window);
    });

    app.run_with_args(&gtk_args)
}
