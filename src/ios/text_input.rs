//! iOS text input handling — external (Bluetooth / USB-C) keyboard support.
//!
//! iOS 13.4+ surfaces physical keyboard events through `UIKeyboardHIDUsage`
//! (a USB HID Usage Table subset) along with `UIKeyModifierFlags`.  This
//! module translates those raw values into the string-keyed `Keystroke`
//! structs that the rest of GPUI understands.
//!
//! Software-keyboard input (the on-screen keyboard) is handled separately:
//! characters typed there arrive as plain `NSString` text from the
//! `UITextInput` / `UIKeyInput` delegate and are converted directly into
//! `KeyDown` events in `IosWindow::handle_text_input`.
//!
//! # HID usage table reference
//! <https://www.usb.org/sites/default/files/documents/hut1_12v2.pdf>
//! – Table 12: Keyboard/Keypad Page (0x07)

// ---------------------------------------------------------------------------
// Modifiers
// ---------------------------------------------------------------------------

/// Keyboard-modifier state decoded from `UIKeyModifierFlags`.
///
/// `UIKeyModifierFlags` bit assignments (iOS 13.4+):
///
/// | Bit (shift) | Meaning              |
/// |-------------|----------------------|
/// | 16          | Caps Lock (α-shift)  |
/// | 17          | Shift                |
/// | 18          | Control              |
/// | 19          | Alternate (Option)   |
/// | 20          | Command              |
/// | 21          | Numeric-pad key      |
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    /// The ⌘ Command key (mapped to the "platform" modifier for GPUI parity).
    pub platform: bool,
    pub function: bool,
}

const FLAG_SHIFT: u32 = 1 << 17;
const FLAG_CONTROL: u32 = 1 << 18;
const FLAG_ALT: u32 = 1 << 19;
const FLAG_COMMAND: u32 = 1 << 20;

/// Decode a `UIKeyModifierFlags` bitmask into a [`Modifiers`] struct.
pub fn modifier_flags_to_modifiers(flags: u32) -> Modifiers {
    Modifiers {
        control: flags & FLAG_CONTROL != 0,
        alt: flags & FLAG_ALT != 0,
        shift: flags & FLAG_SHIFT != 0,
        platform: flags & FLAG_COMMAND != 0,
        function: false,
    }
}

// ---------------------------------------------------------------------------
// HID Usage → key string
// ---------------------------------------------------------------------------

