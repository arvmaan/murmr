use anyhow::{Context, Result};
use std::process::Command;

/// Method used to paste text at the current cursor position.
#[derive(Debug, Clone, PartialEq)]
pub enum PasteMethod {
    /// Wayland: wl-copy + wtype Ctrl+V
    Wtype,
    /// X11: xclip + xdotool key ctrl+v
    Xdotool,
    /// macOS: pbcopy + osascript Cmd+V
    MacOS,
    /// Auto-detect based on environment variables
    Auto,
}

impl PasteMethod {
    /// Parse a paste method from a configuration string.
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "wtype" | "wayland" => Self::Wtype,
            "xdotool" | "x11" | "xorg" => Self::Xdotool,
            "macos" | "pbcopy" => Self::MacOS,
            _ => Self::Auto,
        }
    }

    /// Detect the appropriate paste method from the display server environment.
    pub fn detect() -> Result<Self> {
        Self::detect_from_env(
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
            std::env::var("DISPLAY").ok().as_deref(),
        )
    }

    /// Detection logic separated for testability (no env var mutation needed).
    pub fn detect_from_env(wayland_display: Option<&str>, display: Option<&str>) -> Result<Self> {
        if cfg!(target_os = "macos") {
            return Ok(Self::MacOS);
        }
        match (wayland_display, display) {
            (Some(w), _) if !w.is_empty() => Ok(Self::Wtype),
            (_, Some(d)) if !d.is_empty() => Ok(Self::Xdotool),
            _ => anyhow::bail!("no display server detected (need WAYLAND_DISPLAY or DISPLAY)"),
        }
    }

    /// Check if the required paste tools are available on PATH.
    pub fn check_tools_available(&self) -> Vec<(&'static str, bool)> {
        match self {
            PasteMethod::Wtype => vec![
                ("wl-copy", is_command_available("wl-copy")),
                ("wtype", is_command_available("wtype")),
            ],
            PasteMethod::Xdotool => vec![
                ("xclip", is_command_available("xclip")),
                ("xdotool", is_command_available("xdotool")),
            ],
            PasteMethod::MacOS => vec![("pbcopy", is_command_available("pbcopy"))],
            PasteMethod::Auto => vec![],
        }
    }
}

/// Paste text at the current cursor position using the specified method.
pub fn paste_text(text: &str, method: &PasteMethod) -> Result<()> {
    let resolved = match method {
        PasteMethod::Auto => PasteMethod::detect()?,
        other => other.clone(),
    };

    match resolved {
        PasteMethod::Wtype => paste_wtype(text),
        PasteMethod::Xdotool => paste_xdotool(text),
        PasteMethod::MacOS => paste_macos(text),
        PasteMethod::Auto => unreachable!(),
    }
}

fn paste_wtype(text: &str) -> Result<()> {
    let status = Command::new("wl-copy")
        .arg(text)
        .status()
        .context("failed to run wl-copy (is wl-clipboard installed?)")?;

    if !status.success() {
        anyhow::bail!("wl-copy exited with status {}", status);
    }

    let status = Command::new("wtype")
        .args(["-M", "ctrl", "v", "-m", "ctrl"])
        .status()
        .context("failed to run wtype (is wtype installed?)")?;

    if !status.success() {
        anyhow::bail!("wtype exited with status {}", status);
    }

    Ok(())
}

fn paste_xdotool(text: &str) -> Result<()> {
    let mut child = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("failed to run xclip (is xclip installed?)")?;

    if let Some(ref mut stdin) = child.stdin {
        use std::io::Write;
        stdin
            .write_all(text.as_bytes())
            .context("failed to write to xclip stdin")?;
    }

    let status = child.wait().context("failed to wait for xclip")?;
    if !status.success() {
        anyhow::bail!("xclip exited with status {}", status);
    }

    let status = Command::new("xdotool")
        .args(["key", "ctrl+v"])
        .status()
        .context("failed to run xdotool (is xdotool installed?)")?;

    if !status.success() {
        anyhow::bail!("xdotool exited with status {}", status);
    }

    Ok(())
}

