use gray_matter::{Matter, engine::YAML};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Serialize, Deserialize)]
pub struct ParseResult {
    pub frontmatter: Value,
    pub compiled_truth: String,
    pub timeline: String,
}

#[derive(Debug)]
pub struct MarkdownParser;

impl MarkdownParser {
    pub fn parse(content: &str) -> ParseResult {
        let matter = Matter::<YAML>::new();
        let result = matter.parse(content);

        let (frontmatter, body) = match result {
            Ok(parsed) => (parsed.data.unwrap_or_default(), parsed.content),
            Err(_) => (Value::Object(serde_json::Map::new()), content.to_string()),
        };
        let (compiled_truth, timeline) = Self::split_body(&body);

        ParseResult {
            frontmatter,
            compiled_truth,
            timeline,
        }
    }

    fn split_body(body: &str) -> (String, String) {
        match body.find("\n---\n") {
            Some(pos) => {
                let truth = body[..pos].trim().to_string();
                let timeline = body[pos + 5..].trim().to_string();
                (truth, timeline)
            }
            None => (body.trim().to_string(), String::new()),
        }
    }

    pub fn to_canonical(frontmatter: &Value, compiled_truth: &str, timeline: &str) -> String {
        let sorted_fm = Self::sort_frontmatter_key(frontmatter);
        let fm_str = serde_json::to_string(&sorted_fm).unwrap_or_default();
        format!("---\n{}\n---\n{}\n\n{}\n", fm_str, compiled_truth.trim(), timeline.trim())
    }

    pub fn content_hash(canonical: &str) -> String {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let normalized = canonical.nfc().collect::<String>();
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn sort_frontmatter_key(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut sorted: Vec<_> = map.iter().collect();
                sorted.sort_by_key(|(k, _)| *k);
                let mut new_map = serde_json::Map::new();
                for (k, v) in sorted {
                    new_map.insert(k.clone(), Self::sort_frontmatter_key(v));
                }
                Value::Object(new_map)
            }
            _ => value.clone(),
        }
    }

    pub fn normalize_slug(slug: &str) -> String {
        slug.nfc().collect::<String>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_with_frontmatter_and_divider() {
        let content = "---\ntype: concept\ntitle: Test\n---\ntruth content\n\n---\n\ntimeline content";
        let result = MarkdownParser::parse(content);
        assert_eq!(result.compiled_truth, "truth content");
        assert_eq!(result.timeline, "timeline content");
    }

    #[test]
    fn test_parse_without_divider() {
        let result = MarkdownParser::parse("just content");
        assert_eq!(result.compiled_truth, "just content");
        assert_eq!(result.timeline, "");
    }
}