/// Convert a `UIKeyboardHIDUsage` code into the canonical GPUI key string.
///
/// GPUI uses lowercase ASCII strings for printable characters
/// (`"a"`, `"1"`, `","` …) and named strings for special keys
/// (`"enter"`, `"backspace"`, `"f1"` …).
///
/// Unknown codes fall back to `"unknown-XX"` where `XX` is the hex usage id.
pub fn key_code_to_string(code: u32) -> String {
    match code {
        // ── Letters: HID 0x04 (a) … 0x1D (z) ───────────────────────────────
        0x04..=0x1D => {
            let c = (b'a' + (code - 0x04) as u8) as char;
            c.to_string()
        }

        // ── Digits: HID 0x1E (1) … 0x26 (9), 0x27 (0) ──────────────────────
        0x1E..=0x26 => {
            let digit = (b'1' + (code - 0x1E) as u8) as char;
            digit.to_string()
        }
        0x27 => "0".to_string(),

        // ── Special / editing keys ────────────────────────────────────────────
        0x28 => "enter".to_string(),
        0x29 => "escape".to_string(),
        0x2A => "backspace".to_string(),
        0x2B => "tab".to_string(),
        0x2C => " ".to_string(),

        // ── Punctuation ───────────────────────────────────────────────────────
        0x2D => "-".to_string(),
        0x2E => "=".to_string(),
        0x2F => "[".to_string(),
        0x30 => "]".to_string(),
        0x31 => "\\".to_string(),
        0x32 => "#".to_string(), // Non-US # and ~
        0x33 => ";".to_string(),
        0x34 => "'".to_string(),
        0x35 => "`".to_string(),
        0x36 => ",".to_string(),
        0x37 => ".".to_string(),
        0x38 => "/".to_string(),

        // ── Caps Lock ─────────────────────────────────────────────────────────
        0x39 => "caps_lock".to_string(),

        // ── Function keys F1–F12 ─────────────────────────────────────────────
        0x3A => "f1".to_string(),
        0x3B => "f2".to_string(),
        0x3C => "f3".to_string(),
        0x3D => "f4".to_string(),
        0x3E => "f5".to_string(),
        0x3F => "f6".to_string(),
        0x40 => "f7".to_string(),
        0x41 => "f8".to_string(),
        0x42 => "f9".to_string(),
        0x43 => "f10".to_string(),
        0x44 => "f11".to_string(),
        0x45 => "f12".to_string(),

        // ── Navigation cluster ────────────────────────────────────────────────
        0x49 => "insert".to_string(),
        0x4A => "home".to_string(),
        0x4B => "pageup".to_string(),
        0x4C => "delete".to_string(), // Forward-delete (⌦)
        0x4D => "end".to_string(),
        0x4E => "pagedown".to_string(),

        // ── Arrow keys ────────────────────────────────────────────────────────
        0x4F => "right".to_string(),
        0x50 => "left".to_string(),
        0x51 => "down".to_string(),
        0x52 => "up".to_string(),

        // ── Numpad ────────────────────────────────────────────────────────────
        0x53 => "num_lock".to_string(),
        0x54 => "numpad_/".to_string(),
        0x55 => "numpad_*".to_string(),
        0x56 => "numpad_-".to_string(),
        0x57 => "numpad_+".to_string(),
        0x58 => "numpad_enter".to_string(),
        0x59 => "numpad_1".to_string(),
        0x5A => "numpad_2".to_string(),
        0x5B => "numpad_3".to_string(),
        0x5C => "numpad_4".to_string(),
        0x5D => "numpad_5".to_string(),
        0x5E => "numpad_6".to_string(),
        0x5F => "numpad_7".to_string(),
        0x60 => "numpad_8".to_string(),
        0x61 => "numpad_9".to_string(),
        0x62 => "numpad_0".to_string(),
        0x63 => "numpad_.".to_string(),

        // ── Function keys F13–F24 (extended keyboards) ───────────────────────
        0x68 => "f13".to_string(),
        0x69 => "f14".to_string(),
        0x6A => "f15".to_string(),
        0x6B => "f16".to_string(),
        0x6C => "f17".to_string(),
        0x6D => "f18".to_string(),
        0x6E => "f19".to_string(),
        0x6F => "f20".to_string(),
        0x70 => "f21".to_string(),
        0x71 => "f22".to_string(),
        0x72 => "f23".to_string(),
        0x73 => "f24".to_string(),

        // ── Modifier keys (sent as standalone key events on some keyboards) ───
        0xE0 => "control".to_string(), // Left Control
        0xE1 => "shift".to_string(),   // Left Shift
        0xE2 => "alt".to_string(),     // Left Alt / Option
        0xE3 => "meta".to_string(),    // Left GUI / Command
        0xE4 => "control".to_string(), // Right Control
        0xE5 => "shift".to_string(),   // Right Shift
        0xE6 => "alt".to_string(),     // Right Alt / Option
        0xE7 => "meta".to_string(),    // Right GUI / Command

        // ── Fallback ──────────────────────────────────────────────────────────
        other => format!("unknown-{:02x}", other),
    }
}

// ---------------------------------------------------------------------------
// Keystroke / event types
// ---------------------------------------------------------------------------

/// A fully-decoded keystroke carrying the key name, modifiers, and optional
/// printable character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keystroke {
    pub modifiers: Modifiers,
    /// The key name (e.g. `"a"`, `"enter"`, `"f5"`, `"up"`).
    pub key: String,
    /// The printable character produced by this keystroke, if any.
    ///
    /// `None` for non-printable keys (arrows, function keys, modifiers …).
    pub key_char: Option<String>,
}

/// A key-down event.
#[derive(Clone, Debug)]
pub struct KeyDownEvent {
    pub keystroke: Keystroke,
    /// `true` when the key is being held and this is a repeat event.
    pub is_held: bool,
    /// When `true`, consumers should prefer raw character input over
    /// key-binding lookup (used for IME / text-field input).
    pub prefer_character_input: bool,
}

/// A key-up event.
#[derive(Clone, Debug)]
pub struct KeyUpEvent {
    pub keystroke: Keystroke,
}

/// The platform-level input event produced by `text_input`.
#[derive(Clone, Debug)]
pub enum PlatformKeyEvent {
    KeyDown(KeyDownEvent),
    KeyUp(KeyUpEvent),
}

// ---------------------------------------------------------------------------
// Public constructors
// ---------------------------------------------------------------------------

