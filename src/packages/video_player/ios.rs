use super::{VideoInfo, VideoPlayer};
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use std::collections::HashMap;
use std::sync::Mutex;

/// Global storage for AVPlayer instances keyed by player ID.
static PLAYERS: Mutex<Option<HashMap<u32, PlayerEntry>>> = Mutex::new(None);

/// Next available player ID.
static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

struct PlayerEntry {
    /// The AVPlayer instance (retained).
    player: *mut Object,
    /// Whether looping is enabled.
    looping: bool,
    /// The AVPlayerLayer added to the key window (if visible).
    layer: *mut Object,
}

// AVPlayer pointers are safe to send across threads when properly retained.
unsafe impl Send for PlayerEntry {}

fn with_players<T>(f: impl FnOnce(&mut HashMap<u32, PlayerEntry>) -> T) -> T {
    let mut guard = PLAYERS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    f(map)
}

fn with_player<T>(id: u32, f: impl FnOnce(&mut PlayerEntry) -> Result<T, String>) -> Result<T, String> {
    with_players(|map| {
        match map.get_mut(&id) {
            Some(entry) => f(entry),
            None => Err(format!("VideoPlayer {id} not found")),
        }
    })
}

pub fn create() -> Result<VideoPlayer, String> {
    unsafe {
        let player: *mut Object = msg_send![class!(AVPlayer), alloc];
        let player: *mut Object = msg_send![player, init];
        if player.is_null() {
            return Err("Failed to create AVPlayer".into());
        }

        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        with_players(|map| {
            map.insert(id, PlayerEntry {
                player,
                looping: false,
                layer: std::ptr::null_mut(),
            });
        });

        Ok(VideoPlayer { id })
    }
}

pub fn set_url(id: u32, url: &str) -> Result<VideoInfo, String> {
    with_player(id, |entry| {
        unsafe {
            let url_str = make_nsstring(url);
            let nsurl: *mut Object = msg_send![class!(NSURL), URLWithString: url_str];
            if nsurl.is_null() {
                let _: () = msg_send![url_str, release];
                return Err("Invalid URL".into());
            }

            let item: *mut Object = msg_send![class!(AVPlayerItem), playerItemWithURL: nsurl];
            let _: () = msg_send![entry.player, replaceCurrentItemWithPlayerItem: item];
            let _: () = msg_send![url_str, release];

            wait_for_item_ready(entry.player)?;
            get_video_info(entry.player)
        }
    })
}

pub fn set_file_path(id: u32, path: &str) -> Result<VideoInfo, String> {
    with_player(id, |entry| {
        unsafe {
            let path_str = make_nsstring(path);
            let nsurl: *mut Object = msg_send![class!(NSURL), fileURLWithPath: path_str];
            if nsurl.is_null() {
                let _: () = msg_send![path_str, release];
                return Err("Invalid file path".into());
            }

            let item: *mut Object = msg_send![class!(AVPlayerItem), playerItemWithURL: nsurl];
            let _: () = msg_send![entry.player, replaceCurrentItemWithPlayerItem: item];
            let _: () = msg_send![path_str, release];

            wait_for_item_ready(entry.player)?;
            get_video_info(entry.player)
        }
    })
}

pub fn play(id: u32) -> Result<(), String> {
    with_player(id, |entry| {
        unsafe {
            let _: () = msg_send![entry.player, play];
        }
        Ok(())
    })
}

pub fn pause(id: u32) -> Result<(), String> {
    with_player(id, |entry| {
        unsafe {
            let _: () = msg_send![entry.player, pause];
        }
        Ok(())
    })
}

pub fn seek(id: u32, position_ms: u64) -> Result<(), String> {
    with_player(id, |entry| {
        unsafe {
            let time = make_cmtime(position_ms);
            let _: () = msg_send![entry.player, seekToTime: time];
        }
        Ok(())
    })
}

pub fn set_volume(id: u32, volume: f32) -> Result<(), String> {
    with_player(id, |entry| {
        unsafe {
            let _: () = msg_send![entry.player, setVolume: volume];
        }
        Ok(())
    })
}

pub fn set_speed(id: u32, speed: f32) -> Result<(), String> {
    with_player(id, |entry| {
        unsafe {
            let _: () = msg_send![entry.player, setRate: speed];
        }
        Ok(())
    })
}

pub fn set_looping(id: u32, looping: bool) -> Result<(), String> {
    with_player(id, |entry| {
        entry.looping = looping;
        // AVPlayer does not have a built-in loop property.
        // Looping is typically implemented via AVPlayerLooper or
        // NSNotification observation. For simplicity we set
        // actionAtItemEnd so the player pauses (NoAction keeps the
        // item loaded) and store the flag for the caller to poll.
        unsafe {
            // AVPlayerActionAtItemEnd: 0 = Advance, 1 = Pause, 2 = None
            let action: i64 = if looping { 2 } else { 1 };
            let _: () = msg_send![entry.player, setActionAtItemEnd: action];
        }
        Ok(())
    })
}

pub fn position(id: u32) -> Result<u64, String> {
    with_player(id, |entry| {
        unsafe {
            let time: CMTime = msg_send![entry.player, currentTime];
            Ok(cmtime_to_ms(time))
        }
    })
}

pub fn duration(id: u32) -> Result<u64, String> {
    with_player(id, |entry| {
        unsafe {
            let item: *mut Object = msg_send![entry.player, currentItem];
            if item.is_null() {
                return Err("No current item".into());
            }
            let dur: CMTime = msg_send![item, duration];
            Ok(cmtime_to_ms(dur))
        }
    })
}

