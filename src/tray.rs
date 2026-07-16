use anyhow::Result;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Current state of the murmer system tray icon.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum TrayState {
    Idle = 0,
    Recording = 1,
    Processing = 2,
}

impl TrayState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Recording,
            2 => Self::Processing,
            _ => Self::Idle,
        }
    }

    /// Human-readable label for the state.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "murmer (idle)",
            Self::Recording => "murmer (recording...)",
            Self::Processing => "murmer (processing...)",
        }
    }
}

/// Thread-safe handle to update the tray state from other components.
#[derive(Clone)]
pub struct TrayHandle {
    state: Arc<AtomicU8>,
}

impl TrayHandle {
    /// Get the current tray state.
    pub fn state(&self) -> TrayState {
        TrayState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Update the tray state.
    pub fn set_state(&self, state: TrayState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }
}

/// System tray indicator for murmer.
pub struct SystemTray {
    state: Arc<AtomicU8>,
}

impl SystemTray {
    /// Create a new system tray instance.
    pub fn new() -> Result<(Self, TrayHandle)> {
        let state = Arc::new(AtomicU8::new(TrayState::Idle as u8));
        let handle = TrayHandle {
            state: state.clone(),
        };
        Ok((Self { state }, handle))
    }

    /// Get the current tray state.
    pub fn state(&self) -> TrayState {
        TrayState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Run the system tray service (blocking).
    ///
    /// On Linux, this uses the StatusNotifierItem D-Bus protocol (via ksni)
    /// to display a tray icon. Should be called from a dedicated thread.
    #[cfg(all(feature = "tray", target_os = "linux"))]
    pub fn run(&self) -> Result<()> {
        use ksni::TrayService;

        let tray = MurmerTray {
            state: self.state.clone(),
        };

        let service = TrayService::new(tray);
        service.spawn();

        // Block forever (tray runs in background)
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }

    /// Placeholder when tray feature is disabled.
    #[cfg(not(all(feature = "tray", target_os = "linux")))]
    pub fn run(&self) -> Result<()> {
        tracing::info!("system tray disabled (compiled without 'tray' feature)");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
}

#[cfg(all(feature = "tray", target_os = "linux"))]
struct MurmerTray {
    state: Arc<AtomicU8>,
}

#[cfg(all(feature = "tray", target_os = "linux"))]
impl ksni::Tray for MurmerTray {
    fn id(&self) -> String {
        "murmer".to_string()
    }

    fn title(&self) -> String {
        TrayState::from_u8(self.state.load(Ordering::Relaxed))
            .label()
            .to_string()
    }

    fn icon_name(&self) -> String {
        match TrayState::from_u8(self.state.load(Ordering::Relaxed)) {
            TrayState::Idle => "audio-input-microphone".to_string(),
            TrayState::Recording => "media-record".to_string(),
            TrayState::Processing => "process-working".to_string(),
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![ksni::menu::StandardItem {
            label: "Quit".to_string(),
            activate: Box::new(|_| std::process::exit(0)),
            ..Default::default()
        }
        .into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_state_conversion() {
        assert_eq!(TrayState::from_u8(0), TrayState::Idle);
        assert_eq!(TrayState::from_u8(1), TrayState::Recording);
        assert_eq!(TrayState::from_u8(2), TrayState::Processing);
        assert_eq!(TrayState::from_u8(255), TrayState::Idle); // unknown defaults to Idle
    }

    #[test]
    fn test_tray_state_labels() {
        assert_eq!(TrayState::Idle.label(), "murmer (idle)");
        assert_eq!(TrayState::Recording.label(), "murmer (recording...)");
        assert_eq!(TrayState::Processing.label(), "murmer (processing...)");
    }

    #[test]
    fn test_tray_handle() {
        let (_, handle) = SystemTray::new().unwrap();
        assert_eq!(handle.state(), TrayState::Idle);

        handle.set_state(TrayState::Recording);
        assert_eq!(handle.state(), TrayState::Recording);

        handle.set_state(TrayState::Processing);
        assert_eq!(handle.state(), TrayState::Processing);
    }

    #[test]
    fn test_system_tray_state() {
        let (tray, handle) = SystemTray::new().unwrap();
        assert_eq!(tray.state(), TrayState::Idle);

        handle.set_state(TrayState::Recording);
        assert_eq!(tray.state(), TrayState::Recording);
    }
}
