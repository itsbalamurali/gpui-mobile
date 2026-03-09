//! Video playback for Android and iOS.
//!
//! Provides a cross-platform video player API backed by:
//! - Android: MediaPlayer via JNI
//! - iOS: AVPlayer via Objective-C
//!
//! Feature-gated behind `video_player`.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;

/// Video player state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoPlayerState {
    Uninitialized,
    Initialized,
    Playing,
    Paused,
    Completed,
    Error,
}

/// Video information returned after setting a source.
#[derive(Debug, Clone, Copy)]
pub struct VideoInfo {
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
}

/// A video player instance.
///
/// Each `VideoPlayer` owns a platform-specific media player identified by an
/// integer ID. Resources are released automatically on [`Drop`].
#[derive(Debug)]
#[allow(dead_code)]
pub struct VideoPlayer {
    id: u32,
}

impl VideoPlayer {
    /// Create a new video player.
    pub fn new() -> Result<Self, String> {
        #[cfg(target_os = "ios")]
        {
            ios::create()
        }
        #[cfg(target_os = "android")]
        {
            android::create()
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Err("VideoPlayer is not supported on this platform".into())
        }
    }

    /// Set video source from a URL.
    pub fn set_url(&self, url: &str) -> Result<VideoInfo, String> {
        #[cfg(target_os = "ios")]
        {
            ios::set_url(self.id, url)
        }
        #[cfg(target_os = "android")]
        {
            android::set_url(self.id, url)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = url;
            Err("not supported".into())
        }
    }

    /// Set video source from a file path.
    pub fn set_file_path(&self, path: &str) -> Result<VideoInfo, String> {
        #[cfg(target_os = "ios")]
        {
            ios::set_file_path(self.id, path)
        }
        #[cfg(target_os = "android")]
        {
            android::set_file_path(self.id, path)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = path;
            Err("not supported".into())
        }
    }

    /// Start or resume playback.
    pub fn play(&self) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::play(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::play(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Err("not supported".into())
        }
    }

    /// Pause playback.
    pub fn pause(&self) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::pause(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::pause(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Err("not supported".into())
        }
    }

    /// Seek to a position in milliseconds.
    pub fn seek(&self, position_ms: u64) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::seek(self.id, position_ms)
        }
        #[cfg(target_os = "android")]
        {
            android::seek(self.id, position_ms)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = position_ms;
            Err("not supported".into())
        }
    }

    /// Set volume (0.0 to 1.0).
    pub fn set_volume(&self, volume: f32) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::set_volume(self.id, volume)
        }
        #[cfg(target_os = "android")]
        {
            android::set_volume(self.id, volume)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = volume;
            Err("not supported".into())
        }
    }

    /// Set playback speed (e.g. 1.0 for normal, 2.0 for double speed).
    pub fn set_speed(&self, speed: f32) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::set_speed(self.id, speed)
        }
        #[cfg(target_os = "android")]
        {
            android::set_speed(self.id, speed)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = speed;
            Err("not supported".into())
        }
    }

    /// Enable or disable looping.
    pub fn set_looping(&self, looping: bool) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::set_looping(self.id, looping)
        }
        #[cfg(target_os = "android")]
        {
            android::set_looping(self.id, looping)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = looping;
            Err("not supported".into())
        }
    }

    /// Get current playback position in milliseconds.
    pub fn position(&self) -> Result<u64, String> {
        #[cfg(target_os = "ios")]
        {
            ios::position(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::position(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Err("not supported".into())
        }
    }

    /// Get total duration in milliseconds.
    pub fn duration(&self) -> Result<u64, String> {
        #[cfg(target_os = "ios")]
        {
            ios::duration(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::duration(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Err("not supported".into())
        }
    }

    /// Get video dimensions as `(width, height)`.
    pub fn video_size(&self) -> Result<(u32, u32), String> {
        #[cfg(target_os = "ios")]
        {
            ios::video_size(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::video_size(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Err("not supported".into())
        }
    }

    /// Check if currently playing.
    pub fn is_playing(&self) -> Result<bool, String> {
        #[cfg(target_os = "ios")]
        {
            ios::is_playing(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::is_playing(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Err("not supported".into())
        }
    }

    /// Show the native video surface at the given position and size (in logical pixels).
    ///
    /// On iOS this adds an AVPlayerLayer as a sublayer of the key window.
    /// On Android this adds a TextureView overlay positioned via FrameLayout params.
    /// The surface is placed above the GPUI Metal/Vulkan layer so the video is visible.
    /// Call [`hide_surface`] to remove it.
    pub fn show_surface(&self, x: f32, y: f32, width: f32, height: f32) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::show_surface(self.id, x, y, width, height)
        }
        #[cfg(target_os = "android")]
        {
            android::show_surface(self.id, x, y, width, height)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = (x, y, width, height);
            Err("not supported".into())
        }
    }

    /// Hide (remove) the native video surface.
    pub fn hide_surface(&self) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::hide_surface(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::hide_surface(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Ok(())
        }
    }

    /// Release player resources.
    ///
    /// Called automatically on [`Drop`], but can be invoked early to free
    /// resources sooner.
    pub fn dispose(&self) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            ios::dispose(self.id)
        }
        #[cfg(target_os = "android")]
        {
            android::dispose(self.id)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Ok(())
        }
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        let _ = self.dispose();
    }
}
