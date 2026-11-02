//! Client-side Anti-Fingerprinting Shield for Hardened profile.
//!
//! Injects a high-performance stealth UserScript into all frames at document `Start`.
//! This defeats browser fingerprinting techniques (Canvas, AudioContext, WebGL,
//! Navigator User-Agent, Hardware concurrency, Timezone, Fonts, Screen dimensions, Touch)
//! while ensuring full compatibility with standard web applications and anti-bot verification systems.

use webkit6::{UserContentInjectedFrames, UserScript, UserScriptInjectionTime};

/// Embedded stealth JavaScript payload that neutralizes fingerprinting vectors.
pub const ANTI_FINGERPRINT_SCRIPT: &str =
    include_str!("../../resources/scripts/anti_fingerprint.js");

/// Generates a 256-bit cryptographically secure random session secret from /dev/urandom.
pub fn generate_session_secret() -> String {
    let mut buf = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        if f.read_exact(&mut buf).is_ok() {
            return buf.iter().map(|b| format!("{:02x}", b)).collect();
        }
    }
    format!("{:016x}{:016x}", glib::monotonic_time(), std::process::id())
}

/// Instantiates the WebKit `UserScript` with a fresh session secret, configured to run at `Start` in `AllFrames`.
pub fn create_user_script() -> UserScript {
    let secret = generate_session_secret();
    create_user_script_with_secret(&secret)
}

/// Builds the stealth user script bound to the provided session secret.
pub fn create_user_script_with_secret(secret: &str) -> UserScript {
    let script = build_script_with_secret(secret);
    UserScript::new(
        &script,
        UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    )
}

/// Substitutes the session salt into the stealth JavaScript payload.
pub fn build_script_with_secret(secret: &str) -> String {
    ANTI_FINGERPRINT_SCRIPT.replace("/*__ECHO_SESSION_SECRET__*/", secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_is_non_empty_and_valid() {
        assert!(!ANTI_FINGERPRINT_SCRIPT.trim().is_empty());
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("farbledCanvases"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("SESSION_SALT"));
        assert!(!ANTI_FINGERPRINT_SCRIPT.contains("__echo_anti_fingerprint_installed__"));
        assert!(!ANTI_FINGERPRINT_SCRIPT.contains("__echo_farbled__"));

        let secret = generate_session_secret();
        assert_eq!(secret.len(), 64);
        let built = build_script_with_secret(&secret);
        assert!(built.contains(&secret));
        assert!(!built.contains("/*__ECHO_SESSION_SECRET__*/"));
    }

    #[test]
    fn test_user_agent_and_platform_spoofing() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("Macintosh; Intel Mac OS X 10_15_7"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("MacIntel"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("Apple Computer, Inc."));
    }

    #[test]
    fn test_timezone_and_date_masking() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("America/New_York"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("getTimezoneOffset"));
    }

    #[test]
    fn test_screen_normalization() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("1920"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("1080"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("colorDepth"));
    }

    #[test]
    fn test_font_shield_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("fonts.check"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("measureText"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("offsetWidth"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("offsetHeight"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("ALLOWED_MACOS_FONTS"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("'courier'"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("'palatino'"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("bitstream vera sans mono"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("bookman old style"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("palatino linotype"));
    }

    #[test]
    fn test_touch_support_disabled() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("maxTouchPoints"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("ontouchstart"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("TouchEvent"));
    }

    #[test]
    fn test_canvas_protection_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("getImageData"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("toDataURL"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("toBlob"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("createPrng"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("getDomainSeed"));
    }

    #[test]
    fn test_webgl_masking_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("UNMASKED_VENDOR_WEBGL"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("UNMASKED_RENDERER_WEBGL"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("Apple Inc."));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("Apple GPU"));
    }

    #[test]
    fn test_audio_protection_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("getChannelData"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("copyFromChannel"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("getFloatFrequencyData"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("getByteFrequencyData"));
    }

    #[test]
    fn test_stealth_wrappers_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("[native code]"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("registerNative"));
    }

    #[test]
    fn test_timer_coarsening_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("TIMER_RESOLUTION_MS"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("performance.now"));
    }

    #[test]
    fn test_media_devices_and_webrtc_protection() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("enumerateDevices"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("RTCPeerConnection"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("userAgentData"));
    }

    #[test]
    fn test_network_header_safeguard_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("setRequestHeader"));
        assert!(ANTI_FINGERPRINT_SCRIPT
            .contains("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"));
    }

    #[test]
    fn test_fingerprint_defense_telemetry_present() {
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("fingerprintDefended"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("recordDefense"));
        assert!(ANTI_FINGERPRINT_SCRIPT.contains("flushFingerprintBatch"));
    }
}