pub fn video_size(id: u32) -> Result<(u32, u32), String> {
    with_player(id, |entry| {
        unsafe {
            let item: *mut Object = msg_send![entry.player, currentItem];
            if item.is_null() {
                return Err("No current item".into());
            }
            let size: CGSize = msg_send![item, presentationSize];
            Ok((size.width as u32, size.height as u32))
        }
    })
}

pub fn is_playing(id: u32) -> Result<bool, String> {
    with_player(id, |entry| {
        unsafe {
            let rate: f32 = msg_send![entry.player, rate];
            Ok(rate > 0.0)
        }
    })
}

pub fn show_surface(id: u32, x: f32, y: f32, width: f32, height: f32) -> Result<(), String> {
    with_player(id, |entry| {
        unsafe {
            // Remove existing layer if any
            if !entry.layer.is_null() {
                let _: () = msg_send![entry.layer, removeFromSuperlayer];
                let _: () = msg_send![entry.layer, release];
                entry.layer = std::ptr::null_mut();
            }

            // Create AVPlayerLayer
            let layer: *mut Object = msg_send![class!(AVPlayerLayer), playerLayerWithPlayer: entry.player];
            if layer.is_null() {
                return Err("Failed to create AVPlayerLayer".into());
            }
            let _: () = msg_send![layer, retain];

            // Set frame
            let frame = CGRect {
                origin: CGPoint { x: x as f64, y: y as f64 },
                size: CGSize { width: width as f64, height: height as f64 },
            };
            let _: () = msg_send![layer, setFrame: frame];

            // Set video gravity to aspect-fit
            let gravity = make_nsstring("AVLayerVideoGravityResizeAspect");
            let _: () = msg_send![layer, setVideoGravity: gravity];
            let _: () = msg_send![gravity, release];

            // Add to key window's layer
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            let key_window: *mut Object = msg_send![app, keyWindow];
            if key_window.is_null() {
                let _: () = msg_send![layer, release];
                return Err("No key window".into());
            }
            let window_layer: *mut Object = msg_send![key_window, layer];
            let _: () = msg_send![window_layer, addSublayer: layer];

            entry.layer = layer;
        }
        Ok(())
    })
}

pub fn hide_surface(id: u32) -> Result<(), String> {
    with_player(id, |entry| {
        unsafe {
            if !entry.layer.is_null() {
                let _: () = msg_send![entry.layer, removeFromSuperlayer];
                let _: () = msg_send![entry.layer, release];
                entry.layer = std::ptr::null_mut();
            }
        }
        Ok(())
    })
}

pub fn dispose(id: u32) -> Result<(), String> {
    let entry = with_players(|map| map.remove(&id));
    if let Some(entry) = entry {
        unsafe {
            // Remove layer if showing
            if !entry.layer.is_null() {
                let _: () = msg_send![entry.layer, removeFromSuperlayer];
                let _: () = msg_send![entry.layer, release];
            }
            let _: () = msg_send![entry.player, pause];
            let _: () = msg_send![entry.player, release];
        }
    }
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// CMTime layout as used by AVFoundation.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

/// CGPoint layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

/// CGSize layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

/// CGRect layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

/// CMTime flag indicating a valid time.
const CMTIME_FLAGS_VALID: u32 = 1;
/// CMTime flag indicating positive infinity.
const CMTIME_FLAGS_POSITIVE_INFINITY: u32 = 4;

/// Create a CMTime from milliseconds.
fn make_cmtime(ms: u64) -> CMTime {
    CMTime {
        value: ms as i64,
        timescale: 1000,
        flags: CMTIME_FLAGS_VALID,
        epoch: 0,
    }
}

/// Convert a CMTime to milliseconds.
fn cmtime_to_ms(time: CMTime) -> u64 {
    if time.flags & CMTIME_FLAGS_VALID == 0 {
        return 0;
    }
    if time.flags & CMTIME_FLAGS_POSITIVE_INFINITY != 0 {
        return 0;
    }
    if time.timescale <= 0 {
        return 0;
    }
    ((time.value as f64 / time.timescale as f64) * 1000.0).max(0.0) as u64
}

/// Create a retained NSString from a Rust `&str`.
unsafe fn make_nsstring(s: &str) -> *mut Object {
    let nsstring_class = class!(NSString);
    let bytes = s.as_ptr();
    let len = s.len();
    // NSUTF8StringEncoding = 4
    let obj: *mut Object = msg_send![nsstring_class, alloc];
    let obj: *mut Object = msg_send![obj, initWithBytes:bytes length:len encoding:4u64];
    obj
}

/// Wait (up to 5 seconds) for the current AVPlayerItem to reach ReadyToPlay status.
unsafe fn wait_for_item_ready(player: *mut Object) -> Result<(), String> {
    let item: *mut Object = msg_send![player, currentItem];
    if item.is_null() {
        return Err("No player item".into());
    }

    // AVPlayerItemStatus: 0 = Unknown, 1 = ReadyToPlay, 2 = Failed
    for _ in 0..100 {
        let status: i64 = msg_send![item, status];
        match status {
            1 => return Ok(()),
            2 => return Err("AVPlayerItem failed to load".into()),
            _ => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    Err("Timed out waiting for AVPlayerItem to become ready".into())
}

/// Extract video info from the current player item.
unsafe fn get_video_info(player: *mut Object) -> Result<VideoInfo, String> {
    let item: *mut Object = msg_send![player, currentItem];
    if item.is_null() {
        return Err("No player item".into());
    }

    let dur: CMTime = msg_send![item, duration];
    let size: CGSize = msg_send![item, presentationSize];

    Ok(VideoInfo {
        duration_ms: cmtime_to_ms(dur),
        width: size.width as u32,
        height: size.height as u32,
    })
}
