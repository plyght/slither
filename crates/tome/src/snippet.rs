const SNIPPET_LEN: usize = 200;
const CONTEXT_WORDS: usize = 10;

pub fn extract_snippet(body: &str, query_tokens: &[String]) -> String {
    if body.is_empty() {
        return String::new();
    }

    let lower_body = body.to_lowercase();

    let best_pos = query_tokens
        .iter()
        .filter_map(|token| lower_body.find(token.as_str()))
        .min();

    let start = match best_pos {
        Some(pos) => {
            let pos = pos.min(body.len());
            let pos = if body.is_char_boundary(pos) {
                pos
            } else {
                body.char_indices()
                    .map(|(i, _)| i)
                    .take_while(|&i| i <= pos)
                    .last()
                    .unwrap_or(0)
            };
            let word_start = body[..pos]
                .rfind(|c: char| c.is_whitespace())
                .map(|i| {
                    let words_before = body[..i].split_whitespace().count();
                    if words_before > CONTEXT_WORDS {
                        body[..i]
                            .split_whitespace()
                            .rev()
                            .take(CONTEXT_WORDS)
                            .last()
                            .and_then(|w| body.find(w))
                            .unwrap_or(0)
                    } else {
                        0
                    }
                })
                .unwrap_or(0);
            word_start
        }
        None => 0,
    };

    let mut end = (start + SNIPPET_LEN).min(body.len());
    while end < body.len() && !body.is_char_boundary(end) {
        end += 1;
    }

    let raw = &body[start..end];

    let trimmed = if start > 0 {
        raw.trim_start_matches(|c: char| !c.is_whitespace())
    } else {
        raw
    };

    let result = if end < body.len() {
        let cut = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map(|i| &trimmed[..i])
            .unwrap_or(trimmed);
        format!("{cut}…")
    } else {
        trimmed.to_string()
    };

    result.trim().to_string()
}
