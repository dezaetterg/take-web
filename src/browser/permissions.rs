//! WebKit permission request management and site permission models.
//!
//! Handles runtime hardware/capability access requests (geolocation, microphone,
//! camera, notifications, pointer lock, cross-site data access) initiated by web pages.
//! Provides origin extraction, request type inspection, and decision persistence.

use gtk4::prelude::*;
use webkit6::{
    GeolocationPermissionRequest, MediaKeySystemPermissionRequest, NotificationPermissionRequest,
    PermissionRequest, PointerLockPermissionRequest, UserMediaPermissionRequest,
    WebsiteDataAccessPermissionRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionType {
    Geolocation,
    Microphone,
    Camera,
    CameraAndMicrophone,
    Notifications,
    PointerLock,
    WebsiteDataAccess,
    MediaKeySystem,
    Unknown,
}

impl PermissionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Geolocation => "geolocation",
            Self::Microphone => "microphone",
            Self::Camera => "camera",
            Self::CameraAndMicrophone => "camera_and_microphone",
            Self::Notifications => "notifications",
            Self::PointerLock => "pointer_lock",
            Self::WebsiteDataAccess => "website_data_access",
            Self::MediaKeySystem => "drm_media",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "geolocation" => Self::Geolocation,
            "microphone" => Self::Microphone,
            "camera" => Self::Camera,
            "camera_and_microphone" => Self::CameraAndMicrophone,
            "notifications" => Self::Notifications,
            "pointer_lock" => Self::PointerLock,
            "website_data_access" => Self::WebsiteDataAccess,
            "drm_media" => Self::MediaKeySystem,
            _ => Self::Unknown,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Geolocation => "доступ к вашему местоположению",
            Self::Microphone => "доступ к микрофону",
            Self::Camera => "доступ к веб-камере",
            Self::CameraAndMicrophone => "доступ к веб-камере и микрофону",
            Self::Notifications => "отправку системных уведомлений",
            Self::PointerLock => "захват указателя мыши",
            Self::WebsiteDataAccess => "доступ к cookie и данным хранилища",
            Self::MediaKeySystem => "доступ к защищённому DRM-контенту",
            Self::Unknown => "системное разрешение",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Geolocation => "find-location-symbolic",
            Self::Microphone => "audio-input-microphone-symbolic",
            Self::Camera | Self::CameraAndMicrophone => "camera-web-symbolic",
            Self::Notifications => "preferences-system-notifications-symbolic",
            Self::PointerLock => "input-mouse-symbolic",
            Self::WebsiteDataAccess => "drive-harddisk-symbolic",
            Self::MediaKeySystem => "media-optical-symbolic",
            Self::Unknown => "dialog-question-symbolic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequestInfo {
    pub origin: String,
    pub permission_type: PermissionType,
    pub description: String,
    pub icon_name: &'static str,
    pub extra_details: Option<String>,
}

/// Inspects a generic `webkit6::PermissionRequest` GInterface and identifies its
/// specific permission category and any extra context.
pub fn inspect_request(request: &PermissionRequest) -> (PermissionType, Option<String>) {
    if request
        .downcast_ref::<GeolocationPermissionRequest>()
        .is_some()
    {
        return (PermissionType::Geolocation, None);
    }

    if let Some(user_media) = request.downcast_ref::<UserMediaPermissionRequest>() {
        let audio = user_media.is_for_audio_device();
        let video = user_media.is_for_video_device();
        let perm_type = match (audio, video) {
            (true, true) => PermissionType::CameraAndMicrophone,
            (false, true) => PermissionType::Camera,
            (true, false) => PermissionType::Microphone,
            (false, false) => PermissionType::Unknown,
        };
        return (perm_type, None);
    }

    if request
        .downcast_ref::<NotificationPermissionRequest>()
        .is_some()
    {
        return (PermissionType::Notifications, None);
    }

    if request
        .downcast_ref::<PointerLockPermissionRequest>()
        .is_some()
    {
        return (PermissionType::PointerLock, None);
    }

    if let Some(storage) = request.downcast_ref::<WebsiteDataAccessPermissionRequest>() {
        let requesting = storage.requesting_domain().map(|d| d.to_string());
        let current = storage.current_domain().map(|d| d.to_string());
        let detail = match (requesting, current) {
            (Some(req), Some(curr)) if req != curr => Some(format!("{req} -> {curr}")),
            (Some(req), _) => Some(req),
            (_, Some(curr)) => Some(curr),
            _ => None,
        };
        return (PermissionType::WebsiteDataAccess, detail);
    }

    if request
        .downcast_ref::<MediaKeySystemPermissionRequest>()
        .is_some()
    {
        return (PermissionType::MediaKeySystem, None);
    }

    (PermissionType::Unknown, None)
}

/// Extracts a clean origin (e.g. `https://meet.google.com`) or domain from a URI string.
pub fn extract_origin_from_uri(uri: &str) -> String {
    let trimmed = uri.trim();
    if let Ok(parsed) = url::Url::parse(trimmed) {
        if parsed.scheme() == "echo" {
            let host = parsed.host_str().unwrap_or("internal");
            return format!("echo://{host}");
        }
        let origin = parsed.origin().ascii_serialization();
        if origin != "null" && !origin.is_empty() {
            return origin;
        }
        if let Some(host) = parsed.host_str() {
            return format!("{}://{}", parsed.scheme(), host);
        }
    }
    if let Some(without_scheme) = trimmed.split("://").nth(1) {
        let host = without_scheme.split('/').next().unwrap_or(trimmed);
        return host.to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_origin_from_uri() {
        assert_eq!(
            extract_origin_from_uri("https://meet.google.com/abc-xyz?auth=1"),
            "https://meet.google.com"
        );
        assert_eq!(
            extract_origin_from_uri("http://localhost:3000/dashboard"),
            "http://localhost:3000"
        );
        assert_eq!(
            extract_origin_from_uri("https://vk.com:8443/feed"),
            "https://vk.com:8443"
        );
        assert_eq!(
            extract_origin_from_uri("echo://settings"),
            "echo://settings"
        );
    }

    #[test]
    fn test_permission_type_roundtrip() {
        let types = [
            PermissionType::Geolocation,
            PermissionType::Microphone,
            PermissionType::Camera,
            PermissionType::CameraAndMicrophone,
            PermissionType::Notifications,
            PermissionType::PointerLock,
            PermissionType::WebsiteDataAccess,
            PermissionType::MediaKeySystem,
            PermissionType::Unknown,
        ];

        for p in types {
            assert_eq!(PermissionType::from_str(p.as_str()), p);
            assert!(!p.description().is_empty());
            assert!(!p.icon_name().is_empty());
        }
    }
}
