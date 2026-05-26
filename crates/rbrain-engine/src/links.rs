use regex::Regex;

/// A reference to another page (link)
#[derive(Debug, Clone)]
pub struct LinkRef {
    pub target_slug: String,
    pub edge_type: String,
    pub context: Option<String>,
    pub chunk_id: Option<i64>,
}

pub fn extract_links(content: &str) -> Vec<LinkRef> {
    let mut refs = Vec::new();

    let text = strip_code_fences(content);

    // Matches [[slug]] and [[slug | chunk:N]] formats
    let wiki_re = Regex::new(r"\[\[([^|\]]+?)(?:\s*\|\s*chunk:(\d+))?\s*\]\]").unwrap();
    for cap in wiki_re.captures_iter(&text) {
        let slug = cap[1].trim().to_string();
        // Skip image/attachment links
        if is_media_file(&slug) {
            continue;
        }
        let chunk_id = cap.get(2).and_then(|m| m.as_str().parse::<i64>().ok());
        let sentence = get_sentence_containing(&text, cap.get(0).unwrap().start());
        let edge_type = infer_edge_type(&sentence);
        refs.push(LinkRef {
            target_slug: slug,
            edge_type,
            context: Some(sentence),
            chunk_id,
        });
    }

    let md_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    for cap in md_re.captures_iter(&text) {
        let url = &cap[2];
        if !url.starts_with("http://") && !url.starts_with("https://") {
            let slug = url
                .trim_start_matches("./")
                .trim_end_matches(".md")
                .trim()
                .to_string();
            if is_media_file(&slug) {
                continue;
            }
            let sentence = get_sentence_containing(&text, cap.get(0).unwrap().start());
            let edge_type = infer_edge_type(&sentence);
            refs.push(LinkRef {
                target_slug: slug,
                edge_type,
                context: Some(sentence),
                chunk_id: None,
            });
        }
    }

    refs
}

fn is_media_file(slug: &str) -> bool {
    let lower = slug.to_lowercase();
    matches!(
        lower.rsplit('.').next().unwrap_or(""),
        "jpeg" | "jpg" | "png" | "gif" | "webp" | "svg" | "pdf" | "mp4" | "mp3" | "wav"
            | "zip" | "tar" | "gz" | "xlsx" | "docx" | "pptx"
    )
}

fn infer_edge_type(sentence: &str) -> String {
    let patterns = [
        (
            r"founded|co-founded|創立|設立|창립",
            "founded",
        ),
        (
            r"invested in|backed|投资|投資|투자|出資",
            "invested",
        ),
        (
            r"advises|advisor|顾问|顧問|コンサルタント|자문",
            "advises",
        ),
        (
            r"CEO of|CTO of|works at|在.*工作|社員|職員",
            "works_at",
        ),
        (
            r"attended|参加|參加|参加した|참석",
            "attended",
        ),
    ];

    for (pattern, edge_type) in &patterns {
        if Regex::new(pattern).unwrap().is_match(sentence) {
            return edge_type.to_string();
        }
    }

    "mentions".to_string()
}

fn get_sentence_containing(text: &str, pos: usize) -> String {
    let before = &text[..pos];
    let after = &text[pos..];

    let start = before
        .rfind(|c: char| c == '。' || c == '！' || c == '？' || c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|i| i + before[i..].chars().next().map_or(1, |c| c.len_utf8()))
        .unwrap_or(0);

    let end = after
        .find(|c: char| c == '。' || c == '！' || c == '？' || c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|i| pos + i + after[i..].chars().next().map_or(1, |c| c.len_utf8()))
        .unwrap_or(text.len());

    text[start..end].trim().to_string()
}

fn strip_code_fences(text: &str) -> String {
    let re = Regex::new(r"```[\s\S]*?```").unwrap();
    re.replace_all(text, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wikilinks() {
        let content = "This is a [[test-page]] link.";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "test-page");
        assert_eq!(links[0].chunk_id, None);
    }

    #[test]
    fn test_extract_wikilinks_with_chunk() {
        let content = "See [[research/concepts/自主知识体系 | chunk:42]] for details.";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "research/concepts/自主知识体系");
        assert_eq!(links[0].chunk_id, Some(42));
    }

    #[test]
    fn test_extract_wikilinks_chunk_no_space() {
        let content = "See [[some/slug|chunk:7]] here.";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "some/slug");
        assert_eq!(links[0].chunk_id, Some(7));
    }

    #[test]
    fn test_extract_markdown_links() {
        let content = "See [more info](./another-page.md) for details.";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "another-page");
    }

    #[test]
    fn test_skip_external_links() {
        let content = "Visit [example](https://example.com) for more.";
        let links = extract_links(content);
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_infer_founded() {
        let sentence = "He founded the company in 2020.";
        assert_eq!(infer_edge_type(sentence), "founded");

        let sentence2 = "彼は会社を創立した。";
        assert_eq!(infer_edge_type(sentence2), "founded");
    }

    #[test]
    fn test_infer_invested() {
        let sentence = "She invested in the startup.";
        assert_eq!(infer_edge_type(sentence), "invested");
    }

    #[test]
    fn test_infer_works_at() {
        let sentence = "He is the CEO of Google.";
        assert_eq!(infer_edge_type(sentence), "works_at");
    }

    #[test]
    fn test_default_mentions() {
        let sentence = "This is just a mention.";
        assert_eq!(infer_edge_type(sentence), "mentions");
    }

    #[test]
    fn test_get_sentence_containing() {
        let text = "First sentence. Second sentence with [[link]]. Third sentence.";
        let pos = text.find("[[link]]").unwrap();
        let sentence = get_sentence_containing(text, pos);
        assert_eq!(sentence, "Second sentence with [[link]].");
    }

    #[test]
    fn test_strip_code_fences() {
        let text = r#"Some text
```rust
let x = 5;
```
More text"#;
        let stripped = strip_code_fences(text);
        assert!(!stripped.contains("```"));
        assert!(!stripped.contains("let x"));
    }
}