/// Build a [`KeyDownEvent`] from a typed character.
///
/// Used when the soft keyboard delivers characters via `UIKeyInput`.
pub fn character_to_key_down(c: char) -> PlatformKeyEvent {
    let s = c.to_string();
    PlatformKeyEvent::KeyDown(KeyDownEvent {
        keystroke: Keystroke {
            modifiers: Modifiers::default(),
            key: s.clone(),
            key_char: Some(s),
        },
        is_held: false,
        prefer_character_input: true,
    })
}

/// Build a backspace [`KeyDownEvent`].
pub fn backspace_key_down() -> PlatformKeyEvent {
    PlatformKeyEvent::KeyDown(KeyDownEvent {
        keystroke: Keystroke {
            modifiers: Modifiers::default(),
            key: "backspace".to_string(),
            key_char: None,
        },
        is_held: false,
        prefer_character_input: false,
    })
}

/// Build a [`KeyDownEvent`] from a raw HID usage code and modifier flags.
pub fn key_code_to_key_down(key_code: u32, modifier_flags: u32) -> PlatformKeyEvent {
    let modifiers = modifier_flags_to_modifiers(modifier_flags);
    let key = key_code_to_string(key_code);
    let key_char = printable_key_char(&key, &modifiers);

    PlatformKeyEvent::KeyDown(KeyDownEvent {
        keystroke: Keystroke {
            modifiers,
            key: key.clone(),
            key_char,
        },
        is_held: false,
        prefer_character_input: false,
    })
}