fn paste_macos(text: &str) -> Result<()> {
    // Copy to clipboard via pbcopy
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("failed to run pbcopy")?;

    if let Some(ref mut stdin) = child.stdin {
        use std::io::Write;
        stdin
            .write_all(text.as_bytes())
            .context("failed to write to pbcopy stdin")?;
    }

    let status = child.wait().context("failed to wait for pbcopy")?;
    if !status.success() {
        anyhow::bail!("pbcopy exited with status {}", status);
    }

    // Give the clipboard a moment to settle and the target app to be frontmost
    // before we synthesize the paste keystroke. Without this, fast apps
    // occasionally paste stale content or drop the event.
    std::thread::sleep(std::time::Duration::from_millis(120));

    // Simulate Cmd+V via osascript, retrying once — the first synthetic
    // keystroke can be dropped if focus is still settling after release.
    let mut last_err = String::new();
    for attempt in 0..2 {
        let output = Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to keystroke \"v\" using command down",
            ])
            .output()
            .context("failed to run osascript for Cmd+V")?;

        if output.status.success() {
            return Ok(());
        }

        last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::warn!(
            "osascript paste attempt {} failed: {}",
            attempt + 1,
            last_err
        );
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    anyhow::bail!(
        "osascript paste failed after retry: {}. Grant murmr Accessibility permission.",
        if last_err.is_empty() {
            "unknown error".to_string()
        } else {
            last_err
        }
    )
}

/// Check if a command exists on PATH.
fn is_command_available(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paste_method_from_str() {
        assert_eq!(PasteMethod::from_str("wtype"), PasteMethod::Wtype);
        assert_eq!(PasteMethod::from_str("Wtype"), PasteMethod::Wtype);
        assert_eq!(PasteMethod::from_str("wayland"), PasteMethod::Wtype);
        assert_eq!(PasteMethod::from_str("xdotool"), PasteMethod::Xdotool);
        assert_eq!(PasteMethod::from_str("x11"), PasteMethod::Xdotool);
        assert_eq!(PasteMethod::from_str("xorg"), PasteMethod::Xdotool);
        assert_eq!(PasteMethod::from_str("auto"), PasteMethod::Auto);
        assert_eq!(PasteMethod::from_str("anything"), PasteMethod::Auto);
        assert_eq!(PasteMethod::from_str(""), PasteMethod::Auto);
    }

    #[test]
    fn test_detect_wayland() {
        let result = PasteMethod::detect_from_env(Some("wayland-0"), None);
        assert_eq!(result.unwrap(), PasteMethod::Wtype);
    }

    #[test]
    fn test_detect_wayland_takes_priority() {
        let result = PasteMethod::detect_from_env(Some("wayland-0"), Some(":0"));
        assert_eq!(result.unwrap(), PasteMethod::Wtype);
    }

    #[test]
    fn test_detect_x11() {
        let result = PasteMethod::detect_from_env(None, Some(":0"));
        assert_eq!(result.unwrap(), PasteMethod::Xdotool);
    }

    #[test]
    fn test_detect_no_display() {
        let result = PasteMethod::detect_from_env(None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no display server"));
    }

    #[test]
    fn test_detect_empty_strings_treated_as_absent() {
        let result = PasteMethod::detect_from_env(Some(""), Some(""));
        assert!(result.is_err());
    }

    #[test]
    fn test_check_tools_wtype() {
        let method = PasteMethod::Wtype;
        let tools = method.check_tools_available();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].0, "wl-copy");
        assert_eq!(tools[1].0, "wtype");
    }

    #[test]
    fn test_check_tools_xdotool() {
        let method = PasteMethod::Xdotool;
        let tools = method.check_tools_available();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].0, "xclip");
        assert_eq!(tools[1].0, "xdotool");
    }
}
