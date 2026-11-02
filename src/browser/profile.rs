use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserProfile {
    /// Work & High Performance mode:
    /// Maximum site compatibility, standard security, minimal content disruption.
    Work,

    /// Hardened Shield mode:
    /// Strict tracker & ad blocking, third-party cookie restrictions,
    /// anti-fingerprinting, isolated ephemeral state if desired.
    Hardened,
}

impl BrowserProfile {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Work => "Рабочий (Производительность)",
            Self::Hardened => "Защищенный (Приватность)",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Hardened => "hardened",
        }
    }

    pub fn enables_content_blocker(&self) -> bool {
        match self {
            Self::Work => false,
            Self::Hardened => true,
        }
    }

    pub fn enables_do_not_track(&self) -> bool {
        match self {
            Self::Work => true,
            Self::Hardened => true,
        }
    }

    pub fn third_party_cookies_allowed(&self) -> bool {
        match self {
            Self::Work => true,
            Self::Hardened => false,
        }
    }

    pub fn enables_url_tracking_protection(&self) -> bool {
        match self {
            Self::Work => false,
            Self::Hardened => true,
        }
    }

    pub fn enables_anti_fingerprinting(&self) -> bool {
        match self {
            Self::Work => false,
            Self::Hardened => true,
        }
    }
}
