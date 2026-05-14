// Chunker example - reads a markdown file and outputs chunks as JSON
// Usage: cargo run --example chunker -- tests/fixtures/essay-ja.md [language]

use rbrain_core::page::Language;
use rbrain_core::markdown::MarkdownParser;
use rbrain_search::chunker::Chunker;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: cargo run --example chunker -- <file> [language]");
        eprintln!("  language: ja, zh-hans, zh-hant, ko, en (default: ja)");
        std::process::exit(1);
    }

    let file_path = &args[1];
    let lang_str = args.get(2).map(|s| s.as_str()).unwrap_or("ja");

    let content = std::fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("Failed to read file '{}': {}", file_path, e);
        std::process::exit(1);
    });

    let language: Language = lang_str.parse().unwrap_or_else(|_| {
        eprintln!("Invalid language '{}'. Using 'ja' as default.", lang_str);
        Language::Ja
    });

    // Parse markdown to extract body content (strip frontmatter)
    let parsed = MarkdownParser::parse(&content);

    let chunker = Chunker::new(&language);

    // Chunk compiled truth and timeline separately
    let mut all_chunks = Vec::new();
    if !parsed.compiled_truth.trim().is_empty() {
        all_chunks.extend(chunker.chunk(&parsed.compiled_truth, file_path, &language));
    }
    if !parsed.timeline.trim().is_empty() {
        let tl_chunks = chunker.chunk(&parsed.timeline, file_path, &language);
        all_chunks.extend(tl_chunks);
    }

    println!("{}", serde_json::to_string_pretty(&all_chunks).unwrap());

    eprintln!("\nChunked {} into {} chunks", file_path, all_chunks.len());
    eprintln!("Target chars: {}, Overlap chars: {}", chunker.target_chars, chunker.overlap_chars);
}
