const SNIPPET_LEN: usize = 220;
const WINDOW_STEP: usize = 40;

pub fn extract_snippet(body: &str, query_tokens: &[String]) -> String {
    if body.is_empty() {
        return String::new();
    }

    let lower_body = body.to_lowercase();

    let char_indices: Vec<(usize, char)> = body.char_indices().collect();
    if char_indices.is_empty() {
        return String::new();
    }

    let mut best_start = 0usize;
    let mut best_score: usize = 0;

    let mut offset = 0usize;
    while offset < body.len() {
        let end = find_char_boundary(body, offset + SNIPPET_LEN);
        let window = &lower_body[offset..end];

        let mut score = 0usize;
        for token in query_tokens {
            if window.contains(token.as_str()) {
                score += 1;
                if window.matches(token.as_str()).count() > 1 {
                    score += 1;
                }
            }
        }

        if score > best_score {
            best_score = score;
            best_start = offset;
        }

        if score == query_tokens.len() {
            break;
        }

        offset += WINDOW_STEP;
        if offset >= body.len() {
            break;
        }
        offset = find_char_boundary(body, offset);
    }

    let snap_start = snap_to_word_boundary_left(body, best_start);
    let raw_end = find_char_boundary(body, snap_start + SNIPPET_LEN);
    let snap_end = snap_to_word_boundary_right(body, raw_end);

    let mut snippet = body[snap_start..snap_end].to_string();

    if snap_start > 0 {
        snippet = format!("…{}", snippet.trim_start());
    }
    if snap_end < body.len() {
        snippet = format!("{}…", snippet.trim_end());
    }

    highlight_terms(&snippet, query_tokens)
}

fn highlight_terms(text: &str, query_tokens: &[String]) -> String {
    if query_tokens.is_empty() {
        return text.to_string();
    }

    let lower = text.to_lowercase();
    let mut marks: Vec<(usize, usize)> = Vec::new();

    for token in query_tokens {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(token.as_str()) {
            let abs_start = search_from + pos;
            let abs_end = abs_start + token.len();
            marks.push((abs_start, abs_end));
            search_from = abs_end;
        }
    }

    if marks.is_empty() {
        return text.to_string();
    }

    marks.sort_by_key(|m| m.0);
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in marks {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }

    let mut result = String::with_capacity(text.len() + merged.len() * 13);
    let mut cursor = 0;
    for (s, e) in &merged {
        let s = find_char_boundary(text, *s);
        let e = find_char_boundary(text, *e);
        if s > cursor {
            result.push_str(&text[cursor..s]);
        }
        result.push_str("<mark>");
        result.push_str(&text[s..e]);
        result.push_str("</mark>");
        cursor = e;
    }
    if cursor < text.len() {
        result.push_str(&text[cursor..]);
    }
    result
}

fn find_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx.min(s.len())
}

fn snap_to_word_boundary_left(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let idx = find_char_boundary(s, idx);
    s[..idx]
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0)
}

fn snap_to_word_boundary_right(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let idx = find_char_boundary(s, idx);
    s[idx..]
        .find(|c: char| c.is_whitespace())
        .map(|i| idx + i)
        .unwrap_or(s.len())
}
