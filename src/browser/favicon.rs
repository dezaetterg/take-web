use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use gio::prelude::*;
use gtk4::gdk;
use gtk4::prelude::*;

pub struct FaviconManager {
    cache: Arc<Mutex<HashMap<String, gdk::Texture>>>,
    dir: PathBuf,
}

static INSTANCE: OnceLock<FaviconManager> = OnceLock::new();

impl FaviconManager {
    pub fn global() -> &'static Self {
        INSTANCE.get_or_init(|| {
            let dir = glib::user_data_dir().join("echo-browser").join("favicons");
            let _ = fs::create_dir_all(&dir);
            Self {
                cache: Arc::new(Mutex::new(HashMap::new())),
                dir,
            }
        })
    }

    pub fn extract_domain(url_or_domain: &str) -> String {
        let trimmed = url_or_domain.trim();
        if let Ok(parsed) = url::Url::parse(trimmed) {
            if let Some(host) = parsed.host_str() {
                return host.trim_start_matches("www.").to_lowercase();
            }
        }
        let clean = trimmed
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.")
            .split('/')
            .next()
            .unwrap_or(trimmed)
            .split(':')
            .next()
            .unwrap_or(trimmed);
        clean.to_lowercase()
    }

    pub fn get_cached(&self, domain: &str) -> Option<gdk::Texture> {
        let d = domain.trim_start_matches("www.").to_lowercase();
        if d.is_empty() {
            return None;
        }
        if let Ok(cache) = self.cache.lock() {
            if let Some(tex) = cache.get(&d) {
                return Some(tex.clone());
            }
        }
        let file_path = self.dir.join(format!("{}.png", d));
        if file_path.exists() {
            let gfile = gio::File::for_path(&file_path);
            if let Ok(texture) = gdk::Texture::from_file(&gfile) {
                if let Ok(mut cache) = self.cache.lock() {
                    cache.insert(d, texture.clone());
                }
                return Some(texture);
            }
        }
        None
    }

    pub fn save_texture(&self, domain: &str, texture: &gdk::Texture) {
        let d = domain.trim_start_matches("www.").to_lowercase();
        if d.is_empty() || d == "localhost" || d.starts_with("echo:") || d.starts_with("about:") {
            return;
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(d.clone(), texture.clone());
        }
        let file_path = self.dir.join(format!("{}.png", d));
        if !file_path.exists() {
            let _ = texture.save_to_png(&file_path);
        }
    }

    pub fn create_favicon_widget(url_or_domain: &str, _title: &str) -> gtk4::Widget {
        let domain = Self::extract_domain(url_or_domain);
        let image = gtk4::Image::new();
        image.set_pixel_size(18);
        image.add_css_class("echo-favicon-img");

        if let Some(texture) = Self::global().get_cached(&domain) {
            image.set_paintable(Some(&texture));
            return image.upcast();
        }

        image.set_icon_name(Some("web-browser-symbolic"));
        image.upcast()
    }
}
