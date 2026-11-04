# Contributing Translations to Take Web

Thank you for helping translate Take Web! This guide explains how Take Web's internationalization (i18n) system works, how to improve existing translations, and how to add support for new languages.

---

## 1. Architecture Overview

Take Web uses a unified, zero-cloud JSON-based localization system. All user-visible strings are stored in `resources/locales/` as structured JSON files:

- `resources/locales/en.json` — **Source of Truth** (Base & Default language for all systems).
- `resources/locales/ru.json` — Russian translation.

### How Translations are Consumed:
1. **Rust GTK4 UI (Native Interface)**:
   - Evaluated via `crate::i18n::tr("category.key")` or `tr_fmt("category.key", &[("placeholder", value)])`.
   - Locale is initialized on startup based on user settings (`take://settings`) or the system `LANG` / `LC_ALL` environment variable.
2. **Internal Pages (`take://settings`, `take://start`, `take://trackers`)**:
   - The scheme handler automatically interpolates `{{category.key}}` tags at load time before HTML is served to WebKit, eliminating untranslated flash (FOUT).
3. **Internal JavaScript (`settings.js`, `startpage.js`, `trackers.js`)**:
   - Dynamic client-side scripts fetch active translations from `take://locales/current.json`.

---

## 2. Fallback Mechanism

- English (`en`) is the default fallback for all systems.
- If a string or key is missing in a specific locale (e.g., `ru.json`), the system automatically resolves the corresponding value from `en.json`.
- If a string is missing in `en.json`, the lookup falls back to the key name itself.
- Keys support both dot-notation (`"menu.new_tab"`) and underscore-notation (`"menu_new_tab"`).

---

## 3. How to Improve Existing Translations

1. Open `resources/locales/<lang>.json` (for example, `resources/locales/ru.json`).
2. Search for the key you wish to refine.
3. Update the translation value.
   - **Important**: Keep any `{placeholder}` tokens intact (e.g. `{count}`, `{percent}`, `{domain}`, `{name}`, `{time}`).
4. Verify tests pass:
   ```bash
   cargo test --bin takeweb test_locale_json_parity
   ```

---

## 4. How to Add a New Language

Suppose you want to add German (`de`) or French (`fr`):

### Step 1: Create the Locale File
Copy `resources/locales/en.json` to `resources/locales/<lang>.json` (e.g., `resources/locales/de.json`):
```bash
cp resources/locales/en.json resources/locales/de.json
```
Translate all string values in `de.json`. Do not change the JSON structure or key names.

### Step 2: Register in `src/i18n.rs`
1. Add your language to `UiLanguage`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
   pub enum UiLanguage {
       #[default]
       En,
       Ru,
       De, // Your new language
   }
   ```
2. Embed the JSON file:
   ```rust
   pub static DE_JSON: &str = include_str!("../resources/locales/de.json");
   ```
3. Update `code()` and `from_code()`:
   ```rust
   UiLanguage::De => "de",
   ```
4. Update `resolve_language()` to recognize the new language code:
   ```rust
   "de" => UiLanguage::De,
   ```
5. Add to `get_locale_json_by_code()` and the `lazy_static` table loader in `get_or_init_locale_table()`.

### Step 3: Add Option in Settings UI
In `resources/locales/en.json` and other locales, add display names under `settings.lang_*` if needed.
In `resources/settings/index.html`, add `<option value="de">Deutsch</option>` to the language selector.

### Step 4: Run Verification Tests
```bash
cargo test --bin takeweb
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

## 5. Automated CI Verification

Take Web includes an automated parity test (`test_locale_json_parity`) that validates all translation files against `en.json`.

This test will fail if:
- Any key existing in `en.json` is missing in a supported translation.
- Any orphaned key exists in a translation that does not exist in `en.json`.
- The JSON contains syntax errors or invalid formatting.

To run the parity test locally:
```bash
cargo test --bin takeweb test_locale_json_parity
```

---

## 6. Pull Request Checklist

Before opening a Pull Request:
- [ ] JSON syntax is valid and properly formatted (2 spaces indentation).
- [ ] All `{placeholder}` variables match `en.json` exactly.
- [ ] `cargo test --bin takeweb` passes without errors.
- [ ] `cargo fmt` has been run.
- [ ] `cargo clippy --all-targets -- -D warnings` reports 0 warnings.
