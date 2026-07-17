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
        "You are a slot extraction assistant. Given speech input, extract structured values.\n\
         \n\
         Extract these slots from the user's speech:\n\
         {}\n\
         \n\
         Rules:\n\
         - For 'extracted_*' slots: interpret and clean up the user's intent into a clear statement\n\
         - For 'generated_*' slots: generate appropriate content based on the extracted objective\n\
         - For 'raw_dictation': use the exact input text\n\
         \n\
         Respond with ONLY valid JSON. No markdown, no explanation. Example:\n\
         {{\"extracted_objective\": \"value\", \"extracted_success_condition\": \"value\"}}",
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
            // If JSON parsing fails, use the raw text for all extracted slots
            tracing::warn!("failed to parse LLM slot extraction response as JSON");
            for slot in expected_slots {
                if slot.starts_with("extracted_") && !map.contains_key(slot) {
                    map.insert(slot.clone(), raw_text.to_string());
                }
            }
        }
    }

    Ok(map)
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
}
