mod browser;
mod i18n;
mod privacy;
mod storage;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use ui::BrowserWindow;

const APP_ID: &str = "io.github.dezaetterg.TakeWeb";

fn main() -> glib::ExitCode {
    if std::env::args().any(|arg| arg == "--version" || arg == "-v") {
        println!("takeweb {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }

    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "TakeWeb {} — Fast, privacy-focused open-source web browser for Linux\n",
            env!("CARGO_PKG_VERSION")
        );
        println!("Usage:\n  takeweb [OPTIONS] [URL...]\n");
        println!("Options:");
        println!("  -h, --help          Show this help message");
        println!("  -v, --version       Show version information");
        println!("  -p, --private       Open a private browsing window");
        println!("  --new-window        Open a new browser window");
        println!("  --app=<URL>         Run web app in standalone chromeless mode");
        println!("  --test-suite        Run built-in browser test suite\n");
        return glib::ExitCode::SUCCESS;
    }

    glib::set_prgname(Some("takeweb"));
    glib::set_application_name("Take Web");

    if std::env::var_os("TAKEWEB_GRAPHICS_REEXEC").is_none() {
        let cfg = browser::performance::calculate_graphics_config();
        if !cfg.is_empty() {
            if let Ok(exe) = std::env::current_exe() {
                let mut cmd = std::process::Command::new(exe);
                cmd.env("TAKEWEB_GRAPHICS_REEXEC", "1");
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
                    match cmd.status() {
                        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
                        Err(err) => {
                            eprintln!("graphics re-exec failed: {err}");
                            std::process::exit(1);
                        }
                    }
                }
            } else {
                eprintln!("warning: failed to resolve current_exe for graphics re-exec");
            }
        }
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Initialize locale based on saved user preference or system environment
    let db_path = glib::user_data_dir().join("takeweb/browser.db");
    let configured_lang = if db_path.exists() {
        storage::database::BrowserDatabase::open(&db_path)
            .ok()
            .and_then(|db| db.get_setting("ui_language").ok().flatten())
    } else {
        None
    };
    i18n::init_locale(configured_lang.as_deref());

    // MUST run before GTK/WebKit init
    browser::performance::apply_tuning();
    browser::performance::configure_graphics_environment();

    let args: Vec<String> = std::env::args().collect();
    let is_test_suite = args.iter().any(|arg| arg == "--test-suite");
    let is_private = args.iter().any(|arg| arg == "--private" || arg == "-p");
    let is_new_window = args.iter().any(|arg| arg == "--new-window");
    let app_url = args.iter().enumerate().find_map(|(i, arg)| {
        if let Some(url) = arg.strip_prefix("--app=") {
            Some(url.to_string())
        } else if arg == "--app" {
            args.get(i + 1).cloned()
        } else {
            None
        }
    });

    let cli_urls: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .cloned()
        .collect();

    let mut skip_next = false;
    let mut gtk_args = Vec::new();
    for arg in &args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--app" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--app=")
            || arg == "--test-suite"
            || arg == "--private"
            || arg == "-p"
            || arg == "--new-window"
        {
            continue;
        }
        gtk_args.push(arg.clone());
    }

    let mut flags = gtk4::gio::ApplicationFlags::HANDLES_OPEN;
    if is_test_suite || is_new_window || app_url.is_some() {
        flags |= gtk4::gio::ApplicationFlags::NON_UNIQUE;
    }

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();

    app.connect_startup(|_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            crate::ui::icons::setup_icon_theme(&display);
        }
        gtk4::Window::set_default_icon_name("takeweb");
    });

    // The window's signal handlers only hold weak references to its state,
    // so something has to own it for as long as the application runs.
    let browser: Rc<RefCell<Option<BrowserWindow>>> = Rc::new(RefCell::new(None));

    let browser_activate = Rc::clone(&browser);
    let urls_activate = cli_urls.clone();
    let app_url_activate = app_url.clone();
    app.connect_activate(move |app| {
        let mut slot = browser_activate.borrow_mut();
        if let Some(existing) = slot.as_ref() {
            if is_private {
                existing.enter_private_mode();
            }
            existing.present();
            return;
        }

        let window = if let Some(ref target_url) = app_url_activate {
            BrowserWindow::new_app_mode(app, target_url)
        } else {
            BrowserWindow::new(app, is_test_suite, &urls_activate, is_private)
        };
        window.present();
        *slot = Some(window);
    });

    let browser_open = Rc::clone(&browser);
    let app_url_open = app_url.clone();
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

        let window = if let Some(ref target_url) = app_url_open {
            BrowserWindow::new_app_mode(app, target_url)
        } else {
            BrowserWindow::new(app, is_test_suite, &uris, is_private)
        };
        window.present();
        *slot = Some(window);
    });

    app.run_with_args(&gtk_args)
}
