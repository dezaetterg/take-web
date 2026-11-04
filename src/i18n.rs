use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum UiLanguage {
    Ru = 0,
    En = 1,
}

impl UiLanguage {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Ru => "ru",
            Self::En => "en",
        }
    }

    pub fn accept_language_header(self) -> &'static str {
        match self {
            Self::En => "en-US,en;q=0.9",
            Self::Ru => "ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7",
        }
    }

    pub fn webkit_preferred_languages(self) -> &'static [&'static str] {
        match self {
            Self::En => &["en-US", "en"],
            Self::Ru => &["ru-RU", "ru", "en-US", "en"],
        }
    }

    pub fn spell_checking_languages(self) -> &'static [&'static str] {
        match self {
            Self::En => &["en_US"],
            Self::Ru => &["ru_RU", "en_US"],
        }
    }
}

pub static EN_JSON: &str = include_str!("../resources/locales/en.json");
pub static RU_JSON: &str = include_str!("../resources/locales/ru.json");

static CURRENT_LANG: AtomicU8 = AtomicU8::new(UiLanguage::En as u8);

struct LocaleStore {
    en_map: HashMap<&'static str, &'static str>,
    ru_map: HashMap<&'static str, &'static str>,
}

static STORE: OnceLock<LocaleStore> = OnceLock::new();

fn intern_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn flatten_json(
    val: &serde_json::Value,
    prefix: String,
    out: &mut HashMap<&'static str, &'static str>,
) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let next_prefix = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };

                // Also store single underscore variant if top-level section: e.g. "translate_action_translate"
                if !prefix.is_empty() && !prefix.contains('.') {
                    let underscore_key = format!("{prefix}_{k}");
                    if let serde_json::Value::String(s) = v {
                        out.insert(intern_str(underscore_key), intern_str(s.clone()));
                    }
                }

                flatten_json(v, next_prefix, out);
            }
        }
        serde_json::Value::String(s) => {
            let key_static = intern_str(prefix);
            let val_static = intern_str(s.clone());
            out.insert(key_static, val_static);
        }
        _ => {}
    }
}

fn init_store() -> LocaleStore {
    let mut en_map = HashMap::new();
    let mut ru_map = HashMap::new();

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(EN_JSON) {
        flatten_json(&val, String::new(), &mut en_map);
    }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(RU_JSON) {
        flatten_json(&val, String::new(), &mut ru_map);
    }

    LocaleStore { en_map, ru_map }
}

fn get_store() -> &'static LocaleStore {
    STORE.get_or_init(init_store)
}

/// Resolves target UI language from configured preference and an optional system locale string.
pub fn resolve_language(configured: Option<&str>, sys_locale: Option<&str>) -> UiLanguage {
    if let Some(cfg) = configured {
        let clean = cfg.trim().to_ascii_lowercase();
        if clean == "ru" || clean.starts_with("ru_") || clean.starts_with("ru-") {
            return UiLanguage::Ru;
        }
        if clean == "en" || clean.starts_with("en_") || clean.starts_with("en-") {
            return UiLanguage::En;
        }
        if clean != "auto" && !clean.is_empty() {
            // Any explicitly chosen non-Russian language defaults to English
            return UiLanguage::En;
        }
    }

    if let Some(sys) = sys_locale {
        let clean = sys.trim().to_ascii_lowercase();
        if clean.starts_with("ru") {
            return UiLanguage::Ru;
        }
        if clean.starts_with("en") {
            return UiLanguage::En;
        }
    }

    // Default system fallback: English
    UiLanguage::En
}

/// Detects the target UI language based on configured preference or system environment.
/// Fallback rule: If language is Russian (`ru`), use `UiLanguage::Ru`.
/// Otherwise (English, German, French, etc., or invalid), default to `UiLanguage::En`.
pub fn detect_language(configured: Option<&str>) -> UiLanguage {
    let mut sys_locale = None;
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            sys_locale = Some(val);
            break;
        }
    }
    if sys_locale.is_none() {
        let glib_langs = glib::language_names();
        if let Some(first) = glib_langs.first() {
            sys_locale = Some(first.to_string());
        }
    }
    resolve_language(configured, sys_locale.as_deref())
}

