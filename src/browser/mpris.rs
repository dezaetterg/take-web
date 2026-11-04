//! MPRIS D-Bus integration for TakeWeb (`org.mpris.MediaPlayer2.takeweb`).
//! Exposes media player controls (Play, Pause, Next, Previous) to desktop shells,
//! system tray media widgets, lockscreens, and keyboard media keys.

use std::sync::Arc;

use mpris_server::{
    zbus::{fdo, Result as ZbusResult},
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Time, TrackId, Volume,
};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Raise,
    Quit,
}

pub type MediaCommandCallback = Arc<dyn Fn(MediaCommand) + Send + Sync + 'static>;

#[derive(Debug, Clone)]
pub struct MediaState {
    pub status: PlaybackStatus,
    pub title: String,
    pub artist: String,
    pub url: String,
}

impl Default for MediaState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            title: "Take Web".to_string(),
            artist: String::new(),
            url: String::new(),
        }
    }
}

pub struct TakeMediaPlayer {
    state: Arc<RwLock<MediaState>>,
    command_cb: MediaCommandCallback,
}

impl RootInterface for TakeMediaPlayer {
    async fn identity(&self) -> fdo::Result<String> {
        Ok("Take Web".to_string())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("takeweb".to_string())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["http".into(), "https".into()])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![
            "audio/mpeg".into(),
            "audio/ogg".into(),
            "video/mp4".into(),
            "video/webm".into(),
        ])
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> ZbusResult<()> {
        Ok(())
    }

    async fn raise(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::Raise);
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::Quit);
        Ok(())
    }
}

impl PlayerInterface for TakeMediaPlayer {
    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        let state = self.state.read().await;
        Ok(state.status)
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }

    async fn set_loop_status(&self, _loop_status: LoopStatus) -> ZbusResult<()> {
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> ZbusResult<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_shuffle(&self, _shuffle: bool) -> ZbusResult<()> {
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let state = self.state.read().await;
        let mut m = Metadata::new();
        if let Ok(track_id) = TrackId::try_from("/org/mpris/MediaPlayer2/CurrentTrack") {
            m.set_trackid(Some(track_id));
        }
        if !state.title.is_empty() {
            m.set_title(Some(&state.title));
        }
        if !state.artist.is_empty() {
            m.set_artist(Some(vec![state.artist.as_str()]));
        }
        if !state.url.is_empty() {
            m.set_url(Some(&state.url));
        }
        Ok(m)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(1.0)
    }

    async fn set_volume(&self, _volume: Volume) -> ZbusResult<()> {
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        Ok(Time::ZERO)
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn next(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::Next);
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::Previous);
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::Pause);
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::PlayPause);
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::Stop);
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        (self.command_cb)(MediaCommand::Play);
        Ok(())
    }

    async fn seek(&self, _offset: Time) -> fdo::Result<()> {
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, _position: Time) -> fdo::Result<()> {
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }
}

pub struct MprisController {
    update_tx: tokio::sync::mpsc::UnboundedSender<MediaState>,
}

impl MprisController {
    pub fn new<F>(on_command: F) -> Self
    where
        F: Fn(MediaCommand) + Send + Sync + 'static,
    {
        let state = Arc::new(RwLock::new(MediaState::default()));
        let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<MediaState>();

        let state_clone = Arc::clone(&state);
        let command_cb: MediaCommandCallback = Arc::new(on_command);

        let handle = std::thread::Builder::new()
            .name("takeweb-mpris".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::warn!("Failed to create tokio runtime for MPRIS: {e}");
                        return;
                    }
                };

                rt.block_on(async move {
                    let player = TakeMediaPlayer {
                        state: Arc::clone(&state_clone),
                        command_cb,
                    };

                    let server = match Server::new("takeweb", player).await {
                        Ok(s) => {
                            log::info!("Registered org.mpris.MediaPlayer2.takeweb on D-Bus");
                            s
                        }
                        Err(e) => {
                            log::warn!("Failed to register MPRIS server (D-Bus unavailable): {e}");
                            return;
                        }
                    };

                    while let Some(new_state) = update_rx.recv().await {
                        let status_changed = {
                            let mut lock = state_clone.write().await;
                            let prev_status = lock.status;
                            *lock = new_state.clone();
                            prev_status != new_state.status
                        };

                        let mut m = Metadata::new();
                        if let Ok(track_id) =
                            TrackId::try_from("/org/mpris/MediaPlayer2/CurrentTrack")
                        {
                            m.set_trackid(Some(track_id));
                        }
                        if !new_state.title.is_empty() {
                            m.set_title(Some(&new_state.title));
                        }
                        if !new_state.artist.is_empty() {
                            m.set_artist(Some(vec![new_state.artist.as_str()]));
                        }
                        if !new_state.url.is_empty() {
                            m.set_url(Some(&new_state.url));
                        }

                        let mut props = vec![Property::Metadata(m)];
                        if status_changed {
                            props.push(Property::PlaybackStatus(new_state.status));
                        }

                        let _ = server.properties_changed(props).await;
                    }
                });
            });
        if let Err(err) = handle {
            log::warn!("failed to spawn mpris server thread: {err}");
        }

        Self { update_tx }
    }

    pub fn update_playback(&self, status: PlaybackStatus, title: &str, artist: &str, url: &str) {
        let new_state = MediaState {
            status,
            title: title.to_string(),
            artist: artist.to_string(),
            url: url.to_string(),
        };
        let _ = self.update_tx.send(new_state);
    }
}

pub const MEDIA_LISTENER_SCRIPT: &str = r#"
(function() {
    if (window.__take_media_hooked__) return;
    window.__take_media_hooked__ = true;

    function notifyMedia(event) {
        try {
            if (!window.webkit || !window.webkit.messageHandlers || !window.webkit.messageHandlers.takeMedia) return;
            let title = document.title || '';
            let artist = window.location.hostname || '';
            if (navigator.mediaSession && navigator.mediaSession.metadata) {
                if (navigator.mediaSession.metadata.title) title = navigator.mediaSession.metadata.title;
                if (navigator.mediaSession.metadata.artist) artist = navigator.mediaSession.metadata.artist;
            }
            window.webkit.messageHandlers.takeMedia.postMessage(JSON.stringify({
                event: event,
                title: title,
                artist: artist,
                url: window.location.href
            }));
        } catch (_) {}
    }

    document.addEventListener('play', (e) => {
        if (e.target instanceof HTMLMediaElement) notifyMedia('play');
    }, true);
    document.addEventListener('pause', (e) => {
        if (e.target instanceof HTMLMediaElement) notifyMedia('pause');
    }, true);
    document.addEventListener('ended', (e) => {
        if (e.target instanceof HTMLMediaElement) notifyMedia('ended');
    }, true);
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_listener_script_valid() {
        assert!(MEDIA_LISTENER_SCRIPT.contains("takeMedia"));
        assert!(MEDIA_LISTENER_SCRIPT.contains("mediaSession"));
        assert!(MEDIA_LISTENER_SCRIPT.contains("HTMLMediaElement"));
    }

    #[test]
    fn test_media_state_default() {
        let state = MediaState::default();
        assert_eq!(state.status, PlaybackStatus::Stopped);
        assert_eq!(state.title, "Take Web");
    }
}
