//! Session transcript file repair.
//!
//! Detects and fixes common corruption patterns in JSON transcript files:
//! - Truncated JSON objects at end of file
//! - Duplicate entries from partial writes
//! - Missing closing brackets or delimiters
//!
//! Repair runs automatically when a corrupt transcript is detected during
//! loading. It preserves as much data as possible and logs what was repaired.

use tracing::{debug, info, warn};

/// Result of a repair attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairResult {
    /// Whether the content was modified.
    pub repaired: bool,
    /// Description of repairs performed.
    pub repairs: Vec<String>,
    /// The repaired content (same as input if no repair needed).
    pub content: String,
}

/// Attempt to repair a JSON transcript file's content.
///
/// The transcript is expected to be a JSON array of message objects.
/// Returns the repaired content and a log of what was fixed.
pub fn repair_transcript(raw: &str) -> RepairResult {
    let trimmed = raw.trim();

    // Empty or whitespace-only — return empty array.
    if trimmed.is_empty() {
        return RepairResult {
            repaired: !raw.is_empty(),
            repairs: if raw.is_empty() {
                vec![]
            } else {
                vec!["Empty content normalized to empty array".into()]
            },
            content: "[]".into(),
        };
    }

    // If it already parses as a valid JSON array, check for duplicates.
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        return dedup_entries(arr);
    }

    let mut repairs = Vec::new();
    let mut working = trimmed.to_owned();

    // Fix 1: Missing opening bracket.
    if !working.starts_with('[') {
        if working.starts_with('{') {
            working = format!("[{working}");
            repairs.push("Added missing opening bracket '['".into());
        } else {
            // Not recognizable JSON — wrap everything.
            warn!("Transcript content is not JSON array or object");
            return RepairResult {
                repaired: true,
                repairs: vec!["Content not recognizable as JSON, returned empty array".into()],
                content: "[]".into(),
            };
        }
    }

    // Fix 2: Missing closing bracket.
    if !working.ends_with(']') {
        // Try to find the last complete object by scanning for balanced braces.
        working = fix_truncated_array(&working, &mut repairs);
    }

    // Try to parse after bracket fixes.
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&working) {
        let dedup_result = dedup_entries(arr);
        repairs.extend(dedup_result.repairs);
        return RepairResult {
            repaired: true,
            repairs,
            content: dedup_result.content,
        };
    }

    // Fix 3: Try removing the last potentially corrupt entry.
    if let Some(fixed) = try_remove_last_entry(&working, &mut repairs) {
        return RepairResult {
            repaired: true,
            repairs,
            content: fixed,
        };
    }

    // Last resort: return empty array.
    repairs.push("Could not repair, returned empty array".into());
    info!(repair_count = repairs.len(), "transcript repair exhausted all strategies");

    RepairResult {
        repaired: true,
        repairs,
        content: "[]".into(),
    }
}

/// Fix a JSON array that's missing its closing bracket due to truncation.
///
/// Scans backwards from the end to find the last complete JSON object,
/// then closes the array after it.
fn fix_truncated_array(content: &str, repairs: &mut Vec<String>) -> String {
    // Find the last valid closing brace that could end a complete object.
    let mut depth = 0i32;
    let mut last_complete_end = None;
    let mut in_string = false;
    let mut prev_char = '\0';

    for (i, ch) in content.char_indices() {
        if in_string {
            if ch == '"' && prev_char != '\\' {
                in_string = false;
            }
            prev_char = ch;
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                // depth == 1 means we closed a top-level object inside the array
                // (the outer `[` counts as depth 1).
                if depth == 1 {
                    last_complete_end = Some(i);
                }
            }
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
        prev_char = ch;
    }

    if let Some(end_pos) = last_complete_end {
        let truncated = &content[..=end_pos];
        // Find the opening bracket.
        let result = format!("{truncated}]");
        repairs.push(format!(
            "Truncated after last complete object at byte {end_pos}, added closing ']'"
        ));
        debug!(byte_pos = end_pos, "truncated array fixed");
        result
    } else {
        // No complete object found — just close the bracket.
        repairs.push("No complete objects found, closing empty array".into());
        "[]".into()
    }
}