/// Initializes the active UI language.
pub fn init_locale(configured_code: Option<&str>) {
    let lang = detect_language(configured_code);
    set_language(lang);
}

pub fn current_language() -> UiLanguage {
    match CURRENT_LANG.load(Ordering::Relaxed) {
        0 => UiLanguage::Ru,
        _ => UiLanguage::En,
    }
}

pub fn current_language_code() -> &'static str {
    current_language().as_code()
}

pub fn set_language(lang: UiLanguage) {
    CURRENT_LANG.store(lang as u8, Ordering::Relaxed);
    match lang {
        UiLanguage::En => {
            std::env::set_var("LANGUAGE", "en_US:en");
            std::env::set_var("LC_MESSAGES", "en_US.UTF-8");
        }
        UiLanguage::Ru => {
            std::env::set_var("LANGUAGE", "ru_RU:ru");
            std::env::set_var("LC_MESSAGES", "ru_RU.UTF-8");
        }
    }
}

pub fn set_language_from_code(code: &str) {
    let normalized = code.trim().to_ascii_lowercase();
    if normalized.starts_with("ru") {
        set_language(UiLanguage::Ru);
    } else {
        set_language(UiLanguage::En);
    }
}

/// Translates a given key with zero allocations for a specified language.
/// 1. Tries target language.
/// 2. Falls back to English if missing in target language.
/// 3. Falls back to literal key if missing everywhere.
pub fn tr_lang(key: &str, lang: UiLanguage) -> &'static str {
    let store = get_store();

    // 1. Check primary language
    let primary_map = match lang {
        UiLanguage::Ru => &store.ru_map,
        UiLanguage::En => &store.en_map,
    };
    if let Some(&val) = primary_map.get(key) {
        return val;
    }

    // Check with underscore / dot substitution
    if let Some(val) = lookup_variant(primary_map, key) {
        return val;
    }

    // 2. Fallback to English (if primary was Ru)
    if lang != UiLanguage::En {
        if let Some(&val) = store.en_map.get(key) {
            return val;
        }
        if let Some(val) = lookup_variant(&store.en_map, key) {
            return val;
        }
    }

    // 3. Fallback to static leaked key
    intern_str(key.to_string())
}

/// Translates a given key with zero allocations using the current active language.
pub fn tr(key: &str) -> &'static str {
    tr_lang(key, current_language())
}

fn lookup_variant(map: &HashMap<&'static str, &'static str>, key: &str) -> Option<&'static str> {
    if key.contains('_') {
        let dotted = key.replace('_', ".");
        if let Some(&val) = map.get(dotted.as_str()) {
            return Some(val);
        }
        // First underscore only: e.g. menu_split_view -> menu.split_view
        if let Some(idx) = key.find('_') {
            let mut prefix_dot = String::with_capacity(key.len());
            prefix_dot.push_str(&key[..idx]);
            prefix_dot.push('.');
            prefix_dot.push_str(&key[idx + 1..]);
            if let Some(&val) = map.get(prefix_dot.as_str()) {
                return Some(val);
            }
        }
    } else if key.contains('.') {
        let underscored = key.replace('.', "_");
        if let Some(&val) = map.get(underscored.as_str()) {
            return Some(val);
        }
    }
    None
}

/// Helper to translate strings with placeholders such as `{domain}`, `{count}` for a specified language.
pub fn tr_fmt_lang(key: &str, lang: UiLanguage, args: &[(&str, &str)]) -> String {
    let mut template = tr_lang(key, lang).to_string();
    for (name, val) in args {
        let placeholder = format!("{{{name}}}");
        template = template.replace(&placeholder, val);
    }
    template
}

/// Helper to translate strings with placeholders such as `{domain}`, `{count}`.
pub fn tr_fmt(key: &str, args: &[(&str, &str)]) -> String {
    tr_fmt_lang(key, current_language(), args)
}

