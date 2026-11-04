//! High-performance URL query cleaning and tracking parameter stripping.
//!
//! Strips cross-site tracking tokens and analytics parameters (e.g., `utm_*`, `fbclid`, `gclid`,
//! `yclid`, `msclkid`, etc.) from URLs during navigation, Omnibox input, and link copying.
//! This defeats bounce tracking and prevents advertising networks from correlating user sessions.

use url::Url;

/// Comprehensive database of known tracking and campaign query parameters.
const TRACKING_EXACT_PARAMS: &[&str] = &[
    // Google & Alphabet Ad Tracking
    "gclid",
    "gbraid",
    "wbraid",
    "_ga",
    "_gl",
    "gad_source",
    "gclsrc",
    "dclid",
    // Meta (Facebook, Instagram)
    "fbclid",
    "igshid",
    "fref",
    // Yandex
    "yclid",
    "ymclid",
    "_openstat",
    // VK (VKontakte)
    "vk_click_id",
    // Twitter / X
    "twclid",
    // Microsoft / Bing
    "msclkid",
    // TikTok
    "ttclid",
    // Email & Marketing Automation (MailChimp, HubSpot, Vero, Marketo)
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "vero_id",
    "vero_conv",
    "mkt_tok",
    // Affiliate & Ad Network Click Identifiers
    "zanpid",
    "wickedid",
    "irclickid",
    // Generic Referral & Source Tags
    "ref_src",
    "ref_url",
    "from_source",
];

/// Returns `true` if the specified query parameter name is a known tracker.
#[inline]
pub fn is_tracking_param(param_name: &str) -> bool {
    let lower = param_name.to_ascii_lowercase();

    // 1. All UTM parameters (utm_source, utm_medium, utm_campaign, utm_content, utm_term, utm_id, etc.)
    if lower.starts_with("utm_") {
        return true;
    }

    // 2. Exact match against known tracker database
    TRACKING_EXACT_PARAMS.iter().any(|&p| p == lower)
}

pub struct UrlCleaner;

impl UrlCleaner {
    /// Strips tracking parameters from a URL string.
    ///
    /// Returns `Some(cleaned_url)` if at least one tracking parameter was removed.
    /// Returns `None` if the URL had no tracking parameters or is not an HTTP(S) URL.
    pub fn clean_url(raw_url: &str) -> Option<String> {
        let (cleaned, removed_count) = Self::clean_url_counted(raw_url);
        if removed_count > 0 {
            Some(cleaned)
        } else {
            None
        }
    }

    /// Strips tracking parameters from a URL string, returning the cleaned URL
    /// (or the original string if unchanged) along with the count of removed parameters.
    pub fn clean_url_counted(raw_url: &str) -> (String, usize) {
        let trimmed = raw_url.trim();

        // Only clean standard web URLs (http / https)
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            return (trimmed.to_string(), 0);
        }

        // Fast path: if there is no query delimiter '?', no query params exist
        if !trimmed.contains('?') {
            return (trimmed.to_string(), 0);
        }

        let mut parsed = match Url::parse(trimmed) {
            Ok(url) => url,
            Err(_) => return (trimmed.to_string(), 0),
        };

        let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        if pairs.is_empty() {
            return (trimmed.to_string(), 0);
        }

        let mut kept_pairs = Vec::with_capacity(pairs.len());
        let mut removed_count = 0;

        for (k, v) in pairs {
            if is_tracking_param(&k) {
                removed_count += 1;
            } else {
                kept_pairs.push((k, v));
            }
        }

        if removed_count == 0 {
            return (trimmed.to_string(), 0);
        }

        if kept_pairs.is_empty() {
            // Strip the query string entirely
            parsed.set_query(None);
        } else {
            // Re-serialize only legitimate non-tracking query pairs
            parsed.query_pairs_mut().clear().extend_pairs(&kept_pairs);
        }

        (parsed.to_string(), removed_count)
    }

    /// Convenience helper that returns the cleaned string directly.
    pub fn strip_tracking(raw_url: &str) -> String {
        Self::clean_url_counted(raw_url).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strips_utm_parameters() {
        let raw =
            "https://example.com/article?utm_source=twitter&utm_medium=social&utm_campaign=launch";
        let cleaned = UrlCleaner::clean_url(raw);
        assert_eq!(cleaned.as_deref(), Some("https://example.com/article"));
    }

    #[test]
    fn test_preserves_functional_query_parameters() {
        let raw = "https://example.com/search?q=rust+lang&utm_source=news&page=2";
        let cleaned = UrlCleaner::clean_url(raw);
        assert_eq!(
            cleaned.as_deref(),
            Some("https://example.com/search?q=rust+lang&page=2")
        );
    }

    #[test]
    fn test_strips_facebook_google_yandex_click_ids() {
        let raw = "https://example.com/shop?gclid=ABC12345&fbclid=XYZ67890&yclid=98765&item=laptop";
        let cleaned = UrlCleaner::clean_url(raw);
        assert_eq!(
            cleaned.as_deref(),
            Some("https://example.com/shop?item=laptop")
        );
    }

    #[test]
    fn test_preserves_fragments_and_ports() {
        let raw = "https://localhost:8080/docs?utm_source=rss&version=1.0#installation";
        let cleaned = UrlCleaner::clean_url(raw);
        assert_eq!(
            cleaned.as_deref(),
            Some("https://localhost:8080/docs?version=1.0#installation")
        );
    }

    #[test]
    fn test_case_insensitive_matching() {
        let raw = "https://example.com/?UTM_SOURCE=TEST&FbClId=123";
        let cleaned = UrlCleaner::clean_url(raw);
        assert_eq!(cleaned.as_deref(), Some("https://example.com/"));
    }

    #[test]
    fn test_leaves_clean_urls_untouched() {
        let raw = "https://example.com/product?id=42&color=blue";
        assert_eq!(UrlCleaner::clean_url(raw), None);
        assert_eq!(UrlCleaner::strip_tracking(raw), raw);
    }

    #[test]
    fn test_leaves_internal_schemes_untouched() {
        let raw = "take://start";
        assert_eq!(UrlCleaner::clean_url(raw), None);
        assert_eq!(UrlCleaner::strip_tracking(raw), raw);
    }

    #[test]
    fn test_counts_removed_parameters() {
        let raw = "https://example.com/?utm_source=a&utm_medium=b&gclid=c&q=1";
        let (cleaned, count) = UrlCleaner::clean_url_counted(raw);
        assert_eq!(count, 3);
        assert_eq!(cleaned, "https://example.com/?q=1");
    }
}
