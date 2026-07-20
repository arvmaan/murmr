use anyhow::Result;
use std::collections::HashSet;

/// Events emitted by the hotkey listener.
#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyEvent {
    DictatePressed,
    DictateReleased,
    CommandPressed,
    CommandReleased,
}

/// A parsed key combination (e.g., "Super+Shift+D" → modifiers + key).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyCombo {
    pub modifiers: HashSet<Modifier>,
    pub key: char,
}

/// Modifier keys that can be part of a hotkey combo.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Modifier {
    Super,
    Ctrl,
    Alt,
    Shift,
}

/// Parse a hotkey string like "Super+Shift+D" into a KeyCombo.
pub fn parse_hotkey(combo_str: &str) -> Result<KeyCombo> {
    let parts: Vec<&str> = combo_str.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        anyhow::bail!("empty hotkey string");
    }

    let mut modifiers = HashSet::new();
    let mut key_char = None;

    for part in &parts {
        match part.to_lowercase().as_str() {
            "super" | "meta" | "win" | "mod4" => {
                modifiers.insert(Modifier::Super);
            }
            "ctrl" | "control" => {
                modifiers.insert(Modifier::Ctrl);
            }
            "alt" | "mod1" => {
                modifiers.insert(Modifier::Alt);
            }
            "shift" => {
                modifiers.insert(Modifier::Shift);
            }
            s if s.len() == 1 => {
                if key_char.is_some() {
                    anyhow::bail!("multiple non-modifier keys in hotkey: '{}'", combo_str);
                }
                key_char = Some(s.chars().next().unwrap());
            }
            s => {
                anyhow::bail!("unknown key in hotkey '{}': '{}'", combo_str, s);
            }
        }
    }

    let key = key_char
        .ok_or_else(|| anyhow::anyhow!("no key character found in hotkey '{}'", combo_str))?;

    Ok(KeyCombo { modifiers, key })
}

/// Global hotkey listener using rdev.
pub struct HotkeyListener {
    dictate_combo: KeyCombo,
    command_combo: KeyCombo,
}

impl HotkeyListener {
    /// Create a new hotkey listener for the given key combinations.
    pub fn new(dictate_str: &str, command_str: &str) -> Result<Self> {
        let dictate_combo = parse_hotkey(dictate_str)?;
        let command_combo = parse_hotkey(command_str)?;

        tracing::debug!("dictate hotkey: {:?}", dictate_combo);
        tracing::debug!("command hotkey: {:?}", command_combo);

        Ok(Self {
            dictate_combo,
            command_combo,
        })
    }

    /// The parsed dictate key combination.
    pub fn dictate_combo(&self) -> &KeyCombo {
        &self.dictate_combo
    }

    /// The parsed command key combination.
    pub fn command_combo(&self) -> &KeyCombo {
        &self.command_combo
    }

    /// Start listening for hotkey events. Calls the handler on press/release.
    ///
    /// This function blocks the calling thread. It should be spawned in a
    /// dedicated thread via `tokio::task::spawn_blocking`.
    #[cfg(feature = "hotkeys")]
    pub fn listen<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(HotkeyEvent) + Send + 'static,
    {
        use rdev::{listen, Event, EventType};

        let dictate_key = char_to_rdev_key(self.dictate_combo.key);
        let command_key = char_to_rdev_key(self.command_combo.key);
        let dictate_mods = self.dictate_combo.modifiers.clone();
        let command_mods = self.command_combo.modifiers.clone();

        let mut pressed_modifiers: HashSet<Modifier> = HashSet::new();

        listen(move |event: Event| match event.event_type {
            EventType::KeyPress(key) => {
                if let Some(modifier) = key_to_modifier(&key) {
                    pressed_modifiers.insert(modifier);
                } else if key == dictate_key && pressed_modifiers == dictate_mods {
                    handler(HotkeyEvent::DictatePressed);
                } else if key == command_key && pressed_modifiers == command_mods {
                    handler(HotkeyEvent::CommandPressed);
                }
            }
            EventType::KeyRelease(key) => {
                if let Some(modifier) = key_to_modifier(&key) {
                    pressed_modifiers.remove(&modifier);
                } else if key == dictate_key {
                    handler(HotkeyEvent::DictateReleased);
                } else if key == command_key {
                    handler(HotkeyEvent::CommandReleased);
                }
            }
            _ => {}
        })
        .map_err(|e| anyhow::anyhow!("hotkey listener failed: {:?}", e))
    }

    /// Placeholder when hotkeys feature is disabled.
    #[cfg(not(feature = "hotkeys"))]
    pub fn listen<F>(&self, _handler: F) -> Result<()>
    where
        F: FnMut(HotkeyEvent) + Send + 'static,
    {
        anyhow::bail!("hotkey listener not available (compiled without 'hotkeys' feature)")
    }
}

