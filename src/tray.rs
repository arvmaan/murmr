use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum TrayState {
    Idle,
    Recording,
    Processing,
}

pub struct SystemTray {
    state: TrayState,
}

impl SystemTray {
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: TrayState::Idle,
        })
    }

    pub fn state(&self) -> &TrayState {
        &self.state
    }

    pub fn set_state(&mut self, state: TrayState) {
        self.state = state;
        // TODO: Update tray icon/tooltip based on state
        // Idle: microphone icon (grey)
        // Recording: microphone icon (red/pulsing)
        // Processing: spinner or processing icon
    }

    pub fn run(&self) -> Result<()> {
        // TODO: Initialize ksni tray service
        // Register menu items: Quit, Settings (open config file)
        todo!("implement system tray")
    }
}
