//! Native `take://` URI scheme.
//!
//! Internal pages (start page, settings, privacy report, test suite) used to be
//! loaded through `file://`, which forced `allow_universal_access_from_file_urls`
//! to be enabled globally – any local HTML file could then read arbitrary files.
//! Serving them from memory through a registered scheme removes that need
//! entirely, and since every asset is embedded with `include_bytes!` there is no
//! filesystem path to traverse.

use gtk4::gio;
use webkit6::{soup, SecurityManager, URISchemeRequest, URISchemeResponse, WebContext};

pub const SCHEME: &str = "take";

/// Strict policy for first-party pages: no remote code, no framing, no `<base>`.
const INTERNAL_CSP: &str = "default-src 'none'; \
    script-src 'self' 'unsafe-inline' take:; \
    style-src 'self' 'unsafe-inline' take:; \
    img-src 'self' data: take: https:; \
    font-src 'self' data: take:; \
    connect-src 'self' take:; \
    form-action 'none'; \
    base-uri 'none'; \
    frame-ancestors 'none'";

/// The test suite needs inline handlers and arbitrary requests.
const TEST_SUITE_CSP: &str = "default-src * data: blob: 'unsafe-inline' 'unsafe-eval'";

/// Internal Bergamot WASM host page CSP (`take://translator/`).
const TRANSLATOR_CSP: &str = "default-src 'self' take: take-bergamot: data: blob: 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval'; \
    script-src 'self' take: take-bergamot: data: blob: 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval'; \
    worker-src 'self' take: take-bergamot: blob:; \
    connect-src 'self' take: take-bergamot: data: blob:;";

struct Asset {
    mime: &'static str,
    body: &'static [u8],
    csp: &'static str,
}

macro_rules! asset {
    ($mime:expr, $path:expr) => {
        Some(Asset {
            mime: $mime,
            body: include_bytes!(concat!("../../resources/", $path)),
            csp: INTERNAL_CSP,
        })
    };
}

const HTML: &str = "text/html; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";
const JSON: &str = "application/json; charset=utf-8";
const PNG: &str = "image/png";

/// Maps `take://<host>/<path>` onto an embedded resource.
fn lookup(host: &str, path: &str) -> Option<Asset> {
    let path = path.trim_start_matches('/');
    match (host, path) {
        ("start", "" | "index.html") => asset!(HTML, "startpage/index.html"),
        ("start", "startpage.css") => asset!(CSS, "startpage/startpage.css"),
        ("start", "startpage.js") => asset!(JS, "startpage/startpage.js"),
        ("start", "raccoon.png") => asset!(PNG, "startpage/raccoon.png"),
        ("start", "purple_hills.png") => asset!(PNG, "startpage/purple_hills.png"),

        ("settings", "" | "index.html") => asset!(HTML, "settings/index.html"),
        ("settings", "settings.css") => asset!(CSS, "settings/settings.css"),
        ("settings", "settings.js") => asset!(JS, "settings/settings.js"),
        ("settings", "raccoon.png") => asset!(PNG, "settings/raccoon.png"),
        ("settings", "purple_hills.png") => asset!(PNG, "startpage/purple_hills.png"),

        ("trackers" | "privacy", "" | "index.html") => asset!(HTML, "trackers/index.html"),
        ("trackers" | "privacy", "trackers.css") => asset!(CSS, "trackers/trackers.css"),
        ("trackers" | "privacy", "trackers.js") => asset!(JS, "trackers/trackers.js"),

        ("locales", "" | "current.json" | "current") => Some(Asset {
            mime: JSON,
            body: crate::i18n::get_current_locale_json().as_bytes(),
            csp: INTERNAL_CSP,
        }),
        ("locales", "en.json" | "en") => Some(Asset {
            mime: JSON,
            body: crate::i18n::EN_JSON.as_bytes(),
            csp: INTERNAL_CSP,
        }),
        ("locales", "ru.json" | "ru") => Some(Asset {
            mime: JSON,
            body: crate::i18n::RU_JSON.as_bytes(),
            csp: INTERNAL_CSP,
        }),

        ("test-suite", "" | "index.html") => Some(Asset {
            mime: HTML,
            body: include_bytes!("../../resources/test-suite/index.html"),
            csp: TEST_SUITE_CSP,
        }),
        _ => None,
    }
}

