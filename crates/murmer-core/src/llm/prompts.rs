use super::client::Message;

pub fn cleanup_messages(raw_text: &str, custom_system_prompt: Option<&str>) -> Vec<Message> {
    let system = custom_system_prompt.unwrap_or(DEFAULT_CLEANUP_PROMPT);
    vec![
        Message {
            role: "system".to_string(),
            content: system.to_string(),
        },
        Message {
            role: "user".to_string(),
            content: raw_text.to_string(),
        },
    ]
}

pub fn command_messages(instruction: &str) -> Vec<Message> {
    vec![
        Message {
            role: "system".to_string(),
            content: COMMAND_SYSTEM_PROMPT.to_string(),
        },
        Message {
            role: "user".to_string(),
            content: instruction.to_string(),
        },
    ]
}

const DEFAULT_CLEANUP_PROMPT: &str = "\
You are a dictation cleanup assistant. Your job is to clean up speech-to-text output.

Rules:
- Remove filler words: um, uh, like, you know, basically, actually, sort of, kind of
- Fix punctuation and capitalization
- Normalize spoken punctuation: 'period' → '.', 'comma' → ',', 'new line' → '\\n', \
  'exclamation point' → '!', 'question mark' → '?'
- Normalize numbers: 'twenty three' → '23', 'five dollars' → '$5'
- Honor self-corrections: 'no wait' or 'I mean' means use what follows, discard what preceded
- Preserve the speaker's meaning exactly — do NOT add, rephrase, or explain
- Output ONLY the cleaned text, nothing else";

const COMMAND_SYSTEM_PROMPT: &str = "\
You are a voice command assistant. The user will dictate an instruction. \
Execute the instruction and output ONLY the result. \
Do not explain what you did. Do not add commentary. \
If the instruction is unclear, make your best attempt.

Examples of instructions:
- 'translate to Spanish: hello world' → 'hola mundo'
- 'summarize: [long text]' → [brief summary]
- 'rewrite more formally: hey can u help' → 'Hello, could you please assist me?'
- 'make this a bullet list: first thing second thing third thing' → '• first thing\\n• second thing\\n• third thing'";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_messages_structure() {
        let msgs = cleanup_messages("um hello world", None);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content, "um hello world");
    }

    #[test]
    fn test_custom_system_prompt() {
        let custom = "Just fix punctuation.";
        let msgs = cleanup_messages("hello", Some(custom));
        assert_eq!(msgs[0].content, custom);
    }

    #[test]
    fn test_command_messages_structure() {
        let msgs = command_messages("translate to French: hello");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[1].content.contains("translate to French"));
    }
}
