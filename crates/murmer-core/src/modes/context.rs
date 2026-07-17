use anyhow::Result;
use std::process::Command;

/// Resolve context variables in a template string.
/// Supported variables:
/// - {{context:clipboard}} — current clipboard contents
/// - {{context:git_diff}} — output of git diff
/// - {{context:git_staged}} — output of git diff --cached
/// - {{context:clipboard_or_git_diff}} — clipboard if non-empty, else git diff
/// - {{context:last_output}} — last output murmer pasted
/// - {{context:file:path}} — contents of a file
/// - {{context:shell:command}} — stdout of a shell command
pub fn resolve_context_variables(template: &str, last_output: Option<&str>) -> Result<String> {
    let mut result = template.to_string();

    // Simple context variables
    if result.contains("{{context:clipboard}}") {
        let clipboard = get_clipboard().unwrap_or_default();
        result = result.replace("{{context:clipboard}}", &clipboard);
    }

    if result.contains("{{context:git_diff}}") {
        let diff = run_command("git", &["diff"]).unwrap_or_default();
        result = result.replace("{{context:git_diff}}", &diff);
    }

    if result.contains("{{context:git_staged}}") {
        let staged = run_command("git", &["diff", "--cached"]).unwrap_or_default();
        result = result.replace("{{context:git_staged}}", &staged);
    }

    if result.contains("{{context:clipboard_or_git_diff}}") {
        let clipboard = get_clipboard().unwrap_or_default();
        let value = if clipboard.trim().is_empty() {
            run_command("git", &["diff"]).unwrap_or_default()
        } else {
            clipboard
        };
        result = result.replace("{{context:clipboard_or_git_diff}}", &value);
    }

    if result.contains("{{context:last_output}}") {
        let value = last_output.unwrap_or("");
        result = result.replace("{{context:last_output}}", value);
    }

    // File context: {{context:file:path}}
    while let Some(start) = result.find("{{context:file:") {
        let end = result[start..].find("}}").map(|e| start + e + 2);
        if let Some(end) = end {
            let placeholder = &result[start..end];
            let path = &placeholder["{{context:file:".len()..placeholder.len() - 2];
            let content = std::fs::read_to_string(path)
                .unwrap_or_else(|_| format!("[error: could not read file '{}']", path));
            result = result.replacen(placeholder, &content, 1);
        } else {
            break;
        }
    }

    // Shell context: {{context:shell:command}}
    while let Some(start) = result.find("{{context:shell:") {
        let end = result[start..].find("}}").map(|e| start + e + 2);
        if let Some(end) = end {
            let placeholder = &result[start..end];
            let cmd = &placeholder["{{context:shell:".len()..placeholder.len() - 2];
            let output =
                run_shell(cmd).unwrap_or_else(|_| format!("[error: command '{}' failed]", cmd));
            result = result.replacen(placeholder, &output, 1);
        } else {
            break;
        }
    }

    Ok(result)
}

/// Get clipboard contents using wl-paste (Wayland) or xclip (X11).
fn get_clipboard() -> Result<String> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        run_command("wl-paste", &["--no-newline"])
    } else {
        run_command("xclip", &["-selection", "clipboard", "-o"])
    }
}

/// Run a command and return its stdout.
fn run_command(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run {}: {}", program, e))?;

    if !output.status.success() {
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a shell command via sh -c and return stdout.
fn run_shell(cmd: &str) -> Result<String> {
    let output = Command::new("sh")
        .args(["-c", cmd])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run shell command: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_context_variables() {
        let template = "Hello, this has no variables.";
        let result = resolve_context_variables(template, None).unwrap();
        assert_eq!(result, template);
    }

    #[test]
    fn test_last_output_replacement() {
        let template = "Previous: {{context:last_output}}\nNew: hello";
        let result = resolve_context_variables(template, Some("previous text")).unwrap();
        assert_eq!(result, "Previous: previous text\nNew: hello");
    }

    #[test]
    fn test_last_output_none() {
        let template = "Previous: {{context:last_output}}";
        let result = resolve_context_variables(template, None).unwrap();
        assert_eq!(result, "Previous: ");
    }

    #[test]
    fn test_file_context_nonexistent() {
        let template = "Content: {{context:file:/nonexistent/file.txt}}";
        let result = resolve_context_variables(template, None).unwrap();
        assert!(result.contains("error: could not read file"));
    }

    #[test]
    fn test_file_context_real_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "file contents here").unwrap();
        let template = format!("Content: {{{{context:file:{}}}}}", tmp.path().display());
        let result = resolve_context_variables(&template, None).unwrap();
        assert_eq!(result, "Content: file contents here");
    }

    #[test]
    fn test_shell_context() {
        let template = "Output: {{context:shell:echo hello}}";
        let result = resolve_context_variables(template, None).unwrap();
        assert_eq!(result.trim(), "Output: hello");
    }

    #[test]
    fn test_multiple_context_variables() {
        let template = "A: {{context:last_output}} B: {{context:shell:echo world}}";
        let result = resolve_context_variables(template, Some("first")).unwrap();
        assert!(result.contains("A: first"));
        assert!(result.contains("B: world"));
    }

    #[test]
    fn test_git_diff_in_non_repo() {
        // git diff might return empty in a non-repo or clean repo - that's fine
        let template = "Diff: {{context:git_diff}}";
        let result = resolve_context_variables(template, None).unwrap();
        assert!(result.starts_with("Diff: "));
    }
}
