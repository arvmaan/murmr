use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum PasteMethod {
    Wtype,
    Xdotool,
    Auto,
}

impl PasteMethod {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "wtype" => Self::Wtype,
            "xdotool" => Self::Xdotool,
            _ => Self::Auto,
        }
    }

    pub fn detect() -> Result<Self> {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            Ok(Self::Wtype)
        } else if std::env::var("DISPLAY").is_ok() {
            Ok(Self::Xdotool)
        } else {
            anyhow::bail!("no display server detected (need WAYLAND_DISPLAY or DISPLAY)")
        }
    }
}

/// Paste text at the current cursor position.
pub fn paste_text(text: &str, method: &PasteMethod) -> Result<()> {
    let resolved = match method {
        PasteMethod::Auto => PasteMethod::detect()?,
        other => other.clone(),
    };

    match resolved {
        PasteMethod::Wtype => paste_wtype(text),
        PasteMethod::Xdotool => paste_xdotool(text),
        PasteMethod::Auto => unreachable!(),
    }
}

fn paste_wtype(text: &str) -> Result<()> {
    // wtype types text character by character; for long text, use clipboard + paste
    // For now, use wl-copy + wtype Ctrl+V approach for reliability
    Command::new("wl-copy")
        .arg(text)
        .status()
        .context("failed to run wl-copy (is wl-clipboard installed?)")?;

    Command::new("wtype")
        .args(["-M", "ctrl", "v", "-m", "ctrl"])
        .status()
        .context("failed to run wtype (is wtype installed?)")?;

    Ok(())
}

fn paste_xdotool(text: &str) -> Result<()> {
    // Copy to clipboard then simulate Ctrl+V
    Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        })
        .context("failed to run xclip (is xclip installed?)")?;

    Command::new("xdotool")
        .args(["key", "ctrl+v"])
        .status()
        .context("failed to run xdotool (is xdotool installed?)")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paste_method_from_str() {
        assert_eq!(PasteMethod::from_str("wtype"), PasteMethod::Wtype);
        assert_eq!(PasteMethod::from_str("xdotool"), PasteMethod::Xdotool);
        assert_eq!(PasteMethod::from_str("auto"), PasteMethod::Auto);
        assert_eq!(PasteMethod::from_str("anything"), PasteMethod::Auto);
    }
}