/// Build a [`KeyUpEvent`] from a raw HID usage code and modifier flags.
pub fn key_code_to_key_up(key_code: u32, modifier_flags: u32) -> PlatformKeyEvent {
    let modifiers = modifier_flags_to_modifiers(modifier_flags);
    let key = key_code_to_string(key_code);
    let key_char = printable_key_char(&key, &modifiers);

    PlatformKeyEvent::KeyUp(KeyUpEvent {
        keystroke: Keystroke {
            modifiers,
            key,
            key_char,
        },
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Returns `Some(char)` when `key` represents a single printable character
/// that is not modified by Control or Command (which would make it a
/// keyboard shortcut rather than literal text input).
fn printable_key_char(key: &str, modifiers: &Modifiers) -> Option<String> {
    // Control / Command combos are shortcuts, not text.
    if modifiers.control || modifiers.platform {
        return None;
    }
    // Only single-character keys are printable.
    if key.chars().count() == 1 {
        let c = key.chars().next().unwrap();
        // Apply shift for letters → uppercase.
        if c.is_ascii_alphabetic() && modifiers.shift {
            return Some(c.to_ascii_uppercase().to_string());
        }
        return Some(key.to_string());
    }
    // Space is also printable.
    if key == " " {
        return Some(" ".to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── key_code_to_string ──────────────────────────────────────────────────

    #[test]
    fn letters_map_correctly() {
        assert_eq!(key_code_to_string(0x04), "a");
        assert_eq!(key_code_to_string(0x1D), "z");
        assert_eq!(key_code_to_string(0x08), "e"); // 0x08 - 0x04 = 4 → 'e'
    }

    #[test]
    fn digits_map_correctly() {
        assert_eq!(key_code_to_string(0x1E), "1");
        assert_eq!(key_code_to_string(0x26), "9");
        assert_eq!(key_code_to_string(0x27), "0");
    }

    #[test]
    fn special_keys_map_correctly() {
        assert_eq!(key_code_to_string(0x28), "enter");
        assert_eq!(key_code_to_string(0x29), "escape");
        assert_eq!(key_code_to_string(0x2A), "backspace");
        assert_eq!(key_code_to_string(0x2B), "tab");
        assert_eq!(key_code_to_string(0x2C), " ");
    }

    #[test]
    fn arrow_keys_map_correctly() {
        assert_eq!(key_code_to_string(0x4F), "right");
        assert_eq!(key_code_to_string(0x50), "left");
        assert_eq!(key_code_to_string(0x51), "down");
        assert_eq!(key_code_to_string(0x52), "up");
    }

    #[test]
    fn function_keys_map_correctly() {
        assert_eq!(key_code_to_string(0x3A), "f1");
        assert_eq!(key_code_to_string(0x45), "f12");
        assert_eq!(key_code_to_string(0x68), "f13");
        assert_eq!(key_code_to_string(0x73), "f24");
    }

    #[test]
    fn unknown_code_fallback() {
        assert_eq!(key_code_to_string(0xFF), "unknown-ff");
        assert_eq!(key_code_to_string(0x00), "unknown-00");
    }

    // ── modifier_flags_to_modifiers ─────────────────────────────────────────

    #[test]
    fn no_modifiers() {
        let m = modifier_flags_to_modifiers(0);
        assert!(!m.control && !m.alt && !m.shift && !m.platform);
    }

    #[test]
    fn shift_flag() {
        let m = modifier_flags_to_modifiers(FLAG_SHIFT);
        assert!(m.shift);
        assert!(!m.control && !m.alt && !m.platform);
    }

    #[test]
    fn command_flag() {
        let m = modifier_flags_to_modifiers(FLAG_COMMAND);
        assert!(m.platform);
        assert!(!m.shift && !m.control && !m.alt);
    }

    #[test]
    fn multiple_flags() {
        let flags = FLAG_CONTROL | FLAG_ALT | FLAG_SHIFT;
        let m = modifier_flags_to_modifiers(flags);
        assert!(m.control);
        assert!(m.alt);
        assert!(m.shift);
        assert!(!m.platform);
    }

    // ── character_to_key_down ───────────────────────────────────────────────

    #[test]
    fn character_event_is_prefer_char_input() {
        match character_to_key_down('x') {
            PlatformKeyEvent::KeyDown(ev) => {
                assert!(ev.prefer_character_input);
                assert_eq!(ev.keystroke.key, "x");
                assert_eq!(ev.keystroke.key_char, Some("x".to_string()));
            }
            _ => panic!("expected KeyDown"),
        }
    }

    // ── key_code_to_key_down ────────────────────────────────────────────────

    #[test]
    fn letter_key_down_no_modifiers() {
        match key_code_to_key_down(0x04, 0) {
            // 0x04 = 'a'
            PlatformKeyEvent::KeyDown(ev) => {
                assert_eq!(ev.keystroke.key, "a");
                assert_eq!(ev.keystroke.key_char, Some("a".to_string()));
                assert!(!ev.keystroke.modifiers.shift);
            }
            _ => panic!("expected KeyDown"),
        }
    }

    #[test]
    fn letter_key_down_with_shift_uppercases_key_char() {
        match key_code_to_key_down(0x04, FLAG_SHIFT) {
            // Shift + 'a' → key_char = "A"
            PlatformKeyEvent::KeyDown(ev) => {
                assert_eq!(ev.keystroke.key, "a"); // key name stays lowercase
                assert_eq!(ev.keystroke.key_char, Some("A".to_string()));
                assert!(ev.keystroke.modifiers.shift);
            }
            _ => panic!("expected KeyDown"),
        }
    }

    #[test]
    fn command_key_suppresses_key_char() {
        match key_code_to_key_down(0x04, FLAG_COMMAND) {
            PlatformKeyEvent::KeyDown(ev) => {
                // ⌘A is a shortcut; key_char should be None
                assert_eq!(ev.keystroke.key_char, None);
                assert!(ev.keystroke.modifiers.platform);
            }
            _ => panic!("expected KeyDown"),
        }
    }

    #[test]
    fn special_key_has_no_key_char() {
        match key_code_to_key_down(0x28, 0) {
            // Enter has no printable char
            PlatformKeyEvent::KeyDown(ev) => {
                assert_eq!(ev.keystroke.key, "enter");
                assert_eq!(ev.keystroke.key_char, None);
            }
            _ => panic!("expected KeyDown"),
        }
    }

    // ── key_code_to_key_up ──────────────────────────────────────────────────

    #[test]
    fn key_up_event_structure() {
        match key_code_to_key_up(0x51, 0) {
            // Down-arrow key up
            PlatformKeyEvent::KeyUp(ev) => {
                assert_eq!(ev.keystroke.key, "down");
                assert_eq!(ev.keystroke.key_char, None);
            }
            _ => panic!("expected KeyUp"),
        }
    }

    // ── backspace_key_down ──────────────────────────────────────────────────

    #[test]
    fn backspace_event_has_no_key_char() {
        match backspace_key_down() {
            PlatformKeyEvent::KeyDown(ev) => {
                assert_eq!(ev.keystroke.key, "backspace");
                assert_eq!(ev.keystroke.key_char, None);
            }
            _ => panic!("expected KeyDown"),
        }
    }
}
