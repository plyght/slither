use rust_stemmers::{Algorithm, Stemmer};
use std::sync::LazyLock;

const MIN_TOKEN_LEN: usize = 2;
const MAX_TOKEN_LEN: usize = 40;

static STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
    "from", "is", "it", "its", "be", "as", "was", "are", "were", "been", "has", "have", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "shall", "can", "not",
    "no", "nor", "so", "yet", "both", "either", "neither", "each", "this", "that", "these",
    "those", "which", "who", "whom", "what", "when", "where", "why", "how", "all", "any", "few",
    "more", "most", "other", "some", "such", "than", "too", "very", "just", "also", "into", "onto",
    "upon", "about", "above", "after", "before", "between", "during", "through", "under", "over",
    "then", "if", "up", "out", "he", "she", "we", "they", "you", "me", "him", "her", "us", "them",
    "my", "your", "his", "our", "their", "its", "i",
];

static STEMMER: LazyLock<Stemmer> = LazyLock::new(|| Stemmer::create(Algorithm::English));

pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut tokens = Vec::new();

    let mut current = String::new();
    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch == '\'' {
            current.push(ch);
        } else if !current.is_empty() {
            flush_token(&mut current, &mut tokens);
        }
    }
    if !current.is_empty() {
        flush_token(&mut current, &mut tokens);
    }

    tokens
}

fn flush_token(current: &mut String, tokens: &mut Vec<String>) {
    let token = current.trim_matches('\'').to_string();
    current.clear();

    if token.len() < MIN_TOKEN_LEN || token.len() > MAX_TOKEN_LEN {
        return;
    }

    if STOP_WORDS.contains(&token.as_str()) {
        return;
    }

    let stemmed = STEMMER.stem(&token).into_owned();
    tokens.push(stemmed);
}

pub fn tokenize_query(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut tokens = Vec::new();

    let mut current = String::new();
    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch == '\'' {
            current.push(ch);
        } else if !current.is_empty() {
            flush_query_token(&mut current, &mut tokens);
        }
    }
    if !current.is_empty() {
        flush_query_token(&mut current, &mut tokens);
    }

    tokens
}

fn flush_query_token(current: &mut String, tokens: &mut Vec<String>) {
    let token = current.trim_matches('\'').to_string();
    current.clear();

    if token.len() < MIN_TOKEN_LEN || token.len() > MAX_TOKEN_LEN {
        return;
    }

    if STOP_WORDS.contains(&token.as_str()) {
        return;
    }

    let porter = STEMMER.stem(&token).into_owned();
    let legacy = legacy_stem(&token);

    tokens.push(porter.clone());
    if legacy != porter {
        tokens.push(legacy);
    }
}

pub fn tokenize_query_with_originals(text: &str) -> (Vec<String>, Vec<String>) {
    let lower = text.to_lowercase();
    let mut stemmed = Vec::new();
    let mut originals = Vec::new();

    let mut current = String::new();
    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch == '\'' {
            current.push(ch);
        } else if !current.is_empty() {
            let token = current.trim_matches('\'').to_string();
            current.clear();
            if token.len() >= MIN_TOKEN_LEN
                && token.len() <= MAX_TOKEN_LEN
                && !STOP_WORDS.contains(&token.as_str())
            {
                originals.push(token.clone());
                let porter = STEMMER.stem(&token).into_owned();
                let legacy = legacy_stem(&token);
                stemmed.push(porter.clone());
                if legacy != porter {
                    stemmed.push(legacy);
                }
            }
        }
    }
    if !current.is_empty() {
        let token = current.trim_matches('\'').to_string();
        if token.len() >= MIN_TOKEN_LEN
            && token.len() <= MAX_TOKEN_LEN
            && !STOP_WORDS.contains(&token.as_str())
        {
            originals.push(token.clone());
            let porter = STEMMER.stem(&token).into_owned();
            let legacy = legacy_stem(&token);
            stemmed.push(porter.clone());
            if legacy != porter {
                stemmed.push(legacy);
            }
        }
    }

    (stemmed, originals)
}

fn legacy_stem(word: &str) -> String {
    let len = word.len();

    if len > 4 && word.ends_with("ing") {
        let root = &word[..len - 3];
        if root.len() >= 2 {
            return root.to_string();
        }
    }
    if len > 3 && word.ends_with("ed") {
        let root = &word[..len - 2];
        if root.len() >= 2 {
            return root.to_string();
        }
    }
    if len > 3 && word.ends_with("ly") {
        let root = &word[..len - 2];
        if root.len() >= 2 {
            return root.to_string();
        }
    }
    if len > 2 && word.ends_with('s') && !word.ends_with("ss") {
        let root = &word[..len - 1];
        if root.len() >= 2 {
            return root.to_string();
        }
    }

    word.to_string()
}