#[cfg(feature = "hotkeys")]
fn char_to_rdev_key(c: char) -> rdev::Key {
    match c.to_ascii_uppercase() {
        'A' => rdev::Key::KeyA,
        'B' => rdev::Key::KeyB,
        'C' => rdev::Key::KeyC,
        'D' => rdev::Key::KeyD,
        'E' => rdev::Key::KeyE,
        'F' => rdev::Key::KeyF,
        'G' => rdev::Key::KeyG,
        'H' => rdev::Key::KeyH,
        'I' => rdev::Key::KeyI,
        'J' => rdev::Key::KeyJ,
        'K' => rdev::Key::KeyK,
        'L' => rdev::Key::KeyL,
        'M' => rdev::Key::KeyM,
        'N' => rdev::Key::KeyN,
        'O' => rdev::Key::KeyO,
        'P' => rdev::Key::KeyP,
        'Q' => rdev::Key::KeyQ,
        'R' => rdev::Key::KeyR,
        'S' => rdev::Key::KeyS,
        'T' => rdev::Key::KeyT,
        'U' => rdev::Key::KeyU,
        'V' => rdev::Key::KeyV,
        'W' => rdev::Key::KeyW,
        'X' => rdev::Key::KeyX,
        'Y' => rdev::Key::KeyY,
        'Z' => rdev::Key::KeyZ,
        '0' => rdev::Key::Num0,
        '1' => rdev::Key::Num1,
        '2' => rdev::Key::Num2,
        '3' => rdev::Key::Num3,
        '4' => rdev::Key::Num4,
        '5' => rdev::Key::Num5,
        '6' => rdev::Key::Num6,
        '7' => rdev::Key::Num7,
        '8' => rdev::Key::Num8,
        '9' => rdev::Key::Num9,
        _ => rdev::Key::Unknown(c as u32),
    }
}

#[cfg(feature = "hotkeys")]
fn key_to_modifier(key: &rdev::Key) -> Option<Modifier> {
    match key {
        rdev::Key::MetaLeft | rdev::Key::MetaRight => Some(Modifier::Super),
        rdev::Key::ControlLeft | rdev::Key::ControlRight => Some(Modifier::Ctrl),
        rdev::Key::Alt | rdev::Key::AltGr => Some(Modifier::Alt),
        rdev::Key::ShiftLeft | rdev::Key::ShiftRight => Some(Modifier::Shift),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey_basic() {
        let combo = parse_hotkey("Super+Shift+D").unwrap();
        assert!(combo.modifiers.contains(&Modifier::Super));
        assert!(combo.modifiers.contains(&Modifier::Shift));
        assert_eq!(combo.modifiers.len(), 2);
        assert_eq!(combo.key, 'd');
    }

    #[test]
    fn test_parse_hotkey_ctrl_alt() {
        let combo = parse_hotkey("Ctrl+Alt+C").unwrap();
        assert!(combo.modifiers.contains(&Modifier::Ctrl));
        assert!(combo.modifiers.contains(&Modifier::Alt));
        assert_eq!(combo.key, 'c');
    }

    #[test]
    fn test_parse_hotkey_case_insensitive() {
        let combo = parse_hotkey("SUPER+shift+d").unwrap();
        assert!(combo.modifiers.contains(&Modifier::Super));
        assert!(combo.modifiers.contains(&Modifier::Shift));
        assert_eq!(combo.key, 'd');
    }

    #[test]
    fn test_parse_hotkey_aliases() {
        let combo = parse_hotkey("Meta+Control+A").unwrap();
        assert!(combo.modifiers.contains(&Modifier::Super));
        assert!(combo.modifiers.contains(&Modifier::Ctrl));
        assert_eq!(combo.key, 'a');
    }

    #[test]
    fn test_parse_hotkey_single_key_fails() {
        let result = parse_hotkey("D");
        // Single key with no modifiers is valid
        assert!(result.is_ok());
        assert_eq!(result.unwrap().modifiers.len(), 0);
    }

    #[test]
    fn test_parse_hotkey_empty_fails() {
        let result = parse_hotkey("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hotkey_multiple_keys_fails() {
        let result = parse_hotkey("A+B");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hotkey_unknown_modifier_fails() {
        let result = parse_hotkey("Hyper+D");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hotkey_with_spaces() {
        let combo = parse_hotkey("Super + Shift + D").unwrap();
        assert!(combo.modifiers.contains(&Modifier::Super));
        assert_eq!(combo.key, 'd');
    }

    #[test]
    fn test_parse_hotkey_number_key() {
        let combo = parse_hotkey("Ctrl+1").unwrap();
        assert!(combo.modifiers.contains(&Modifier::Ctrl));
        assert_eq!(combo.key, '1');
    }

    #[test]
    fn test_hotkey_listener_creation() {
        let listener = HotkeyListener::new("Super+Shift+D", "Super+Shift+C").unwrap();
        assert_eq!(listener.dictate_combo().key, 'd');
        assert_eq!(listener.command_combo().key, 'c');
    }

    #[test]
    fn test_hotkey_listener_invalid_combo() {
        let result = HotkeyListener::new("", "Super+C");
        assert!(result.is_err());
    }
}
