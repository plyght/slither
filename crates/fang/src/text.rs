use scraper::{Html, Selector};

const STRIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "nav", "footer", "header", "aside", "iframe", "svg", "canvas",
    "form", "button", "input", "select", "textarea", "figure",
];

const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
    "section",
    "article",
    "main",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
    "li",
    "tr",
    "br",
    "hr",
];

pub fn extract_clean_text(document: &Html) -> String {
    let body_selector =
        Selector::parse("body").unwrap_or_else(|_| Selector::parse("html").unwrap());

    let content_selector =
        Selector::parse("main, article, [role='main'], #content, #main, .content, .main, section")
            .ok();

    let root = if let Some(sel) = &content_selector {
        let mut best = None;
        let mut best_text_len = 0usize;
        for el in document.select(sel) {
            let text: String = el.text().collect();
            let len = text.trim().len();
            if len > best_text_len {
                best_text_len = len;
                best = Some(el);
            }
        }
        best
    } else {
        None
    };

    let mut paragraphs: Vec<String> = Vec::new();

    if let Some(main_el) = root {
        collect_text_blocks(main_el, &mut paragraphs);
    } else if let Some(body) = document.select(&body_selector).next() {
        collect_text_blocks(body, &mut paragraphs);
    } else {
        collect_text_from_root(document, &mut paragraphs);
    }

    paragraphs
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn collect_text_blocks(el: scraper::ElementRef, out: &mut Vec<String>) {
    use scraper::node::Node;

    let tag = el.value().name();
    if STRIP_TAGS.contains(&tag) {
        return;
    }

    if BLOCK_TAGS.contains(&tag) {
        let text = collect_inline_text(el);
        let trimmed = collapse_whitespace(&text);
        if !trimmed.is_empty() {
            out.push(trimmed);
        }
        return;
    }

    for child in el.children() {
        match child.value() {
            Node::Element(_) => {
                if let Some(child_el) = scraper::ElementRef::wrap(child) {
                    let child_tag = child_el.value().name();
                    if STRIP_TAGS.contains(&child_tag) {
                        continue;
                    }
                    collect_text_blocks(child_el, out);
                }
            }
            Node::Text(t) => {
                let s = collapse_whitespace(t.as_ref());
                if !s.is_empty() {
                    out.push(s);
                }
            }
            _ => {}
        }
    }
}

fn collect_inline_text(el: scraper::ElementRef) -> String {
    use scraper::node::Node;

    let tag = el.value().name();
    if STRIP_TAGS.contains(&tag) {
        return String::new();
    }

    let mut buf = String::new();
    for child in el.children() {
        match child.value() {
            Node::Text(t) => buf.push_str(t.as_ref()),
            Node::Element(_) => {
                if let Some(child_el) = scraper::ElementRef::wrap(child) {
                    let child_tag = child_el.value().name();
                    if !STRIP_TAGS.contains(&child_tag) {
                        buf.push_str(&collect_inline_text(child_el));
                    }
                }
            }
            _ => {}
        }
    }
    buf
}

fn collect_text_from_root(document: &Html, out: &mut Vec<String>) {
    use scraper::node::Node;

    for node in document.tree.nodes() {
        if let Node::Text(t) = node.value() {
            let s = collapse_whitespace(t.as_ref());
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
}

/// Compute a content quality score (0.0–1.0) measuring how much real, unique
/// text a page contains vs navigation/boilerplate. Real search engines call
/// this "content-to-chrome ratio."
///
/// Returns 0.0 for pure boilerplate (empty, all short fragments, no real sentences).
/// Returns ~1.0 for well-written articles with long, unique paragraphs.
pub fn content_quality_score(body: &str) -> f32 {
    if body.is_empty() {
        return 0.0;
    }

    let words: Vec<&str> = body.split_whitespace().collect();
    let word_count = words.len();
    if word_count < 15 {
        return 0.0;
    }

    // unique word ratio — boilerplate pages repeat the same few navigation terms
    let mut seen = std::collections::HashSet::new();
    for w in &words {
        seen.insert(w.to_ascii_lowercase());
    }
    let unique_ratio = seen.len() as f32 / word_count as f32;

    // sentence quality — real content has sentences (7+ words between periods).
    // boilerplate pages are mostly short fragments: "Fork 12", "Star 0", "Actions"
    let sentences: Vec<&str> = body
        .split(['.', '!', '?'])
        .filter(|s| s.split_whitespace().count() >= 7)
        .collect();
    let sentence_ratio = if word_count > 0 {
        let sentence_words: usize = sentences.iter().map(|s| s.split_whitespace().count()).sum();
        sentence_words as f32 / word_count as f32
    } else {
        0.0
    };

    // paragraph quality — real articles have paragraphs with 20+ words
    let paragraphs: Vec<&str> = body
        .split("\n\n")
        .filter(|p| p.split_whitespace().count() >= 20)
        .collect();
    let long_para_count = paragraphs.len() as f32;
    let para_score = (long_para_count / 3.0).min(1.0); // 3+ long paragraphs = full score

    // weighted combination
    let score = unique_ratio * 0.3 + sentence_ratio * 0.4 + para_score * 0.3;
    score.clamp(0.0, 1.0)
}

pub fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_space = false;

    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                result.push(' ');
            }
            last_was_space = true;
        } else {
            result.push(ch);
            last_was_space = false;
        }
    }

    result.trim().to_string()
}
