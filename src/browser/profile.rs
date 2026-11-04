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
    pub fn enables_content_blocker(&self) -> bool {
        match self {
            Self::Work => false,
            Self::Hardened => true,
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