/// Returns the JSON representation of the current active locale.
pub fn get_current_locale_json() -> &'static str {
    match current_language() {
        UiLanguage::Ru => RU_JSON,
        UiLanguage::En => EN_JSON,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_all_leaf_keys(
        val: &serde_json::Value,
        prefix: String,
        set: &mut std::collections::BTreeSet<String>,
    ) {
        if let serde_json::Value::Object(map) = val {
            for (k, v) in map {
                let full = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                if v.is_object() {
                    extract_all_leaf_keys(v, full, set);
                } else {
                    set.insert(full);
                }
            }
        }
    }

    #[test]
    fn test_locale_json_parity() {
        let en_val: serde_json::Value =
            serde_json::from_str(EN_JSON).expect("en.json must be valid JSON");
        let ru_val: serde_json::Value =
            serde_json::from_str(RU_JSON).expect("ru.json must be valid JSON");

        let mut en_keys = std::collections::BTreeSet::new();
        let mut ru_keys = std::collections::BTreeSet::new();

        extract_all_leaf_keys(&en_val, String::new(), &mut en_keys);
        extract_all_leaf_keys(&ru_val, String::new(), &mut ru_keys);

        let missing_in_ru: Vec<_> = en_keys.difference(&ru_keys).cloned().collect();
        let missing_in_en: Vec<_> = ru_keys.difference(&en_keys).cloned().collect();

        assert!(
            missing_in_ru.is_empty(),
            "ru.json is missing keys present in en.json: {:?}",
            missing_in_ru
        );
        assert!(
            missing_in_en.is_empty(),
            "en.json is missing keys present in ru.json: {:?}",
            missing_in_en
        );
    }

    #[test]
    fn test_i18n_lookup_and_fallback() {
        assert_eq!(
            tr_lang("translate.action_translate", UiLanguage::Ru),
            "Перевести страницу"
        );
        assert_eq!(
            tr_lang("translate_action_translate", UiLanguage::Ru),
            "Перевести страницу"
        );
        assert_eq!(
            tr_lang("menu.split_view", UiLanguage::Ru),
            "Разделенный экран"
        );
        assert_eq!(
            tr_lang("menu_split_view", UiLanguage::Ru),
            "Разделенный экран"
        );

        assert_eq!(
            tr_lang("translate.action_translate", UiLanguage::En),
            "Translate Page"
        );
        assert_eq!(
            tr_lang("translate_action_translate", UiLanguage::En),
            "Translate Page"
        );
        assert_eq!(tr_lang("menu.split_view", UiLanguage::En), "Split View");
        assert_eq!(tr_lang("menu_split_view", UiLanguage::En), "Split View");
    }

    #[test]
    fn test_resolve_language() {
        assert_eq!(resolve_language(Some("ru"), None), UiLanguage::Ru);
        assert_eq!(resolve_language(Some("ru-RU"), None), UiLanguage::Ru);
        assert_eq!(resolve_language(Some("en"), None), UiLanguage::En);
        assert_eq!(resolve_language(Some("en_US"), None), UiLanguage::En);
        // Fallback for other languages is En
        assert_eq!(resolve_language(Some("de_DE"), None), UiLanguage::En);
        assert_eq!(resolve_language(Some("fr"), None), UiLanguage::En);

        // Auto uses system
        assert_eq!(
            resolve_language(Some("auto"), Some("ru_RU.UTF-8")),
            UiLanguage::Ru
        );
        assert_eq!(
            resolve_language(Some("auto"), Some("en_US.UTF-8")),
            UiLanguage::En
        );
        assert_eq!(
            resolve_language(Some("auto"), Some("de_DE.UTF-8")),
            UiLanguage::En
        );
        assert_eq!(resolve_language(None, Some("ru_RU.UTF-8")), UiLanguage::Ru);
        assert_eq!(resolve_language(None, Some("de_DE.UTF-8")), UiLanguage::En);
    }

    #[test]
    fn test_tr_fmt() {
        let s_en = tr_fmt_lang(
            "find_bar.counter",
            UiLanguage::En,
            &[("current", "3"), ("total", "10")],
        );
        assert_eq!(s_en, "3 of 10");

        let s_ru = tr_fmt_lang(
            "find_bar.counter",
            UiLanguage::Ru,
            &[("current", "3"), ("total", "10")],
        );
        assert_eq!(s_ru, "3 из 10");
    }
}
