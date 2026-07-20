use crate::config::ModeConfig;

/// Registry of available prompt template modes (built-in + user-defined).
#[derive(Debug, Clone)]
pub struct ModeRegistry {
    modes: Vec<ModeConfig>,
}

/// Result of matching dictation against the registry.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerMatch {
    pub mode_name: String,
    pub trigger: String,
    pub remaining_text: String,
}

impl ModeRegistry {
    /// Create a registry from user config modes merged with built-in defaults.
    pub fn new(user_modes: &[ModeConfig]) -> Self {
        let mut modes = builtin_modes();
        // User modes override built-ins with the same name
        for user_mode in user_modes {
            if let Some(pos) = modes.iter().position(|m| m.name == user_mode.name) {
                modes[pos] = user_mode.clone();
            } else {
                modes.push(user_mode.clone());
            }
        }
        Self { modes }
    }

    /// Try to match dictation text against any mode's trigger phrases.
    /// Returns the first match found (longest trigger wins on ties).
    pub fn match_trigger(&self, text: &str) -> Option<TriggerMatch> {
        let text_lower = text.to_lowercase();
        let mut best_match: Option<TriggerMatch> = None;

        for mode in &self.modes {
            for trigger in &mode.triggers {
                let trigger_lower = trigger.to_lowercase();
                if text_lower.starts_with(&trigger_lower) {
                    let remaining = text[trigger.len()..].trim_start();
                    // Strip leading colon/dash if present
                    let remaining = remaining
                        .strip_prefix(':')
                        .or_else(|| remaining.strip_prefix('-'))
                        .unwrap_or(remaining)
                        .trim_start();

                    let candidate = TriggerMatch {
                        mode_name: mode.name.clone(),
                        trigger: trigger.clone(),
                        remaining_text: remaining.to_string(),
                    };

                    // Prefer longer triggers
                    if best_match
                        .as_ref()
                        .is_none_or(|b| trigger.len() > b.trigger.len())
                    {
                        best_match = Some(candidate);
                    }
                }
            }
        }

        best_match
    }

    /// Get a mode by name.
    pub fn get_mode(&self, name: &str) -> Option<&ModeConfig> {
        self.modes.iter().find(|m| m.name == name)
    }

    /// All registered modes.
    pub fn all_modes(&self) -> &[ModeConfig] {
        &self.modes
    }
}

/// Names of the built-in modes (which cannot be removed, only overridden).
pub fn builtin_names() -> Vec<String> {
    builtin_modes().into_iter().map(|m| m.name).collect()
}

/// Built-in default modes that are always available.
fn builtin_modes() -> Vec<ModeConfig> {
    vec![
        ModeConfig {
            name: "loop".to_string(),
            triggers: vec![
                "loop this".to_string(),
                "ralph this".to_string(),
                "iterate on".to_string(),
            ],
            description: "Convert a task into a persistence-gated iterative loop".to_string(),
            template: LOOP_TEMPLATE.to_string(),
            output: None,
        },
        ModeConfig {
            name: "review".to_string(),
            triggers: vec![
                "review this".to_string(),
                "check this".to_string(),
                "audit".to_string(),
            ],
            description: "Adversarial review with failure-mode checklist".to_string(),
            template: REVIEW_TEMPLATE.to_string(),
            output: None,
        },
        ModeConfig {
            name: "spec".to_string(),
            triggers: vec![
                "spec this".to_string(),
                "specify".to_string(),
                "make this precise".to_string(),
            ],
            description: "Turn vague intent into a pseudo-formal brief".to_string(),
            template: SPEC_TEMPLATE.to_string(),
            output: None,
        },
        ModeConfig {
            name: "fan".to_string(),
            triggers: vec![
                "fan out".to_string(),
                "parallel".to_string(),
                "explore angles".to_string(),
            ],
            description: "Generate a diverse parallel search brief".to_string(),
            template: FAN_TEMPLATE.to_string(),
            output: None,
        },
        ModeConfig {
            name: "command".to_string(),
            triggers: vec![
                "translate".to_string(),
                "summarize".to_string(),
                "rewrite".to_string(),
                "explain".to_string(),
            ],
            description: "Direct passthrough to LLM".to_string(),
            template: "{{raw_dictation}}".to_string(),
            output: None,
        },
    ]
}

const LOOP_TEMPLATE: &str = "\
OBJECTIVE: {{extracted_objective}}

SUCCESS PREDICATE: {{extracted_success_condition}}
This is a property of the finished artifact, not of your confidence in it.

DOES NOT COUNT:
{{generated_non_counting_outcomes}}

VERIFICATION: After each attempt, check the artifact against the success \
predicate with a concrete, repeatable test. A pass you cannot reproduce does \
not count as a pass.

PERSISTENCE: Assume a solution exists. Do not stop because the task is hard, \
because progress is slow, or because you have already tried several times. \
Stop only when the success predicate holds under verification.

SOURCES: Use external search for background and prior art only — never to \
obtain the answer the predicate must be satisfied by independently.

RETURN: Return only the artifact that passes verification. Do not return \
partial progress, a plan, a summary, or an explanation of difficulty.";

const REVIEW_TEMPLATE: &str = "\
TASK: Adversarially verify the following artifact. Your job is to REFUTE it, \
not to confirm it. Approach it fresh; do not trust its author's reasoning.

ARTIFACT:
{{context:clipboard_or_git_diff}}

FOCUS: {{extracted_focus}}

FAILURE-MODE CHECKLIST (audit against each — do not settle for generic quality notes):
{{generated_failure_modes}}

