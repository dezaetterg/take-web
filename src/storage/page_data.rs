//! JSON payloads pushed into the internal pages. They only depend on a
//! `BrowserDatabase`, so they can be built on the background DB thread.

use serde_json::{json, Value};

use super::database::{BrowserDatabase, FAVORITES_FOLDER};

pub fn start_page_payload(db: &BrowserDatabase) -> Value {
    let tracker_count = db
        .get_privacy_summary(7)
        .map(|summary| summary.total_trackers_blocked)
        .unwrap_or_else(|err| {
            log::warn!("cannot read tracker stats: {err}");
            0
        });

    let fingerprint_count: u64 = db
        .get_fingerprint_stats()
        .map(|stats| stats.iter().map(|s| s.defended_count).sum())
        .unwrap_or(0);

    let reading_item = db
        .get_reading_list(true)
        .ok()
        .and_then(|items| items.into_iter().next())
        .map(|item| {
            json!({
                "title": item.title,
                "preview_text": item.preview_text,
                "domain": item.domain,
                "url": item.url,
            })
        });

    let favorites: Vec<Value> = db
        .get_bookmarks(Some(FAVORITES_FOLDER))
        .unwrap_or_else(|err| {
            log::warn!("cannot read favorites: {err}");
            Vec::new()
        })
        .into_iter()
        .map(|b| json!({ "title": b.title, "url": b.url }))
        .collect();

    json!({
        "tracker_count": tracker_count,
        "fingerprint_count": fingerprint_count,
        "total_privacy_defended": tracker_count + fingerprint_count,
        "reading_item": reading_item,
        "favorites": favorites,
    })
}

pub fn trackers_page_payload(db: &BrowserDatabase) -> Value {
    let summary_7d = db.get_privacy_summary(7).unwrap_or_default();
    let summary_all = db.get_privacy_summary(36500).unwrap_or_default();
    let all_stats = db.get_all_tracker_stats().unwrap_or_default();

    let mut top_trackers: Vec<Value> = summary_7d
        .top_blocked_domains
        .iter()
        .map(|(domain, count)| json!({ "domain": domain, "count": count }))
        .collect();

    if top_trackers.is_empty() {
        top_trackers = summary_all
            .top_blocked_domains
            .iter()
            .map(|(domain, count)| json!({ "domain": domain, "count": count }))
            .collect();
    }

    let stats_list: Vec<Value> = all_stats
        .iter()
        .map(|s| {
            json!({
                "domain": s.domain,
                "rule": s.tracker_rule,
                "blocked_count": s.blocked_count,
                "last_blocked_at": s.last_blocked_at,
            })
        })
        .collect();

    let privacy_config = db
        .get_setting("privacy_config")
        .unwrap_or(None)
        .unwrap_or_else(|| {
            json!({
                "level": 3,
                "mode": "simple",
                "custom_toggles": {
                    "ad_blocking": true,
                    "canvas_noise": true,
                    "webgl_masking": true,
                    "audio_jitter": true,
                    "font_shield": true,
                    "url_cleaning": true,
                    "touch_screen": true,
                    "header_spoof": true,
                    "webrtc_leak": true,
                    "anti_timing": true
                }
            })
            .to_string()
        });

    let fp_stats = db.get_fingerprint_stats().unwrap_or_default();
    let total_fingerprint_defended: u64 = fp_stats.iter().map(|s| s.defended_count).sum();
    let mut fp_map = serde_json::Map::new();
    for s in &fp_stats {
        fp_map.insert(s.vector.clone(), json!(s.defended_count));
    }

    json!({
        "total_blocked_7d": summary_7d.total_trackers_blocked,
        "total_blocked_all": summary_all.total_trackers_blocked,
        "unique_trackers": all_stats.len(),
        "top_trackers": top_trackers,
        "all_stats": stats_list,
        "privacy_config": privacy_config,
        "fingerprint_stats": Value::Object(fp_map),
        "fingerprint_total": total_fingerprint_defended,
    })
}

pub fn settings_page_payload(db: &BrowserDatabase) -> Value {
    let settings = db.get_all_settings().unwrap_or_default();
    let mut val = json!(settings);
    if let Some(map) = val.as_object_mut() {
        let installed = crate::browser::translator::list_installed_models();
        let models_json: Vec<Value> = installed
            .into_iter()
            .map(|m| {
                json!({
                    "pair": m.pair,
                    "source_lang": m.source_lang,
                    "target_lang": m.target_lang,
                    "size_bytes": m.size_bytes,
                    "size_formatted": crate::browser::translator::format_bytes(m.size_bytes),
                })
            })
            .collect();
        map.insert("bergamot_models".to_string(), Value::Array(models_json));
        map.insert(
            "bergamot_total_size".to_string(),
            Value::String(crate::browser::translator::format_bytes(
                crate::browser::translator::total_installed_bytes(),
            )),
        );
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_reflect_database_contents() {
        let db = BrowserDatabase::in_memory().unwrap();
        db.record_tracker_blocked_n("ads.example", "ads", 3)
            .unwrap();
        db.record_fingerprint_defended_n("canvas", 5).unwrap();
        db.record_fingerprint_defended_n("webgl", 2).unwrap();

        let start = start_page_payload(&db);
        assert_eq!(start["tracker_count"], 3);
        assert_eq!(start["fingerprint_count"], 7);
        assert_eq!(start["total_privacy_defended"], 10);
        assert!(start["favorites"].is_array());

        let trackers = trackers_page_payload(&db);
        assert_eq!(trackers["total_blocked_all"], 3);
        assert_eq!(trackers["unique_trackers"], 1);
        assert_eq!(trackers["fingerprint_total"], 7);
        assert_eq!(trackers["fingerprint_stats"]["canvas"], 5);
        assert_eq!(trackers["fingerprint_stats"]["webgl"], 2);
    }
}
