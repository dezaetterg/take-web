use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub favicon_url: Option<String>,
    pub folder: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub visit_time: i64,
    pub visit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingItem {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub preview_text: Option<String>,
    pub is_read: bool,
    pub added_at: i64,
    pub offline_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupTabEntry {
    pub id: i64,
    pub group_id: String,
    pub title: String,
    pub url: String,
    pub position: i32,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerStat {
    pub domain: String,
    pub tracker_rule: String,
    pub blocked_count: u64,
    pub last_blocked_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FingerprintStat {
    pub vector: String,
    pub defended_count: u64,
    pub last_defended_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrivacySummary {
    pub total_trackers_blocked: u64,
    pub unique_trackers_count: usize,
    pub days_period: u32,
    pub top_blocked_domains: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OmniboxItemType {
    Bookmark,
    History,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmniboxResult {
    pub title: String,
    pub url: String,
    pub item_type: OmniboxItemType,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitePermissionEntry {
    pub origin: String,
    pub permission_type: String,
    pub state: String,
    pub updated_at: i64,
}
