use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::str::FromStr;

use crate::error::{BrainError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageType {
    Concept,
    Person,
    Org,
    Event,
    Note,
    Source,
    Wiki,
    Book,
    Paper,
    Figure,
    Synthesis,
    Question,
    Draft,
    Memo,
    Period,
    ResearchRun,
    ResearchQuestion,
    AnalysisPlan,
    Script,
    Dataset,
    Artifact,
    Result,
    Finding,
    Limitation,
    ValidationReport,
    ActionItem,
    ResearchMemo,
    LiteratureCorpus,
    CitationRecord,
    ArticleNote,
    Brief,
    HotspotReport,
    Recommendation,
    MethodNote,
    Unknown(String),
}

impl PageType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Concept => "concept",
            Self::Person => "person",
            Self::Org => "org",
            Self::Event => "event",
            Self::Note => "note",
            Self::Source => "source",
            Self::Wiki => "wiki",
            Self::Book => "book",
            Self::Paper => "paper",
            Self::Figure => "figure",
            Self::Synthesis => "synthesis",
            Self::Question => "question",
            Self::Draft => "draft",
            Self::Memo => "memo",
            Self::Period => "period",
            Self::ResearchRun => "research_run",
            Self::ResearchQuestion => "research_question",
            Self::AnalysisPlan => "analysis_plan",
            Self::Script => "script",
            Self::Dataset => "dataset",
            Self::Artifact => "artifact",
            Self::Result => "result",
            Self::Finding => "finding",
            Self::Limitation => "limitation",
            Self::ValidationReport => "validation_report",
            Self::ActionItem => "action_item",
            Self::ResearchMemo => "research_memo",
            Self::LiteratureCorpus => "literature_corpus",
            Self::CitationRecord => "citation_record",
            Self::ArticleNote => "article_note",
            Self::Brief => "brief",
            Self::HotspotReport => "hotspot_report",
            Self::Recommendation => "recommendation",
            Self::MethodNote => "method_note",
            Self::Unknown(raw) => raw.as_str(),
        }
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
}

impl fmt::Display for PageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PageType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase().replace('-', "_");
        let page_type = match normalized.as_str() {
            "concept" => Self::Concept,
            "person" => Self::Person,
            "org" | "organization" => Self::Org,
            "event" => Self::Event,
            "note" => Self::Note,
            "source" => Self::Source,
            "wiki" => Self::Wiki,
            "book" => Self::Book,
            "paper" => Self::Paper,
            "figure" => Self::Figure,
            "synthesis" => Self::Synthesis,
            "question" => Self::Question,
            "draft" => Self::Draft,
            "memo" => Self::Memo,
            "period" => Self::Period,
            "research_run" => Self::ResearchRun,
            "research_question" => Self::ResearchQuestion,
            "analysis_plan" => Self::AnalysisPlan,
            "script" => Self::Script,
            "dataset" => Self::Dataset,
            "artifact" => Self::Artifact,
            "result" => Self::Result,
            "finding" => Self::Finding,
            "limitation" => Self::Limitation,
            "validation_report" => Self::ValidationReport,
            "action_item" => Self::ActionItem,
            "research_memo" => Self::ResearchMemo,
            "literature_corpus" => Self::LiteratureCorpus,
            "citation_record" => Self::CitationRecord,
            "article_note" => Self::ArticleNote,
            "brief" => Self::Brief,
            "hotspot_report" => Self::HotspotReport,
            "recommendation" => Self::Recommendation,
            "method_note" => Self::MethodNote,
            _ => Self::Unknown(s.trim().to_string()),
        };
        Ok(page_type)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    Mentions,
    Cites,
    DerivedFrom,
    Updates,
    UsesDataset,
    UsesCorpus,
    UsesMethod,
    ComputedBy,
    Produces,
    Supports,
    Contradicts,
    Validates,
    Limits,
    Recommends,
    Evidence,
    Related,
    References,
    Develops,
    Unknown(String),
}

