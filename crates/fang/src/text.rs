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

fn collect_text_blocks(
    el: scraper::ElementRef,
    out: &mut Vec<String>,
) {
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
