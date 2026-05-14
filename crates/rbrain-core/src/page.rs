use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Language {
    Ja,
    ZhHans,
    ZhHant,
    Ko,
    En,
    Other(String),
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::Ja => write!(f, "ja"),
            Language::ZhHans => write!(f, "zh-hans"),
            Language::ZhHant => write!(f, "zh-hant"),
            Language::Ko => write!(f, "ko"),
            Language::En => write!(f, "en"),
            Language::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::str::FromStr for Language {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ja" => Ok(Language::Ja),
            "zh-hans" | "zh" => Ok(Language::ZhHans),
            "zh-hant" => Ok(Language::ZhHant),
            "ko" => Ok(Language::Ko),
            "en" => Ok(Language::En),
            other => Ok(Language::Other(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub slug: String,
    pub page_type: String,
    pub title: String,
    pub tags: Vec<String>,
    pub frontmatter: serde_json::Value,
    pub compiled_truth: String,
    pub timeline: String,
    pub language: Option<Language>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub content_hash: String,
}

impl Page {
    pub fn new(slug: String, page_type: String, content: String) -> Self {
        let now = Utc::now();
        Self {
            slug,
            page_type,
            title: String::new(),
            tags: Vec::new(),
            frontmatter: serde_json::Value::Object(serde_json::Map::new()),
            compiled_truth: content,
            timeline: String::new(),
            language: None,
            created_at: now,
            updated_at: now,
            content_hash: String::new(),
        }
    }
}