impl EdgeType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Mentions => "mentions",
            Self::Cites => "cites",
            Self::DerivedFrom => "derived_from",
            Self::Updates => "updates",
            Self::UsesDataset => "uses_dataset",
            Self::UsesCorpus => "uses_corpus",
            Self::UsesMethod => "uses_method",
            Self::ComputedBy => "computed_by",
            Self::Produces => "produces",
            Self::Supports => "supports",
            Self::Contradicts => "contradicts",
            Self::Validates => "validates",
            Self::Limits => "limits",
            Self::Recommends => "recommends",
            Self::Evidence => "evidence",
            Self::Related => "related",
            Self::References => "references",
            Self::Develops => "develops",
            Self::Unknown(raw) => raw.as_str(),
        }
    }

    pub fn validate_label(label: &str) -> Result<()> {
        if label.trim().is_empty() {
            return Err(BrainError::Conflict(
                "edge_type cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

impl fmt::Display for EdgeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EdgeType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase().replace('-', "_");
        let edge_type = match normalized.as_str() {
            "mentions" => Self::Mentions,
            "cites" => Self::Cites,
            "derived_from" => Self::DerivedFrom,
            "updates" => Self::Updates,
            "uses_dataset" => Self::UsesDataset,
            "uses_corpus" => Self::UsesCorpus,
            "uses_method" => Self::UsesMethod,
            "computed_by" => Self::ComputedBy,
            "produces" => Self::Produces,
            "supports" => Self::Supports,
            "contradicts" => Self::Contradicts,
            "validates" => Self::Validates,
            "limits" => Self::Limits,
            "recommends" => Self::Recommends,
            "evidence" => Self::Evidence,
            "related" => Self::Related,
            "references" => Self::References,
            "develops" => Self::Develops,
            _ => Self::Unknown(s.trim().to_string()),
        };
        Ok(edge_type)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineSource {
    User,
    Validator,
    Agent,
    Script,
    Llm,
}

impl fmt::Display for TimelineSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::User => "user",
            Self::Validator => "validator",
            Self::Agent => "agent",
            Self::Script => "script",
            Self::Llm => "llm",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub ts: DateTime<Utc>,
    pub source: TimelineSource,
    pub kind: String,
    pub payload: Value,
}

impl TimelineEntry {
    pub fn new(source: TimelineSource, kind: impl Into<String>, payload: Value) -> Self {
        Self {
            ts: Utc::now(),
            source,
            kind: kind.into(),
            payload,
        }
    }

    pub fn manual_note(text: impl Into<String>) -> Self {
        Self {
            ts: Utc::now(),
            source: TimelineSource::User,
            kind: "manual_note".to_string(),
            payload: serde_json::json!({ "text": text.into() }),
        }
    }

    pub fn parse_compat(raw: &str) -> Result<Vec<Self>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        if trimmed.starts_with('[') {
            let entries: Vec<Self> = serde_json::from_str(trimmed)?;
            return Ok(entries);
        }
        Ok(vec![Self {
            ts: Utc::now(),
            source: TimelineSource::User,
            kind: "legacy_timeline".to_string(),
            payload: serde_json::json!({ "text": raw }),
        }])
    }

    pub fn to_json(entries: &[Self]) -> Result<String> {
        serde_json::to_string(entries).map_err(BrainError::from)
    }

    pub fn append_compat(raw: &str, entry: Self) -> Result<String> {
        let mut entries = Self::parse_compat(raw)?;
        entries.push(entry);
        Self::to_json(&entries)
    }

    pub fn prepend_compat(raw: &str, entry: Self) -> Result<String> {
        let mut entries = Self::parse_compat(raw)?;
        entries.insert(0, entry);
        Self::to_json(&entries)
    }

    pub fn render_compat(raw: &str) -> String {
        match Self::parse_compat(raw) {
            Ok(entries) if !entries.is_empty() => entries
                .iter()
                .map(Self::render_entry)
                .collect::<Vec<_>>()
                .join("\n"),
            _ => raw.to_string(),
        }
    }

    pub fn take_lines_compat(raw: &str) -> Vec<String> {
        match Self::parse_compat(raw) {
            Ok(entries) => entries
                .iter()
                .filter(|entry| entry.kind == "take")
                .map(Self::render_entry)
                .collect(),
            Err(_) => raw
                .lines()
                .filter(|line| line.contains("[take/"))
                .map(ToString::to_string)
                .collect(),
        }
    }

    fn render_entry(entry: &Self) -> String {
        match entry.kind.as_str() {
            "dated_event" => {
                let fallback_date = entry.ts.format("%Y-%m-%d").to_string();
                let date = entry
                    .payload
                    .get("date")
                    .and_then(Value::as_str)
                    .unwrap_or(&fallback_date);
                let text = entry
                    .payload
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if let Some(source) = entry.payload.get("source").and_then(Value::as_str) {
                    format!("- {}: {} [Source: {}]", date, text, source)
                } else {
                    format!("- {}: {}", date, text)
                }
            }
            "take" => {
                let kind = entry
                    .payload
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("interpretation");
                let text = entry
                    .payload
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                format!(
                    "- [take/{}] {}: {}",
                    kind,
                    entry.ts.format("%Y-%m-%d"),
                    text
                )
            }
            "legacy_timeline" => entry
                .payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            _ => format!("- [{}:{}] {}", entry.source, entry.kind, entry.payload),
        }
    }
}

#[derive(Debug)]
pub struct PageSchema;

impl PageSchema {
    pub const CURRENT_VERSION: i64 = 1;

    pub fn validate_frontmatter(page_type: &str, frontmatter: &Value) -> Result<()> {
        let parsed = PageType::from_str(page_type).unwrap();

        let Some(obj) = frontmatter.as_object() else {
            return Err(BrainError::Conflict(format!(
                "frontmatter for page_type '{}' must be a JSON object",
                page_type
            )));
        };

        if let Some(type_value) = obj.get("type").and_then(Value::as_str) {
            let fm_type = PageType::from_str(type_value).unwrap();
            if !parsed.is_unknown() && !fm_type.is_unknown() && parsed != fm_type {
                return Err(BrainError::Conflict(format!(
                    "frontmatter type '{}' does not match page_type '{}'",
                    type_value, page_type
                )));
            }
        }

        for &field in required_fields(&parsed) {
            if !obj.contains_key(field) || is_effectively_empty(&obj[field]) {
                return Err(BrainError::Conflict(format!(
                    "frontmatter for page_type '{}' missing required field '{}'",
                    page_type, field
                )));
            }
        }

        Ok(())
    }
}

fn required_fields(page_type: &PageType) -> &'static [&'static str] {
    match page_type {
        PageType::ResearchRun => &["run_id", "status"],
        PageType::Dataset => &["source"],
        PageType::Artifact => &["artifact_kind", "path"],
        PageType::Finding => &["status"],
        PageType::ValidationReport => &["validator", "status"],
        PageType::ActionItem => &["action_kind", "status"],
        PageType::LiteratureCorpus => &["title"],
        PageType::CitationRecord => &[
            "title",
            "authors",
            "year",
            "journal",
            "abstract",
            "source",
            "record_hash",
        ],
        PageType::Brief => &["time_window", "corpus", "source_count"],
        PageType::HotspotReport => &["corpus"],
        PageType::Recommendation => &["title"],
        _ => &[],
    }
}

fn is_effectively_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_page_type_aliases() {
        assert_eq!(
            PageType::from_str("research-run").unwrap(),
            PageType::ResearchRun
        );
        assert_eq!(PageType::from_str("book").unwrap(), PageType::Book);
        assert!(PageType::from_str("custom").unwrap().is_unknown());
    }

    #[test]
    fn validates_structured_citation_record() {
        let fm = serde_json::json!({
            "type": "citation_record",
            "title": "A paper",
            "authors": ["A"],
            "year": 2026,
            "journal": "J",
            "abstract": "Abstract",
            "source": "csv",
            "record_hash": "sha256:abc"
        });
        PageSchema::validate_frontmatter("citation_record", &fm).unwrap();
    }

    #[test]
    fn rejects_missing_structured_required_field() {
        let fm = serde_json::json!({ "type": "citation_record", "title": "A paper" });
        let err = PageSchema::validate_frontmatter("citation_record", &fm)
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing required field"));
    }

    #[test]
    fn legacy_note_frontmatter_can_be_empty() {
        PageSchema::validate_frontmatter("note", &serde_json::json!({})).unwrap();
    }

    #[test]
    fn parses_legacy_timeline_as_single_entry() {
        let entries = TimelineEntry::parse_compat("2026-01-01: note").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "legacy_timeline");
    }

    #[test]
    fn appends_structured_timeline_and_renders_text() {
        let raw = TimelineEntry::append_compat(
            "",
            TimelineEntry::new(
                TimelineSource::User,
                "take",
                serde_json::json!({ "kind": "question", "text": "What follows?" }),
            ),
        )
        .unwrap();

        let rendered = TimelineEntry::render_compat(&raw);
        assert!(rendered.contains("[take/question]"));
        assert!(rendered.contains("What follows?"));

        let takes = TimelineEntry::take_lines_compat(&raw);
        assert_eq!(takes.len(), 1);
    }

    #[test]
    fn prepends_dated_event_to_legacy_timeline() {
        let raw = TimelineEntry::prepend_compat(
            "- 2026-01-01: old note",
            TimelineEntry::new(
                TimelineSource::User,
                "dated_event",
                serde_json::json!({
                    "date": "2026-06-14",
                    "text": "new event",
                    "source": "source-page"
                }),
            ),
        )
        .unwrap();

        let rendered = TimelineEntry::render_compat(&raw);
        assert!(rendered.lines().next().unwrap().contains("new event"));
        assert!(rendered.contains("old note"));
    }
}
