//! Small JSON query surface for Micro Apps.
//!
//! The App dialect has no objects or arrays, so the host exposes a path-based
//! reader instead of a general JSON value. This keeps network responses useful
//! without adding a second scripting model to MBC.
//!
//! Paths are dot-separated: `data.today_items.0.voc_spelling`, `errors.0.msg`.
//! A leading non-JSON prefix (for example an `HTTP 200\n` status line) is
//! skipped, so hosts can pass their raw response text through unchanged.

use serde_json::Value;

fn parse(text: &str) -> Option<Value> {
    let start = text.find(['{', '['])?;
    serde_json::from_str(&text[start..]).ok()
}

fn query<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for raw in path.split('.') {
        let segment = raw.trim();
        if segment.is_empty() {
            continue;
        }
        if let Ok(index) = segment.parse::<usize>() {
            current = current.get(index)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

/// Reads a scalar as text. Missing paths and JSON null return `""`.
#[must_use]
pub fn get_text(text: &str, path: &str) -> String {
    let parsed = parse(text);
    match parsed.as_ref().and_then(|value| query(value, path)) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(value @ (Value::Array(_) | Value::Object(_))) => value.to_string(),
        _ => String::new(),
    }
}

/// Reads a scalar as a number. Strings are parsed; booleans map to 1/0.
#[must_use]
pub fn get_number(text: &str, path: &str) -> f64 {
    match parse(text).as_ref().and_then(|value| query(value, path)) {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => text.parse().unwrap_or(0.0),
        Some(Value::Bool(flag)) => f64::from(u8::from(*flag)),
        _ => 0.0,
    }
}

/// Number of elements in an array, keys in an object, or bytes in a string.
#[must_use]
pub fn length(text: &str, path: &str) -> f64 {
    match parse(text).as_ref().and_then(|value| query(value, path)) {
        Some(Value::Array(items)) => items.len() as f64,
        Some(Value::Object(map)) => map.len() as f64,
        Some(Value::String(text)) => text.len() as f64,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::{get_number, get_text, length};

    const RESPONSE: &str = "HTTP 200\n{\"errors\":[],\"success\":true,\"data\":{\"today_items\":[{\"voc_spelling\":\"abandon\",\"is_finished\":false},{\"voc_spelling\":\"absorb\",\"is_finished\":true}],\"progress\":{\"finished\":1,\"total\":2}}}";

    #[test]
    fn skips_an_http_prefix_and_reads_nested_fields() {
        assert_eq!(
            get_text(RESPONSE, "data.today_items.0.voc_spelling"),
            "abandon"
        );
        assert_eq!(get_text(RESPONSE, "data.today_items.1.is_finished"), "true");
        assert_eq!(get_number(RESPONSE, "data.progress.total"), 2.0);
        assert_eq!(length(RESPONSE, "data.today_items"), 2.0);
    }

    #[test]
    fn missing_paths_are_empty_or_zero() {
        assert_eq!(get_text(RESPONSE, "errors.0.msg"), "");
        assert_eq!(get_number(RESPONSE, "missing"), 0.0);
        assert_eq!(length(RESPONSE, "data.progress"), 2.0);
    }

    #[test]
    fn malformed_json_is_rejected_without_panicking() {
        assert_eq!(get_text("not json", "a"), "");
        assert_eq!(get_number("HTTP 500\n", "a"), 0.0);
    }
}
