use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ferrous_opencc::OpenCC;
use ferrous_opencc::config::BuiltinConfig;
use std::sync::OnceLock;

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

impl Language {
    pub fn detect(text: &str) -> Self {
        if text.trim().is_empty() {
            return Language::En;
        }

        let mut hangul_count = 0;
        let mut kana_count = 0;
        let mut han_count = 0;
        let mut latin_count = 0;

        for c in text.chars() {
            if c.is_whitespace() || c.is_ascii_punctuation() {
                continue;
            }
            if matches!(c, '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}') {
                hangul_count += 1;
            } else if matches!(c, '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' | '\u{FF65}'..='\u{FF9F}') {
                kana_count += 1;
            } else if matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}') {
                han_count += 1;
            } else if c.is_ascii_alphabetic() {
                latin_count += 1;
            }
        }

        if hangul_count > 0 {
            return Language::Ko;
        }

        if kana_count > 0 {
            return Language::Ja;
        }

        if han_count > 0 {
            let is_predominantly_han = han_count * 2 >= latin_count || han_count >= 3;
            let detected = whatlang::detect(text).map(|i| i.lang());
            
            match detected {
                Some(whatlang::Lang::Cmn) => {
                    return if Self::is_traditional(text) {
                        Language::ZhHant
                    } else {
                        Language::ZhHans
                    };
                }
                _ => {
                    if is_predominantly_han {
                        return if Self::is_traditional(text) {
                            Language::ZhHant
                        } else {
                            Language::ZhHans
                        };
                    }
                }
            }
        }

        match whatlang::detect(text).map(|i| i.lang()) {
            Some(whatlang::Lang::Jpn) => Language::Ja,
            Some(whatlang::Lang::Kor) => Language::Ko,
            Some(whatlang::Lang::Cmn) => {
                if Self::is_traditional(text) {
                    Language::ZhHant
                } else {
                    Language::ZhHans
                }
            }
            Some(whatlang::Lang::Eng) => Language::En,
            Some(other) => Language::Other(other.code().to_string()),
            None => Language::En,
        }
    }

    fn is_traditional(text: &str) -> bool {
        static OPENCC: OnceLock<Option<OpenCC>> = OnceLock::new();
        let oc_opt = OPENCC.get_or_init(|| {
            OpenCC::from_config(BuiltinConfig::T2s).ok()
        });
        if let Some(oc) = oc_opt {
            oc.convert(text) != text
        } else {
            text.chars().any(|c| matches!(c, '國' | '學' | '會' | '體' | '電' | '動' | '門' | '間' | '開' | '關' | '時' | '從' | '東' | '業' | '長' | '風' | '發' | '雲' | '馬' | '魚' | '書' | '這' | '對' | '華' | '進' | '實' | '說' | '過' | '萬' | '廣' | '與' | '無' | '樂' | '劇' | '辦' | '標' | '選' | '興' | '鳳' | '龍' | '龜' | '聽' | '觀' | '響' | '讀' | '讓' | '樣' | '總' | '歸' | '錢' | '點' | '戰' | '適' | '頭' | '禮' | '雙' | '邊' | '蘆' | '護'))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detect() {
        // English
        assert_eq!(Language::detect("This is a simple English sentence."), Language::En);

        // Simplified Chinese
        assert_eq!(Language::detect("这是一篇关于人工智能的测试文档。"), Language::ZhHans);
        assert_eq!(Language::detect("支持向量机（SVM）是一种监督学习方法。"), Language::ZhHans);
        assert_eq!(Language::detect("学习"), Language::ZhHans);

        // Traditional Chinese
        assert_eq!(Language::detect("這是一篇關於人工智慧的測試文檔。"), Language::ZhHant);
        assert_eq!(Language::detect("學習"), Language::ZhHant);

        // Japanese
        assert_eq!(Language::detect("これは人工知能に関するテスト文書です。"), Language::Ja);
        assert_eq!(Language::detect("日本語のテスト"), Language::Ja);

        // Korean
        assert_eq!(Language::detect("이것은 인공지능에 관한 테스트 문서입니다."), Language::Ko);
        assert_eq!(Language::detect("한국어 테스트"), Language::Ko);

        // Short queries / Mixed
        assert_eq!(Language::detect("因材施教"), Language::ZhHans);
    }
}
