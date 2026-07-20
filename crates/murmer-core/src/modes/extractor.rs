use crate::llm::client::{LlmClient, Message};
use anyhow::Result;

/// Extract structured slots from casual speech using the LLM.
/// Given a template with named placeholders and raw dictation,
/// asks the LLM to return JSON with slot values.
pub async fn extract_slots(
    client: &LlmClient,
    model: &str,
    template: &str,
    raw_text: &str,
) -> Result<std::collections::HashMap<String, String>> {
    let slots = find_template_slots(template);

    if slots.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // For simple passthrough templates, just use raw_dictation
    if slots == vec!["raw_dictation"] {
        let mut map = std::collections::HashMap::new();
        map.insert("raw_dictation".to_string(), raw_text.to_string());
        return Ok(map);
    }

    let slot_descriptions = slots
        .iter()
        .map(|s| describe_slot(s))
        .collect::<Vec<_>>()
        .join("\n");

    let system_prompt = format!(
        "You compile casual dictation into the slots of a rigorous prompt template. \
         The user speaks loosely; you turn their intent into sharp, checkable content.\n\
         \n\
         Fill exactly these slots — one JSON key per slot, no more, no fewer:\n\
         {}\n\
         \n\
         Rules:\n\
         - 'extracted_*' slots: interpret and sharpen the user's intent into a clear, \
         specific statement. Do not editorialize or add scope they did not ask for.\n\
         - 'generated_*' slots: generate the requested scaffolding grounded in the \
         extracted objective. Be concrete and domain-specific — generic filler is worse \
         than nothing. Follow the exact format named in each slot's description.\n\
         - 'raw_dictation': the exact input text, unchanged.\n\
         - Every slot MUST be present and non-empty. If a slot cannot be grounded in the \
         input, infer the most reasonable specific value rather than leaving it blank.\n\
         \n\
         Output ONLY a single valid JSON object. No markdown fences, no prose. Every value \
         is a JSON string; escape newlines inside a value as \\n so the JSON stays valid.\n\
         Example shape (values illustrative):\n\
         {{\"extracted_objective\": \"...\", \"extracted_success_condition\": \"...\"}}",
        slot_descriptions
    );

    let messages = vec![
        Message {
            role: "system".to_string(),
            content: system_prompt,
        },
        Message {
            role: "user".to_string(),
            content: raw_text.to_string(),
        },
    ];

    let response = client.chat(model, messages).await?;
    parse_slot_json(&response, &slots, raw_text)
}

/// Find all {{placeholder}} names in a template, excluding context: prefixed ones.
pub fn find_template_slots(template: &str) -> Vec<String> {
    let mut slots = Vec::new();
    let mut pos = 0;
    while let Some(start) = template[pos..].find("{{") {
        let abs_start = pos + start + 2;
        if let Some(end) = template[abs_start..].find("}}") {
            let slot_name = &template[abs_start..abs_start + end];
            // Skip context variables — those are resolved separately
            if !slot_name.starts_with("context:") && !slots.contains(&slot_name.to_string()) {
                slots.push(slot_name.to_string());
            }
            pos = abs_start + end + 2;
        } else {
            break;
        }
    }
    slots
}