METHOD: For every claimed defect, give a concrete failing case — an input, \
state, or scenario that triggers it — not a vague concern. If you cannot \
construct one, do not report it. Treat \"looks fine\" as unverified, not passed.

RETURN: List only confirmed defects, each with its reproducing evidence, \
ordered by severity. If no defect survives scrutiny, say so explicitly rather \
than inventing minor nits.";

const SPEC_TEMPLATE: &str = "\
Convert the following intent into a rigorous task specification. Write the \
success predicate first; if you cannot state one checkable sentence, say so \
rather than inventing scope.

INTENT: \"{{raw_dictation}}\"

Output this exact structure:

DEFINITIONS: Every load-bearing term, defined starting from its degenerate and \
edge cases — the ones a lazy solution would exploit.

TASK: One exact success predicate, with quantifiers and scope. State what the \
finished artifact must satisfy, measured over what population or range.

DOES NOT COUNT: Enumerated near-misses that satisfy the wording but not the \
intent — narrowed scope, reduction to an unvalidated assumption, anecdotal or \
small-sample evidence, a plan or survey in place of the artifact.

VERIFICATION: A concrete, repeatable procedure that decides whether the \
predicate holds. Independent of the process that produced the artifact.

RETURN CONDITION: A predicate over the artifact, never over confidence. \
\"I believe it works\" is not a return condition; \"the verification passes\" is.";

const FAN_TEMPLATE: &str = "\
OBJECTIVE: {{extracted_objective}}

ORCHESTRATION:
- Begin with a genuinely diverse portfolio — at least 3 approach families that \
  differ by underlying mechanism, not by wording or role label.
- Keep early workers blind to any favored approach so they cannot converge on it.
- Track approaches in a registry grouped by idea. Mark a route blocked when it \
  stalls at a step as hard as the goal itself; reopen it only for a materially \
  new mechanism, not a retry.
- Develop approaches independently first; cross-pollinate only late.

APPROACH REGISTRY (seed — workers expand this):
{{generated_approaches}}

VERIFICATION: Adversarially audit every candidate with a fresh perspective, \
not self-critique. If independent workers agree, treat that as a signal they \
lacked diversity, not as confirmation the answer is right.

RETURN: Only a candidate that survives the adversarial audit. A status report \
on progress is not a candidate.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_builtins() {
        let registry = ModeRegistry::new(&[]);
        assert_eq!(registry.all_modes().len(), 5);
        assert!(registry.get_mode("loop").is_some());
        assert!(registry.get_mode("review").is_some());
        assert!(registry.get_mode("spec").is_some());
        assert!(registry.get_mode("fan").is_some());
        assert!(registry.get_mode("command").is_some());
    }

    #[test]
    fn test_trigger_match_basic() {
        let registry = ModeRegistry::new(&[]);
        let result = registry.match_trigger("loop this: get tests passing");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.mode_name, "loop");
        assert_eq!(m.remaining_text, "get tests passing");
    }

    #[test]
    fn test_trigger_match_case_insensitive() {
        let registry = ModeRegistry::new(&[]);
        let result = registry.match_trigger("Loop This: fix the bug");
        assert!(result.is_some());
        assert_eq!(result.unwrap().mode_name, "loop");
    }

    #[test]
    fn test_trigger_match_no_colon() {
        let registry = ModeRegistry::new(&[]);
        let result = registry.match_trigger("review this the auth module");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.mode_name, "review");
        assert_eq!(m.remaining_text, "the auth module");
    }

    #[test]
    fn test_trigger_match_none() {
        let registry = ModeRegistry::new(&[]);
        let result = registry.match_trigger("hello world this is normal dictation");
        assert!(result.is_none());
    }

    #[test]
    fn test_trigger_match_command_mode() {
        let registry = ModeRegistry::new(&[]);
        let result = registry.match_trigger("translate this to Spanish");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.mode_name, "command");
        assert_eq!(m.remaining_text, "this to Spanish");
    }

    #[test]
    fn test_user_modes_override_builtins() {
        let user_mode = ModeConfig {
            name: "loop".to_string(),
            triggers: vec!["custom loop".to_string()],
            description: "Custom loop".to_string(),
            template: "CUSTOM: {{raw_dictation}}".to_string(),
            output: None,
        };
        let registry = ModeRegistry::new(&[user_mode]);
        let mode = registry.get_mode("loop").unwrap();
        assert_eq!(mode.triggers, vec!["custom loop"]);
    }

    #[test]
    fn test_user_modes_added() {
        let user_mode = ModeConfig {
            name: "my-mode".to_string(),
            triggers: vec!["do my thing".to_string()],
            description: "My custom mode".to_string(),
            template: "Hello: {{raw_dictation}}".to_string(),
            output: Some("clipboard".to_string()),
        };
        let registry = ModeRegistry::new(&[user_mode]);
        assert_eq!(registry.all_modes().len(), 6);
        assert!(registry.get_mode("my-mode").is_some());
    }

    #[test]
    fn test_longest_trigger_wins() {
        let user_modes = vec![
            ModeConfig {
                name: "short".to_string(),
                triggers: vec!["do".to_string()],
                description: "".to_string(),
                template: "".to_string(),
                output: None,
            },
            ModeConfig {
                name: "long".to_string(),
                triggers: vec!["do something".to_string()],
                description: "".to_string(),
                template: "".to_string(),
                output: None,
            },
        ];
        let registry = ModeRegistry::new(&user_modes);
        let result = registry.match_trigger("do something specific");
        assert_eq!(result.unwrap().mode_name, "long");
    }
}