/// Try removing the last entry if it's corrupt.
fn try_remove_last_entry(content: &str, repairs: &mut Vec<String>) -> Option<String> {
    // Find the last comma that separates entries.
    let mut depth = 0i32;
    let mut last_comma_at_depth_1 = None;
    let mut in_string = false;
    let mut prev_char = '\0';

    for (i, ch) in content.char_indices() {
        if in_string {
            if ch == '"' && prev_char != '\\' {
                in_string = false;
            }
            prev_char = ch;
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 1 => last_comma_at_depth_1 = Some(i),
            _ => {}
        }
        prev_char = ch;
    }

    if let Some(comma_pos) = last_comma_at_depth_1 {
        let fixed = format!("{}]", &content[..comma_pos]);
        if serde_json::from_str::<Vec<serde_json::Value>>(&fixed).is_ok() {
            repairs.push("Removed last corrupt entry after trailing comma".into());
            return Some(fixed);
        }
    }

    // Try with just the opening bracket closed.
    let minimal = "[";
    if content.starts_with('[') {
        let fixed = format!("{minimal}]");
        if serde_json::from_str::<Vec<serde_json::Value>>(&fixed).is_ok() {
            repairs.push("Removed all entries (all corrupt), returned empty array".into());
            return Some(fixed);
        }
    }

    None
}

/// Remove duplicate entries from a transcript array.
///
/// Two entries are considered duplicates if they serialize to the same JSON.
fn dedup_entries(entries: Vec<serde_json::Value>) -> RepairResult {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::with_capacity(entries.len());
    let mut removed = 0usize;

    for entry in &entries {
        let key = serde_json::to_string(entry).unwrap_or_default();
        if seen.insert(key) {
            deduped.push(entry.clone());
        } else {
            removed += 1;
        }
    }

    let repaired = removed > 0;
    let repairs = if repaired {
        vec![format!("Removed {removed} duplicate entries")]
    } else {
        vec![]
    };

    let content = serde_json::to_string(&deduped).unwrap_or_else(|_| "[]".into());

    RepairResult {
        repaired,
        repairs,
        content,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_json_array_unchanged() {
        let input = r#"[{"role":"user","content":"hello"}]"#;
        let result = repair_transcript(input);
        assert!(!result.repaired);
        assert!(result.repairs.is_empty());
    }

    #[test]
    fn empty_string_returns_empty_array() {
        let result = repair_transcript("");
        assert!(!result.repaired);
        assert_eq!(result.content, "[]");
    }

    #[test]
    fn whitespace_only_returns_empty_array() {
        let result = repair_transcript("   \n  ");
        assert!(result.repaired);
        assert_eq!(result.content, "[]");
    }

    #[test]
    fn missing_closing_bracket() {
        let input = r#"[{"role":"user","content":"hello"}"#;
        let result = repair_transcript(input);
        assert!(result.repaired);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["role"], "user");
    }

    #[test]
    fn truncated_mid_object() {
        let input = r#"[{"role":"user","content":"hello"},{"role":"assistant","con"#;
        let result = repair_transcript(input);
        assert!(result.repaired);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.content).unwrap();
        // Should preserve the first complete object and drop the truncated one.
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["content"], "hello");
    }

    #[test]
    fn duplicate_entries_removed() {
        let input = r#"[{"role":"user","content":"hi"},{"role":"user","content":"hi"}]"#;
        let result = repair_transcript(input);
        assert!(result.repaired);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn multiple_valid_entries_preserved() {
        let input = r#"[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]"#;
        let result = repair_transcript(input);
        assert!(!result.repaired);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn missing_opening_bracket_for_object() {
        let input = r#"{"role":"user","content":"hello"}]"#;
        let result = repair_transcript(input);
        assert!(result.repaired);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn completely_corrupt_returns_empty() {
        let input = "this is not json at all";
        let result = repair_transcript(input);
        assert!(result.repaired);
        assert_eq!(result.content, "[]");
    }

    #[test]
    fn nested_objects_in_content_dont_confuse_parser() {
        let input = r#"[{"role":"user","content":"{\"key\": \"value\"}"}]"#;
        let result = repair_transcript(input);
        assert!(!result.repaired);
    }

    #[test]
    fn repairs_log_describes_actions() {
        let input = r#"[{"a":1},{"b":2"#;
        let result = repair_transcript(input);
        assert!(result.repaired);
        assert!(!result.repairs.is_empty());
        // At least one repair description exists.
        assert!(result.repairs.iter().any(|r| !r.is_empty()));
    }
}