/// Fill a template with slot values.
pub fn fill_template(template: &str, slots: &std::collections::HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in slots {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Describe what a slot expects, based on its name.
fn describe_slot(name: &str) -> String {
    match name {
        "extracted_objective" => {
            format!("- {}: The core task/objective from the user's speech", name)
        }
        "extracted_success_condition" => {
            format!(
                "- {}: A measurable, checkable condition for completion",
                name
            )
        }
        "extracted_focus" => {
            format!("- {}: The specific area or aspect to focus on", name)
        }
        "generated_non_counting_outcomes" => format!(
            "- {}: Generate 4-6 bullet points listing ways an agent might cheat or produce \
             answer-shaped non-solutions (e.g., deleting tests, narrowing scope, etc.). \
             Format as '- item' per line.",
            name
        ),
        "generated_failure_modes" => format!(
            "- {}: Generate 4-6 bullet points listing domain-specific ways the artifact could \
             be subtly wrong. Format as '- item' per line.",
            name
        ),
        "generated_approaches" => format!(
            "- {}: Generate 3-5 genuinely distinct approach families for solving the objective. \
             Format as numbered list.",
            name
        ),
        "raw_dictation" => {
            format!("- {}: The exact user speech (return as-is)", name)
        }
        _ => format!("- {}: Extract the relevant value from the speech", name),
    }
}

/// Parse the LLM's JSON response into a slot map.
fn parse_slot_json(
    response: &str,
    expected_slots: &[String],
    raw_text: &str,
) -> Result<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();

    // Always include raw_dictation
    map.insert("raw_dictation".to_string(), raw_text.to_string());

    // Try to find JSON in the response (LLM might wrap it in markdown)
    let json_str = extract_json_from_response(response);

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(serde_json::Value::Object(obj)) => {
            for (key, value) in obj {
                let str_value = match value {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                map.insert(key, str_value);
            }
        }
        _ => {
            tracing::warn!("failed to parse LLM slot extraction response as JSON");
        }
    }

    // Guarantee every declared slot has a value. A slot left unfilled would
    // otherwise leak its raw `{{placeholder}}` into the final prompt (fill_template
    // leaves unmatched placeholders verbatim). Whether the JSON failed entirely or
    // merely omitted a slot, fall back to a sensible default per slot kind.
    for slot in expected_slots {
        let missing = map.get(slot).map(|v| v.trim().is_empty()).unwrap_or(true);
        if missing {
            map.insert(slot.clone(), fallback_slot_value(slot, raw_text));
        }
    }

    Ok(map)
}

/// A safe default for a slot the LLM failed to produce, so the filled template is
/// still coherent rather than containing a stray `{{placeholder}}`.
fn fallback_slot_value(slot: &str, raw_text: &str) -> String {
    match slot {
        // Intent slots degrade to the user's own words — still meaningful.
        s if s.starts_with("extracted_") || s == "raw_dictation" => raw_text.to_string(),
        // Generated scaffolding can't be faked well; emit an honest placeholder line
        // the downstream agent can act on, never a broken template token.
        "generated_non_counting_outcomes" => {
            "- (none enumerated — apply your own judgment about answer-shaped non-solutions)"
                .to_string()
        }
        "generated_failure_modes" => {
            "- (none enumerated — audit for the domain's usual subtle-wrongness modes)".to_string()
        }
        "generated_approaches" => {
            "1. (none seeded — begin from genuinely distinct approach families)".to_string()
        }
        _ => raw_text.to_string(),
    }
}

/// Remove any template placeholders that were never filled, as a last-resort
/// safety net so a stray `{{slot}}` can never reach the user's cursor.
pub fn strip_unfilled_placeholders(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        if let Some(end_rel) = rest[start + 2..].find("}}") {
            result.push_str(&rest[..start]);
            rest = &rest[start + 2 + end_rel + 2..];
        } else {
            break;
        }
    }
    result.push_str(rest);
    result
}

