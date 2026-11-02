//! Native `take://` URI scheme (with legacy `echo://` alias support).
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
pub const ALT_SCHEME: &str = "echo";

/// Strict policy for first-party pages: no remote code, no framing, no `<base>`.
const INTERNAL_CSP: &str = "default-src 'none'; \
    script-src 'self' 'unsafe-inline' take: echo:; \
    style-src 'self' 'unsafe-inline' take: echo:; \
    img-src 'self' data: take: echo: https:; \
    font-src 'self' data: take: echo:; \
    connect-src 'self' take: echo:; \
    form-action 'none'; \
    base-uri 'none'; \
    frame-ancestors 'none'";

/// The test suite needs inline handlers and arbitrary requests.
const TEST_SUITE_CSP: &str = "default-src * data: blob: 'unsafe-inline' 'unsafe-eval'";

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
const PNG: &str = "image/png";

/// Maps `take://<host>/<path>` or `echo://<host>/<path>` onto an embedded resource.
fn lookup(host: &str, path: &str) -> Option<Asset> {
    let path = path.trim_start_matches('/');
    match (host, path) {
        ("start", "" | "index.html") => asset!(HTML, "startpage/index.html"),
        ("start", "startpage.css") => asset!(CSS, "startpage/startpage.css"),
        ("start", "startpage.js") => asset!(JS, "startpage/startpage.js"),
        ("start", "raccoon.png") => asset!(PNG, "startpage/raccoon.png"),

        ("settings", "" | "index.html") => asset!(HTML, "settings/index.html"),
        ("settings", "settings.css") => asset!(CSS, "settings/settings.css"),
        ("settings", "settings.js") => asset!(JS, "settings/settings.js"),
        ("settings", "raccoon.png") => asset!(PNG, "settings/raccoon.png"),

        ("trackers" | "privacy", "" | "index.html") => asset!(HTML, "trackers/index.html"),
        ("trackers" | "privacy", "trackers.css") => asset!(CSS, "trackers/trackers.css"),
        ("trackers" | "privacy", "trackers.js") => asset!(JS, "trackers/trackers.js"),

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
    if url.scheme() != SCHEME && url.scheme() != ALT_SCHEME {
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

    // Alias redirect: if a request arrives on echo://..., redirect to take://...
    if let Ok(url) = url::Url::parse(&uri) {
        if url.scheme() == ALT_SCHEME {
            let mut target = format!("{SCHEME}://{}{}", url.host_str().unwrap_or(""), url.path());
            if let Some(query) = url.query() {
                target.push('?');
                target.push_str(query);
            }
            if let Some(f) = url.fragment() {
                target.push('#');
                target.push_str(f);
            }
            let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(b""));
            let response = URISchemeResponse::new(&stream, 0);
            response.set_status(301, Some("Moved Permanently"));
            let headers = soup::MessageHeaders::new(soup::MessageHeadersType::Response);
            headers.append("Location", &target);
            headers.append("Cache-Control", "no-store");
            response.set_http_headers(headers);
            request.finish_with_response(&response);
            return;
        }
    }

    if let Some((host, _path)) = parse(&uri) {
        if host == "error" {
            let dummy_err = glib::Error::new(
                glib::FileError::Noent,
                "Пример сетевой ошибки для демонстрации страницы take://error",
            );
            let html = crate::browser::error_page::render_error_page("take://error", &dummy_err);
            let bytes = html.into_bytes();
            let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
            let response = URISchemeResponse::new(&stream, -1);
            response.set_status(200, Some("OK"));
            response.set_content_type("text/html; charset=utf-8");
            let headers = soup::MessageHeaders::new(soup::MessageHeadersType::Response);
            headers.append("Content-Type", "text/html; charset=utf-8");
            headers.append("Cache-Control", "no-store");
            response.set_http_headers(headers);
            request.finish_with_response(&response);
            return;
        }
    }

    match parse(&uri).and_then(|(host, path)| lookup(&host, &path)) {
        Some(asset) => respond(request, 200, "OK", asset.mime, asset.csp, asset.body),
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

/// Registers `take://` and `echo://` and marks them as secure schemes.
pub fn register(context: &WebContext) {
    context.register_uri_scheme(SCHEME, handle);
    context.register_uri_scheme(ALT_SCHEME, handle);

    if let Some(security) = context.security_manager() {
        register_security(&security);
    }
}

fn register_security(security: &SecurityManager) {
    security.register_uri_scheme_as_secure(SCHEME);
    security.register_uri_scheme_as_secure(ALT_SCHEME);
    // Note: Neither scheme is registered as CORS-enabled to prevent
    // any cross-origin requests from web pages into internal resources.
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
        assert!(lookup("start", "/../settings/settings.js").is_none());
        assert!(lookup("start", "/settings.js").is_none());
        assert!(lookup("evil", "/").is_none());
    }

    #[test]
    fn canonical_urls_and_logical_names() {
        assert_eq!(
            canonical_load_url("take://start").as_deref(),
            Some("take://start/")
        );
        assert_eq!(
            canonical_load_url("echo://start").as_deref(),
            Some("take://start/")
        );
        assert_eq!(
            canonical_load_url("take://privacy").as_deref(),
            Some("take://trackers/")
        );
        assert_eq!(
            canonical_load_url("echo://privacy").as_deref(),
            Some("take://trackers/")
        );
        assert_eq!(
            canonical_load_url("take://settings#about").as_deref(),
            Some("take://settings/#about")
        );
        assert_eq!(
            canonical_load_url("echo://settings#about").as_deref(),
            Some("take://settings/#about")
        );
        assert_eq!(canonical_load_url("take://nope"), None);
        assert_eq!(canonical_load_url("echo://nope"), None);
        assert_eq!(
            logical_from_uri("take://settings/#about").as_deref(),
            Some("take://settings")
        );
        assert_eq!(
            logical_from_uri("echo://settings/#about").as_deref(),
            Some("take://settings")
        );
        assert_eq!(logical_from_uri("https://example.com/"), None);
    }
}