fn parse(uri: &str) -> Option<(String, String)> {
    let url = url::Url::parse(uri).ok()?;
    if url.scheme() != SCHEME {
        return None;
    }
    Some((url.host_str()?.to_ascii_lowercase(), url.path().to_string()))
}

fn respond(
    request: &URISchemeRequest,
    status: u32,
    reason: &str,
    mime: &str,
    csp: &str,
    body: &'static [u8],
) {
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(body));
    let response = URISchemeResponse::new(&stream, body.len() as i64);
    response.set_status(status, Some(reason));
    response.set_content_type(mime);

    let headers = soup::MessageHeaders::new(soup::MessageHeadersType::Response);
    headers.append("Content-Type", mime);
    headers.append("Content-Security-Policy", csp);
    headers.append("X-Content-Type-Options", "nosniff");
    headers.append("Referrer-Policy", "no-referrer");
    // Internal pages are tiny and embedded; skip HTTP caching so theme/data updates are never stale.
    headers.append("Cache-Control", "no-store");
    response.set_http_headers(headers);

    request.finish_with_response(&response);
}

fn respond_owned(
    request: &URISchemeRequest,
    status: u32,
    reason: &str,
    mime: &str,
    csp: &str,
    body: Vec<u8>,
) {
    let len = body.len() as i64;
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(body));
    let response = URISchemeResponse::new(&stream, len);
    response.set_status(status, Some(reason));
    response.set_content_type(mime);

    let headers = soup::MessageHeaders::new(soup::MessageHeadersType::Response);
    headers.append("Content-Type", mime);
    headers.append("Content-Security-Policy", csp);
    headers.append("X-Content-Type-Options", "nosniff");
    headers.append("Referrer-Policy", "no-referrer");
    headers.append("Cache-Control", "no-store");
    response.set_http_headers(headers);

    request.finish_with_response(&response);
}

fn handle_translator_host(request: &URISchemeRequest, path: &str) {
    let clean = path.trim_start_matches('/');
    match clean {
        "" | "index.html" => {
            respond(
                request,
                200,
                "OK",
                HTML,
                TRANSLATOR_CSP,
                crate::browser::translator::TRANSLATOR_HOST_HTML.as_bytes(),
            );
        }
        "bergamot-translator-worker.js" => {
            let file_path = crate::browser::translator::wasm_runtime_dir()
                .join("bergamot-translator-worker.js");
            match std::fs::read(&file_path) {
                Ok(bytes) => respond_owned(request, 200, "OK", JS, TRANSLATOR_CSP, bytes),
                Err(_) => respond(
                    request,
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    TRANSLATOR_CSP,
                    b"404 WASM Glue Not Found",
                ),
            }
        }
        "bergamot-translator-worker.wasm" => {
            let file_path = crate::browser::translator::wasm_runtime_dir()
                .join("bergamot-translator-worker.wasm");
            match std::fs::read(&file_path) {
                Ok(bytes) => respond_owned(
                    request,
                    200,
                    "OK",
                    "application/wasm",
                    TRANSLATOR_CSP,
                    bytes,
                ),
                Err(_) => respond(
                    request,
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    TRANSLATOR_CSP,
                    b"404 WASM Binary Not Found",
                ),
            }
        }
        "wasm_payload.js" => match crate::browser::translator::build_wasm_payload_js() {
            Some(bytes) => respond_owned(request, 200, "OK", JS, TRANSLATOR_CSP, bytes),
            None => respond(
                request,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                TRANSLATOR_CSP,
                b"404 WASM Binary Not Found",
            ),
        },
        p if p.starts_with("models/") && p.ends_with(".js") => {
            let pair = &p["models/".len()..p.len() - ".js".len()];
            match crate::browser::translator::build_model_bundle_js(pair) {
                Some(bytes) => respond_owned(request, 200, "OK", JS, TRANSLATOR_CSP, bytes),
                None => respond(
                    request,
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    TRANSLATOR_CSP,
                    b"404 Model Bundle Not Found",
                ),
            }
        }
        _ => respond(
            request,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            TRANSLATOR_CSP,
            b"404 Not Found",
        ),
    }
}

