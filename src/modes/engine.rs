use super::context;
use super::extractor;
use super::registry::{ModeRegistry, TriggerMatch};
use crate::config::Config;
use crate::llm::client::OllamaClient;
use anyhow::Result;

/// Output routing for a mode's result.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputRoute {
    /// Paste at cursor (default).
    Paste,
    /// Copy to clipboard only.
    Clipboard,
    /// Write to a file.
    File(String),
    /// Pipe to a command's stdin.
    Exec(String),
}

impl OutputRoute {
    /// Parse an output route from a config string.
    pub fn from_config(s: Option<&str>) -> Self {
        match s {
            None | Some("paste") => Self::Paste,
            Some("clipboard") => Self::Clipboard,
            Some(s) if s.starts_with("file:") => Self::File(s["file:".len()..].to_string()),
            Some(s) if s.starts_with("exec:") => Self::Exec(s["exec:".len()..].to_string()),
            Some(_) => Self::Paste,
        }
    }
}

/// Result of processing dictation through the mode engine.
#[derive(Debug, Clone)]
pub struct ModeResult {
    pub text: String,
    pub route: OutputRoute,
    pub mode_name: String,
}

/// Process dictation through the mode engine.
/// Returns Some(ModeResult) if a trigger matched, None for normal cleanup.
pub async fn process_dictation(
    text: &str,
    config: &Config,
    client: &OllamaClient,
    last_output: Option<&str>,
) -> Result<Option<ModeResult>> {
    let registry = ModeRegistry::new(&config.modes);

    let trigger_match = match registry.match_trigger(text) {
        Some(m) => m,
        None => return Ok(None),
    };

    let mode = registry
        .get_mode(&trigger_match.mode_name)
        .ok_or_else(|| anyhow::anyhow!("mode '{}' not found in registry", trigger_match.mode_name))?
        .clone();

    tracing::info!(
        "mode triggered: {} (trigger: '{}')",
        mode.name,
        trigger_match.trigger
    );

    let result = execute_mode(
        &trigger_match,
        &mode.template,
        mode.output.as_deref(),
        config,
        client,
        last_output,
    )
    .await?;

    Ok(Some(ModeResult {
        text: result,
        route: OutputRoute::from_config(mode.output.as_deref()),
        mode_name: mode.name,
    }))
}

/// Execute a matched mode: extract slots, fill template, resolve context.
async fn execute_mode(
    trigger_match: &TriggerMatch,
    template: &str,
    _output: Option<&str>,
    config: &Config,
    client: &OllamaClient,
    last_output: Option<&str>,
) -> Result<String> {
    // Extract slots from the remaining text
    let slots = extractor::extract_slots(
        client,
        &config.llm.command_model,
        template,
        &trigger_match.remaining_text,
    )
    .await?;

    // Fill the template with extracted slots
    let filled = extractor::fill_template(template, &slots);

    // Resolve context variables
    let resolved = context::resolve_context_variables(&filled, last_output)?;

    Ok(resolved)
}

/// Route the mode result to its destination.
pub fn route_output(result: &ModeResult) -> Result<()> {
    match &result.route {
        OutputRoute::Paste => {
            // Caller handles paste
            Ok(())
        }
        OutputRoute::Clipboard => {
            // Copy to clipboard without pasting
            let method = if std::env::var("WAYLAND_DISPLAY").is_ok() {
                "wl-copy"
            } else {
                "xclip"
            };
            if method == "wl-copy" {
                std::process::Command::new("wl-copy")
                    .arg(&result.text)
                    .status()
                    .map_err(|e| anyhow::anyhow!("wl-copy failed: {}", e))?;
            } else {
                let mut child = std::process::Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| anyhow::anyhow!("xclip failed: {}", e))?;
                if let Some(ref mut stdin) = child.stdin {
                    use std::io::Write;
                    stdin.write_all(result.text.as_bytes())?;
                }
                child.wait()?;
            }
            Ok(())
        }
        OutputRoute::File(path) => {
            std::fs::write(path, &result.text)
                .map_err(|e| anyhow::anyhow!("failed to write to {}: {}", path, e))?;
            tracing::info!("mode output written to {}", path);
            Ok(())
        }
        OutputRoute::Exec(command) => {
            let mut child = std::process::Command::new("sh")
                .args(["-c", command])
                .stdin(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| anyhow::anyhow!("failed to exec '{}': {}", command, e))?;
            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                stdin.write_all(result.text.as_bytes())?;
            }
            child.wait()?;
            tracing::info!("mode output piped to '{}'", command);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_route_from_config() {
        assert_eq!(OutputRoute::from_config(None), OutputRoute::Paste);
        assert_eq!(OutputRoute::from_config(Some("paste")), OutputRoute::Paste);
        assert_eq!(
            OutputRoute::from_config(Some("clipboard")),
            OutputRoute::Clipboard
        );
        assert_eq!(
            OutputRoute::from_config(Some("file:/tmp/out.md")),
            OutputRoute::File("/tmp/out.md".to_string())
        );
        assert_eq!(
            OutputRoute::from_config(Some("exec:pbcopy")),
            OutputRoute::Exec("pbcopy".to_string())
        );
        assert_eq!(
            OutputRoute::from_config(Some("unknown")),
            OutputRoute::Paste
        );
    }

    #[test]
    fn test_route_output_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let result = ModeResult {
            text: "hello world".to_string(),
            route: OutputRoute::File(path.clone()),
            mode_name: "test".to_string(),
        };
        route_output(&result).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello world");
    }
}
