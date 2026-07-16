use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyEvent {
    DictatePressed,
    DictateReleased,
    CommandPressed,
    CommandReleased,
}

pub struct HotkeyListener {
    dictate_combo: String,
    command_combo: String,
}

impl HotkeyListener {
    pub fn new(dictate_combo: &str, command_combo: &str) -> Result<Self> {
        Ok(Self {
            dictate_combo: dictate_combo.to_string(),
            command_combo: command_combo.to_string(),
        })
    }

    pub fn dictate_combo(&self) -> &str {
        &self.dictate_combo
    }

    pub fn command_combo(&self) -> &str {
        &self.command_combo
    }

    /// Start listening for hotkey events. Calls the handler on press/release.
    pub async fn listen<F>(&self, _handler: F) -> Result<()>
    where
        F: FnMut(HotkeyEvent) + Send + 'static,
    {
        // TODO: Implement global hotkey listening
        // Options:
        // 1. rdev::listen() for cross-platform (X11 + some Wayland)
        // 2. evdev directly for Linux (works everywhere, needs input group)
        // 3. D-Bus GlobalShortcuts portal for Wayland compositors
        todo!("implement hotkey listener")
    }
}
