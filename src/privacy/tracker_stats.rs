/// Classifies a resource URL against known tracker patterns and returns (domain, rule_description)
pub fn classify_tracker(url_str: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url_str).ok()?;
    let host = parsed.host_str()?.to_lowercase();

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
            &["mc.yandex.ru", "an.yandex.ru"],
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
        assert_eq!(classify_tracker("https://github.com/rust-lang/rust"), None);
    }
}
