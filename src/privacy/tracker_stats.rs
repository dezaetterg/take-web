/// Classifies a resource URL against known tracker patterns and returns (domain, rule_description)
pub fn classify_tracker(url_str: &str) -> Option<(String, String)> {
    let trimmed = url_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Support relative paths (e.g. "/gen_204?...") used by in-page beacons
    let (host, path) = if let Ok(parsed) = url::Url::parse(trimmed) {
        let host = parsed.host_str().unwrap_or_default().to_lowercase();
        let path = parsed.path().to_lowercase();
        (host, path)
    } else {
        let lower = trimmed.to_lowercase();
        if lower.starts_with('/') {
            (String::new(), lower)
        } else {
            (lower.clone(), lower)
        }
    };

    // Google telemetry & tracking endpoints (including relative beacons on google.com)
    if path.contains("gen_204") {
        return Some((
            "google.com/gen_204".to_string(),
            "Google Telemetry (gen_204)".to_string(),
        ));
    }
    if path.contains("client_204") {
        return Some((
            "google.com/client_204".to_string(),
            "Google Telemetry (client_204)".to_string(),
        ));
    }
    if host.contains("ogads-pa.") || host.starts_with("ogads-pa.clients6.google.") {
        return Some((
            "ogads-pa.clients6.google.com".to_string(),
            "Google Ad Personalization".to_string(),
        ));
    }
    if host.starts_with("play.google.") && path.contains("log") {
        return Some((
            "play.google.com/log".to_string(),
            "Google Play Telemetry".to_string(),
        ));
    }

    // Meta Pixel /tr/ endpoint
    if host.contains("facebook.com") && path.contains("/tr") {
        return Some((
            "pixel.facebook.com".to_string(),
            "Meta Pixel & Social".to_string(),
        ));
    }

    // Yandex Click telemetry
    if host.contains("yandex.") && path.contains("/clck") {
        return Some((
            "yandex.ru/clck".to_string(),
            "Yandex Telemetry (clck)".to_string(),
        ));
    }

    let trackers: &[(&[&str], &str, &str)] = &[
        (
            &["google-analytics.com", "googletagmanager.com"],
            "google-analytics.com",
            "Google Analytics",
        ),
        (
            &[
                "doubleclick.net",
                "googleadservices.com",
                "googlesyndication.com",
                "2mdn.net",
                "admob.com",
            ],
            "doubleclick.net",
            "Google Advertising",
        ),
        (
            &["facebook.net", "connect.facebook.net", "pixel.facebook.com"],
            "facebook.net",
            "Meta Pixel & Social",
        ),
        (
            &["mc.yandex.ru", "an.yandex.ru", "awaps.yandex.net"],
            "mc.yandex.ru",
            "Yandex Metrika",
        ),
        (&["hotjar.com"], "hotjar.com", "Hotjar Behavioral"),
        (&["clarity.ms"], "clarity.ms", "Microsoft Clarity"),
        (
            &["criteo.com", "criteo.net"],
            "criteo.com",
            "Criteo Retargeting",
        ),
        (
            &["scorecardresearch.com"],
            "scorecardresearch.com",
            "ScorecardResearch",
        ),
        (
            &["adnxs.com", "appnexus.com"],
            "adnxs.com",
            "AppNexus Ad Exchange",
        ),
        (&["taboola.com"], "taboola.com", "Taboola Recommendations"),
        (
            &["outbrain.com"],
            "outbrain.com",
            "Outbrain Recommendations",
        ),
        (
            &["amazon-adsystem.com"],
            "amazon-adsystem.com",
            "Amazon AdSystem",
        ),
        (&["adroll.com"], "adroll.com", "AdRoll Retargeting"),
        (&["quantserve.com"], "quantserve.com", "Quantcast Analytics"),
        (
            &["segment.io", "segment.com"],
            "segment.io",
            "Segment Telemetry",
        ),
        (&["mixpanel.com"], "mixpanel.com", "Mixpanel Analytics"),
        (&["amplitude.com"], "amplitude.com", "Amplitude Analytics"),
        (
            &["analytics.twitter.com", "ads-twitter.com"],
            "analytics.twitter.com",
            "Twitter Conversion",
        ),
        (&["top-fwz1.mail.ru"], "mail.ru", "Mail.ru Analytics"),
        (&["analytics.tiktok.com"], "tiktok.com", "TikTok Pixel"),
        (&["bat.bing.com"], "bing.com", "Microsoft Bing Ads"),
        (
            &["newrelic.com", "nr-data.net"],
            "newrelic.com",
            "New Relic Telemetry",
        ),
        (&["sentry.io"], "sentry.io", "Sentry Telemetry"),
    ];

    for (patterns, domain, rule) in trackers {
        for pattern in *patterns {
            if host == *pattern || host.ends_with(&format!(".{pattern}")) {
                return Some((domain.to_string(), rule.to_string()));
            }
        }
    }

    if (host.contains("tracker") || host.contains("telemetry") || host.contains("analytics"))
        && !host.contains("github")
        && !host.contains("gitlab")
    {
        return Some((host, "Generic Web Telemetry".to_string()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_tracker() {
        assert_eq!(
            classify_tracker("https://www.google-analytics.com/analytics.js"),
            Some((
                "google-analytics.com".to_string(),
                "Google Analytics".to_string()
            ))
        );
        assert_eq!(
            classify_tracker("https://connect.facebook.net/en_US/fbevents.js"),
            Some((
                "facebook.net".to_string(),
                "Meta Pixel & Social".to_string()
            ))
        );
        assert_eq!(
            classify_tracker("https://mc.yandex.ru/metrika/tag.js"),
            Some(("mc.yandex.ru".to_string(), "Yandex Metrika".to_string()))
        );
        assert_eq!(
            classify_tracker("https://c.clarity.ms/s/0.7.20/clarity.js"),
            Some(("clarity.ms".to_string(), "Microsoft Clarity".to_string()))
        );
        assert_eq!(
            classify_tracker("/gen_204?s=web&t=42"),
            Some((
                "google.com/gen_204".to_string(),
                "Google Telemetry (gen_204)".to_string()
            ))
        );
        assert_eq!(
            classify_tracker("https://www.google.com/gen_204?s=web"),
            Some((
                "google.com/gen_204".to_string(),
                "Google Telemetry (gen_204)".to_string()
            ))
        );
        assert_eq!(
            classify_tracker("https://ogads-pa.clients6.google.com/v1/user"),
            Some((
                "ogads-pa.clients6.google.com".to_string(),
                "Google Ad Personalization".to_string()
            ))
        );
        assert_eq!(classify_tracker("https://github.com/rust-lang/rust"), None);
    }
}