/// Try to extract JSON from an LLM response that might have markdown wrapping.
fn extract_json_from_response(response: &str) -> String {
    let trimmed = response.trim();

    // Try direct parse first
    if trimmed.starts_with('{') {
        return trimmed.to_string();
    }

    // Try to find JSON in markdown code block
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }

    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if candidate.starts_with('{') {
                return candidate.to_string();
            }
        }
    }

    // Try to find a JSON object anywhere
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return trimmed[start..=end].to_string();
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_template_slots_basic() {
        let template = "TASK: {{extracted_objective}}\nSUCCESS: {{extracted_success_condition}}";
        let slots = find_template_slots(template);
        assert_eq!(
            slots,
            vec!["extracted_objective", "extracted_success_condition"]
        );
    }

    #[test]
    fn test_find_template_slots_skips_context() {
        let template = "{{context:clipboard}}\n{{extracted_focus}}\n{{context:git_diff}}";
        let slots = find_template_slots(template);
        assert_eq!(slots, vec!["extracted_focus"]);
    }

    #[test]
    fn test_find_template_slots_deduplicates() {
        let template = "{{slot_a}} and {{slot_a}} again";
        let slots = find_template_slots(template);
        assert_eq!(slots, vec!["slot_a"]);
    }

    #[test]
    fn test_find_template_slots_empty() {
        let slots = find_template_slots("no placeholders here");
        assert!(slots.is_empty());
    }

    #[test]
    fn test_fill_template() {
        let template = "TASK: {{objective}}\nDONE: {{condition}}";
        let mut slots = std::collections::HashMap::new();
        slots.insert("objective".to_string(), "fix tests".to_string());
        slots.insert("condition".to_string(), "all green".to_string());
        let result = fill_template(template, &slots);
        assert_eq!(result, "TASK: fix tests\nDONE: all green");
    }

    #[test]
    fn test_fill_template_missing_slot() {
        let template = "TASK: {{objective}}\nEXTRA: {{missing}}";
        let mut slots = std::collections::HashMap::new();
        slots.insert("objective".to_string(), "do thing".to_string());
        let result = fill_template(template, &slots);
        assert_eq!(result, "TASK: do thing\nEXTRA: {{missing}}");
    }

    #[test]
    fn test_extract_json_direct() {
        let response = r#"{"key": "value"}"#;
        assert_eq!(extract_json_from_response(response), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_markdown() {
        let response = "Here's the result:\n```json\n{\"key\": \"value\"}\n```\n";
        assert_eq!(extract_json_from_response(response), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_embedded() {
        let response = "The output is {\"key\": \"value\"} and that's it.";
        assert_eq!(extract_json_from_response(response), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_parse_slot_json_success() {
        let response =
            r#"{"extracted_objective": "fix auth", "extracted_success_condition": "tests pass"}"#;
        let slots = vec![
            "extracted_objective".to_string(),
            "extracted_success_condition".to_string(),
        ];
        let result = parse_slot_json(response, &slots, "raw input").unwrap();
        assert_eq!(result.get("extracted_objective").unwrap(), "fix auth");
        assert_eq!(
            result.get("extracted_success_condition").unwrap(),
            "tests pass"
        );
        assert_eq!(result.get("raw_dictation").unwrap(), "raw input");
    }

    #[test]
    fn test_parse_slot_json_invalid_falls_back() {
        let response = "not json at all";
        let slots = vec!["extracted_objective".to_string()];
        let result = parse_slot_json(response, &slots, "raw input").unwrap();
        assert_eq!(result.get("extracted_objective").unwrap(), "raw input");
    }

    #[test]
    fn test_parse_slot_json_fills_generated_on_failure() {
        // On a total JSON failure, generated_* slots must still be filled so no
        // raw {{placeholder}} can leak into the final prompt.
        let response = "the model rambled instead of returning JSON";
        let slots = vec![
            "extracted_objective".to_string(),
            "generated_non_counting_outcomes".to_string(),
        ];
        let result = parse_slot_json(response, &slots, "get tests passing").unwrap();
        assert_eq!(
            result.get("extracted_objective").unwrap(),
            "get tests passing"
        );
        assert!(!result
            .get("generated_non_counting_outcomes")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_parse_slot_json_fills_omitted_slot() {
        // Valid JSON that omits a declared slot must still get a fallback value.
        let response = r#"{"extracted_objective": "fix auth"}"#;
        let slots = vec![
            "extracted_objective".to_string(),
            "generated_failure_modes".to_string(),
        ];
        let result = parse_slot_json(response, &slots, "raw").unwrap();
        assert_eq!(result.get("extracted_objective").unwrap(), "fix auth");
        assert!(result.contains_key("generated_failure_modes"));
        assert!(!result
            .get("generated_failure_modes")
            .unwrap()
            .trim()
            .is_empty());
    }

    #[test]
    fn test_parse_slot_json_empty_value_gets_fallback() {
        // A slot present but blank should be treated as missing.
        let response = r#"{"extracted_objective": "   "}"#;
        let slots = vec!["extracted_objective".to_string()];
        let result = parse_slot_json(response, &slots, "do the thing").unwrap();
        assert_eq!(result.get("extracted_objective").unwrap(), "do the thing");
    }

    #[test]
    fn test_strip_unfilled_placeholders() {
        let text = "TASK: real content\nMISSING: {{never_filled}}\nEND";
        assert_eq!(
            strip_unfilled_placeholders(text),
            "TASK: real content\nMISSING: \nEND"
        );
    }

    #[test]
    fn test_strip_unfilled_placeholders_none() {
        let text = "nothing to strip here";
        assert_eq!(strip_unfilled_placeholders(text), text);
    }

    #[test]
    fn test_strip_unfilled_placeholders_unterminated() {
        // A stray unterminated "{{" is left as-is rather than eating the rest.
        let text = "keep this {{ and this";
        assert_eq!(strip_unfilled_placeholders(text), text);
    }
}
