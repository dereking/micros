//! Online dictionary lookup used by momo's study queue.
//!
//! Maimemo's OpenAPI returns word spellings but no definitions or examples, so
//! the host queries Youdao's public JSON dictionary. This module only depends
//! on the [`Transport`] abstraction; the macOS and ESP32 hosts share the URL
//! construction, batching and response flattening.

use serde_json::{Value as JsonValue, json};

use crate::{HttpRequest, MAX_REQUEST_BODY_BYTES, Transport, envelope};

/// Public Youdao JSON dictionary endpoint.
pub const YOUDAO_ENDPOINT: &str = "https://dict.youdao.com/jsonapi";

/// Upper bound on words per batch. Each word is a separate HTTPS request.
pub const MAX_WORDS: usize = 10;

/// Looks up newline-separated words and returns the same
/// `"HTTP <status>\n{\"entries\":[…]}"` envelope the App consumes.
pub fn lookup_words(transport: &dyn Transport, words: &str) -> String {
    let mut entries = Vec::new();
    let mut failures = 0;
    let mut seen = std::collections::BTreeSet::new();

    for word in words
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .take(MAX_WORDS)
    {
        if !seen.insert(word.to_ascii_lowercase()) {
            continue;
        }
        match fetch_youdao(transport, word) {
            Ok(entry) => entries.push(entry),
            Err(_) => failures += 1,
        }
    }

    if entries.is_empty() && failures > 0 {
        return envelope(0, "dictionary lookup failed");
    }
    let payload = json!({ "entries": entries });
    envelope(200, &payload.to_string())
}

fn fetch_youdao(transport: &dyn Transport, word: &str) -> Result<JsonValue, String> {
    let url = format!("{}?q={}", YOUDAO_ENDPOINT, percent_encode(word));
    let request = HttpRequest {
        method: "GET".into(),
        url,
        headers: vec![
            ("User-Agent".into(), "micros-momo/1.0".into()),
            ("Accept".into(), "application/json".into()),
        ],
        body: String::new(),
    };
    debug_assert!(request.body.len() <= MAX_REQUEST_BODY_BYTES);
    let response = transport.execute(&request)?;
    if response.status == 0 {
        return Err(response.body);
    }
    parse_youdao(word, &response.body)
}

/// Flattens the first `ec` entry of a Youdao response into the App's entry
/// shape. Missing fields become empty strings; malformed JSON is an error.
pub fn parse_youdao(word: &str, body: &str) -> Result<JsonValue, String> {
    let root: JsonValue = serde_json::from_str(body).map_err(|error| error.to_string())?;

    let entry = match root.pointer("/ec/word/0") {
        Some(ec) => {
            let mut definitions: Vec<String> = Vec::new();
            if let Some(trs) = ec.get("trs").and_then(JsonValue::as_array) {
                for tr in trs {
                    if let Some(lines) = tr.pointer("/tr/0/l/i").and_then(JsonValue::as_array) {
                        for line in lines {
                            if let Some(text) = line.as_str() {
                                let text = text.trim();
                                if !text.is_empty() && !definitions.iter().any(|item| item == text)
                                {
                                    definitions.push(text.to_owned());
                                }
                            }
                        }
                    }
                }
            }
            let phonetic = ec
                .get("ukphone")
                .or_else(|| ec.get("usphone"))
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim()
                .to_owned();
            let definition = truncate(&definitions.join("；"), 220);
            let example = root
                .pointer("/blng_sents_part/sentence-pair/0/sentence")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim()
                .to_owned();
            let translation = root
                .pointer("/blng_sents_part/sentence-pair/0/sentence-translation")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim()
                .to_owned();
            json!({
                "word": word,
                "phonetic": phonetic,
                "definition": definition,
                "example": example,
                "cloze": make_cloze(&example, word),
                "translation": translation,
            })
        }
        None => json!({
            "word": word,
            "phonetic": "",
            "definition": "",
            "example": "",
            "cloze": "",
            "translation": "",
        }),
    };
    Ok(entry)
}

fn truncate(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for (index, character) in text.chars().enumerate() {
        if index >= limit {
            out.push('…');
            break;
        }
        out.push(character);
    }
    out
}

/// Percent-encodes a word for the dictionary query string. Unreserved
/// characters stay literal; everything else is UTF-8 percent-encoded.
#[must_use]
pub fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Replaces the target word in an example sentence with a blank. Returns an
/// empty string when the word does not occur in the sentence.
#[must_use]
pub fn make_cloze(sentence: &str, word: &str) -> String {
    if sentence.is_empty() || word.is_empty() {
        return String::new();
    }
    let lower_sentence = sentence.to_ascii_lowercase();
    let lower_word = word.to_ascii_lowercase();
    match lower_sentence.find(&lower_word) {
        Some(start)
            if sentence.is_char_boundary(start)
                && sentence.is_char_boundary(start + word.len()) =>
        {
            format!(
                "{}______{}",
                &sentence[..start],
                &sentence[start + word.len()..]
            )
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpResponse;

    struct Dictionary;

    impl Transport for Dictionary {
        fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            assert!(request.url.starts_with(YOUDAO_ENDPOINT));
            assert_eq!(request.method, "GET");
            let body = r#"{"ec":{"word":[{"ukphone":"əˈbændən","trs":[{"tr":[{"l":{"i":["v. 放弃","n. 放任"]}}]}]}]},"blng_sents_part":{"sentence-pair":[{"sentence":"They had to abandon the plan.","sentence-translation":"他们不得不放弃这个计划。"}]}}"#;
            Ok(HttpResponse::new(200, body))
        }
    }

    #[test]
    fn youdao_json_is_flattened_for_the_app() {
        let body = r#"{"ec":{"word":[{"ukphone":"əˈbændən","trs":[{"tr":[{"l":{"i":["v. 放弃","n. 放任"]}}]}]}]},"blng_sents_part":{"sentence-pair":[{"sentence":"They had to abandon the plan.","sentence-translation":"他们不得不放弃这个计划。"}]}}"#;
        let entry = parse_youdao("abandon", body).expect("parse");
        assert_eq!(entry["word"], "abandon");
        assert_eq!(entry["phonetic"], "əˈbændən");
        assert_eq!(entry["definition"], "v. 放弃；n. 放任");
        assert_eq!(entry["cloze"], "They had to ______ the plan.");
        assert_eq!(entry["translation"], "他们不得不放弃这个计划。");
    }

    #[test]
    fn dictionary_words_are_percent_encoded() {
        assert_eq!(percent_encode("Mother's Day"), "Mother%27s%20Day");
        assert_eq!(
            make_cloze("Mother's Day is in May.", "Mother's Day"),
            "______ is in May."
        );
        assert_eq!(make_cloze("No target here.", "abandon"), "");
    }

    #[test]
    fn batch_lookup_returns_one_entry() {
        let response = lookup_words(&Dictionary, "abandon\nAbandon\n");
        assert!(response.starts_with("HTTP 200\n"), "{response}");
        let payload: JsonValue =
            serde_json::from_str(response.split_once('\n').expect("envelope").1).expect("json");
        assert_eq!(payload["entries"].as_array().expect("entries").len(), 1);
        assert_eq!(payload["entries"][0]["word"], "abandon");
    }

    #[test]
    fn a_missing_entry_keeps_an_empty_shape() {
        let entry = parse_youdao("missing", r#"{"ec":{"word":[]}}"#).expect("parse");
        assert_eq!(entry["word"], "missing");
        assert_eq!(entry["definition"], "");
    }
}