pub fn interpolate_html(raw_html: &str) -> String {
    let mut result = String::with_capacity(raw_html.len());
    let mut cursor = 0;
    while let Some(start_idx) = raw_html[cursor..].find("{{") {
        let abs_start = cursor + start_idx;
        result.push_str(&raw_html[cursor..abs_start]);
        if let Some(end_idx) = raw_html[abs_start..].find("}}") {
            let abs_end = abs_start + end_idx;
            let key = raw_html[abs_start + 2..abs_end].trim();
            if key == "current_lang" {
                result.push_str(crate::i18n::current_language_code());
            } else {
                result.push_str(crate::i18n::tr(key));
            }
            cursor = abs_end + 2;
        } else {
            result.push_str(&raw_html[abs_start..]);
            cursor = raw_html.len();
            break;
        }
    }
    result.push_str(&raw_html[cursor..]);
    result
}

fn handle(request: &URISchemeRequest) {
    let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();

    // Internal pages are read-only documents.
    let method = request
        .http_method()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "GET".into());
    if method != "GET" && method != "HEAD" {
        respond(
            request,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            INTERNAL_CSP,
            b"405",
        );
        return;
    }

    if let Some((host, path)) = parse(&uri) {
        if host == "translator" {
            handle_translator_host(request, &path);
            return;
        }
        if host == "error" {
            let dummy_err = glib::Error::new(
                glib::FileError::Noent,
                "Sample network error for take://error",
            );
            let html = crate::browser::error_page::render_error_page("take://error", &dummy_err);
            respond_owned(request, 200, "OK", HTML, INTERNAL_CSP, html.into_bytes());
            return;
        }
    }

    match parse(&uri).and_then(|(host, path)| lookup(&host, &path)) {
        Some(asset) => {
            if asset.mime == HTML {
                if let Ok(raw_str) = std::str::from_utf8(asset.body) {
                    let interpolated = interpolate_html(raw_str);
                    respond_owned(
                        request,
                        200,
                        "OK",
                        asset.mime,
                        asset.csp,
                        interpolated.into_bytes(),
                    );
                    return;
                }
            }
            respond(request, 200, "OK", asset.mime, asset.csp, asset.body)
        }
        None => {
            log::debug!("scheme not found: {uri}");
            respond(
                request,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                INTERNAL_CSP,
                b"404 Not Found",
            );
        }
    }
}

/// Strict allowlist resolver for `take-bergamot://wasm/<file>` and
/// `take-bergamot://models/<pair>/<file>`. Rejects any file or language pair
/// not explicitly declared in `crate::browser::translator`.
pub fn resolve_bergamot_file(uri: &str) -> Option<(std::path::PathBuf, &'static str)> {
    let url = url::Url::parse(uri).ok()?;
    if url.scheme() != crate::browser::translator::BERGAMOT_SCHEME {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let path = url.path().trim_start_matches('/');
    if path.is_empty() || path.contains("..") || path.contains('\\') {
        return None;
    }

    match host.as_str() {
        "wasm" => {
            let asset = crate::browser::translator::WASM_RUNTIME_ASSETS
                .iter()
                .find(|a| a.filename == path)?;
            let mime = if asset.filename.ends_with(".wasm") {
                "application/wasm"
            } else {
                JS
            };
            Some((
                crate::browser::translator::wasm_runtime_dir().join(asset.filename),
                mime,
            ))
        }
        "models" => {
            let (pair, filename) = path.split_once('/')?;
            let spec = crate::browser::translator::find_model_spec(pair)?;
            let file_asset = spec.files.iter().find(|f| f.filename == filename)?;
            Some((
                crate::browser::translator::model_pair_dir(spec.pair).join(file_asset.filename),
                "application/octet-stream",
            ))
        }
        _ => None,
    }
}

fn handle_bergamot(request: &URISchemeRequest) {
    let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
    let method = request
        .http_method()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "GET".into());

    if method != "GET" && method != "HEAD" {
        respond(
            request,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            INTERNAL_CSP,
            b"405",
        );
        return;
    }

    let Some((file_path, mime)) = resolve_bergamot_file(&uri) else {
        respond(
            request,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            INTERNAL_CSP,
            b"404 Not Found",
        );
        return;
    };

    match std::fs::read(&file_path) {
        Ok(bytes) => {
            let len = bytes.len() as i64;
            let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
            let response = URISchemeResponse::new(&stream, len);
            response.set_status(200, Some("OK"));
            response.set_content_type(mime);

            let headers = soup::MessageHeaders::new(soup::MessageHeadersType::Response);
            headers.append("Content-Type", mime);
            headers.append("Access-Control-Allow-Origin", "*");
            headers.append("Cross-Origin-Resource-Policy", "cross-origin");
            headers.append("X-Content-Type-Options", "nosniff");
            headers.append("Cache-Control", "no-store");
            response.set_http_headers(headers);

            request.finish_with_response(&response);
        }
        Err(err) => {
            log::warn!("take-bergamot failed to open {:?}: {err}", file_path);
            respond(
                request,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                INTERNAL_CSP,
                b"500 Error Reading Model",
            );
        }
    }
}

/// Registers `take://` and `take-bergamot://` and marks them as secure schemes.
pub fn register(context: &WebContext) {
    context.register_uri_scheme(SCHEME, handle);
    context.register_uri_scheme(crate::browser::translator::BERGAMOT_SCHEME, handle_bergamot);

    if let Some(security) = context.security_manager() {
        register_security(&security);
    }
}

fn register_security(security: &SecurityManager) {
    security.register_uri_scheme_as_secure(SCHEME);
    // Note: `take://` is not registered as CORS-enabled to prevent
    // any cross-origin requests from web pages into internal UI resources.
    // `take-bergamot://` is a separate read-only allowlisted scheme for local WASM & model weights.
    security.register_uri_scheme_as_secure(crate::browser::translator::BERGAMOT_SCHEME);
    security.register_uri_scheme_as_cors_enabled(crate::browser::translator::BERGAMOT_SCHEME);
}

/// Canonical form used when loading an internal page: `take://host/`.
/// The trailing slash makes relative asset URLs resolve under the same host.
pub fn canonical_load_url(logical: &str) -> Option<String> {
    let (host, _) = parse(logical)?;
    let host = if host == "privacy" {
        "trackers".to_string()
    } else {
        host
    };
    lookup(&host, "")?;
    let fragment = url::Url::parse(logical)
        .ok()
        .and_then(|u| u.fragment().map(str::to_owned));
    Some(match fragment {
        Some(f) => format!("{SCHEME}://{host}/#{f}"),
        None => format!("{SCHEME}://{host}/"),
    })
}

/// `take://settings/#about` → `take://settings`. Returns `None` for non-internal URIs.
pub fn logical_from_uri(uri: &str) -> Option<String> {
    let (host, _) = parse(uri)?;
    Some(format!("{SCHEME}://{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_resolves_known_pages_and_rejects_traversal() {
        assert!(lookup("start", "/").is_some());
        assert!(lookup("start", "/startpage.css").is_some());
        assert!(lookup("privacy", "/trackers.js").is_some());
        assert!(lookup("locales", "/current.json").is_some());
        assert!(lookup("locales", "/en.json").is_some());
        assert!(lookup("locales", "/ru.json").is_some());
        assert!(lookup("start", "/../settings/settings.js").is_none());
        assert!(lookup("start", "/settings.js").is_none());
        assert!(lookup("evil", "/").is_none());
    }

    #[test]
    fn test_interpolate_html() {
        let template = "<html lang=\"{{current_lang}}\"><title>{{app.name}}</title><body>{{settings.header}}</body></html>";
        let rendered = interpolate_html(template);
        assert!(rendered.contains("Take Web"));
        assert!(rendered.contains("<html lang=\""));
    }

    #[test]
    fn canonical_urls_and_logical_names() {
        assert_eq!(
            canonical_load_url("take://start").as_deref(),
            Some("take://start/")
        );
        assert_eq!(
            canonical_load_url("take://privacy").as_deref(),
            Some("take://trackers/")
        );
        assert_eq!(
            canonical_load_url("take://settings#about").as_deref(),
            Some("take://settings/#about")
        );
        assert_eq!(canonical_load_url("take://nope"), None);
        assert_eq!(
            logical_from_uri("take://settings/#about").as_deref(),
            Some("take://settings")
        );
        assert_eq!(logical_from_uri("https://example.com/"), None);
    }

    #[test]
    fn bergamot_scheme_resolves_allowlisted_and_rejects_traversal() {
        let wasm = resolve_bergamot_file("take-bergamot://wasm/bergamot-translator-worker.wasm");
        assert!(wasm.is_some());
        assert_eq!(wasm.unwrap().1, "application/wasm");

        let model =
            resolve_bergamot_file("take-bergamot://models/en-ru/model.enru.intgemm.alphas.bin");
        assert!(model.is_some());

        assert!(resolve_bergamot_file("take-bergamot://wasm/../../etc/passwd").is_none());
        assert!(resolve_bergamot_file("take-bergamot://models/en-ru/evil.bin").is_none());
        assert!(resolve_bergamot_file("take-bergamot://models/xx-yy/vocab.spm").is_none());
    }
}
