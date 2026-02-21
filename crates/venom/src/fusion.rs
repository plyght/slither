use std::collections::HashMap;

use slither_core::SearchResult;

const RRF_K: usize = 60;

const DOMAIN_EXACT_BOOST: f64 = 0.05;
const DOMAIN_PREFIX_BOOST: f64 = 0.025;
const HOMEPAGE_BOOST: f64 = 0.02;
const SHALLOW_PATH_BOOST: f64 = 0.004;
const TITLE_EXACT_BOOST: f64 = 0.015;
const TITLE_WORD_PREFIX_BOOST: f64 = 0.01;
const MULTI_SOURCE_BOOST: f64 = 0.01;
const JUNK_URL_PENALTY: f64 = -0.03;
const DEEP_PATH_PENALTY: f64 = -0.006;
const ERROR_PAGE_PENALTY: f64 = -0.025;
const PROFILE_PAGE_PENALTY: f64 = -0.015;

pub fn reciprocal_rank_fusion(
    lists: &[Vec<SearchResult>],
    limit: usize,
    query: &str,
) -> Vec<SearchResult> {
    let mut scores: HashMap<u64, f64> = HashMap::new();
    let mut doc_map: HashMap<u64, SearchResult> = HashMap::new();
    let mut source_count: HashMap<u64, usize> = HashMap::new();

    for list in lists {
        for (rank, result) in list.iter().enumerate() {
            let score = 1.0 / (RRF_K + rank + 1) as f64;
            *scores.entry(result.doc_id).or_insert(0.0) += score;
            *source_count.entry(result.doc_id).or_insert(0) += 1;
            doc_map
                .entry(result.doc_id)
                .or_insert_with(|| result.clone());
        }
    }

    let query_lower = query.trim().to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

    for (doc_id, score) in scores.iter_mut() {
        let sources = source_count.get(doc_id).copied().unwrap_or(1);
        if sources > 1 {
            *score += MULTI_SOURCE_BOOST * (sources - 1) as f64;
        }

        if let Some(result) = doc_map.get(doc_id) {
            let boost = compute_url_boost(&result.url, &result.title, &query_lower, &query_terms);
            *score += boost;
        }
    }

    let mut fused: Vec<SearchResult> = scores
        .into_iter()
        .filter_map(|(doc_id, score)| {
            doc_map.remove(&doc_id).map(|mut r| {
                r.score = score as f32;
                r
            })
        })
        .collect();

    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    fused = dedup_by_url(fused);
    fused = dedup_by_domain(fused, &query_lower);
    fused.truncate(limit);
    fused
}

fn compute_url_boost(url: &str, title: &str, query_lower: &str, query_terms: &[&str]) -> f64 {
    let mut boost = 0.0;

    let (domain, path) = match parse_domain_path(url) {
        Some(dp) => dp,
        None => return 0.0,
    };

    let domain_lower = domain.to_lowercase();
    let domain_bare = domain_lower.strip_prefix("www.").unwrap_or(&domain_lower);
    let domain_name = domain_bare.split('.').next().unwrap_or(domain_bare);
    let path_lower = path.to_lowercase();
    let title_lower = title.to_lowercase();

    if is_junk_url(&path_lower, title) {
        boost += JUNK_URL_PENALTY;
    }

    if is_error_page(&title_lower) {
        boost += ERROR_PAGE_PENALTY;
    }

    if is_profile_page(&path_lower) {
        boost += PROFILE_PAGE_PENALTY;
    }

    let query_no_spaces: String = query_lower.chars().filter(|c| !c.is_whitespace()).collect();

    if domain_name == query_no_spaces || domain_name == query_lower {
        boost += DOMAIN_EXACT_BOOST;
    } else if query_terms.len() == 1 {
        if domain_name.starts_with(query_lower) && query_lower.len() >= 3 {
            let coverage = query_lower.len() as f64 / domain_name.len() as f64;
            boost += DOMAIN_PREFIX_BOOST * coverage;
        } else if query_lower.starts_with(domain_name) && domain_name.len() >= 3 {
            let coverage = domain_name.len() as f64 / query_lower.len() as f64;
            boost += DOMAIN_PREFIX_BOOST * coverage * 0.5;
        }
    } else {
        for term in query_terms {
            if domain_name == *term {
                boost += DOMAIN_EXACT_BOOST * 0.4;
                break;
            } else if term.len() >= 3 && domain_name.starts_with(*term) {
                boost += DOMAIN_PREFIX_BOOST * 0.3;
                break;
            }
        }
    }

    let is_homepage =
        path.is_empty() || path == "/" || path == "/index.html" || path == "/index.htm";

    if is_homepage {
        boost += HOMEPAGE_BOOST;
    } else {
        let depth = path.matches('/').count();
        if depth <= 2 {
            boost += SHALLOW_PATH_BOOST;
        } else if depth >= 5 {
            boost += DEEP_PATH_PENALTY;
        }
    }

    if query_terms.len() == 1 && title_lower.starts_with(query_lower) {
        boost += TITLE_EXACT_BOOST;
    } else if title_lower.contains(query_lower) {
        boost += TITLE_EXACT_BOOST * 0.5;
    } else {
        let title_no_spaces: String = title_lower
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect();
        if title_no_spaces.contains(&query_no_spaces) {
            boost += TITLE_EXACT_BOOST * 0.4;
        }
    }

    for term in query_terms {
        if term.len() >= 3 {
            for word in title_lower.split(|c: char| !c.is_alphanumeric()) {
                if word.starts_with(*term) && word != *term && word.len() > term.len() {
                    let coverage = term.len() as f64 / word.len() as f64;
                    boost += TITLE_WORD_PREFIX_BOOST * coverage;
                    break;
                }
            }
        }
    }

    boost
}

fn is_junk_url(path_lower: &str, title: &str) -> bool {
    static WIKI_JUNK: &[&str] = &[
        "/wiki/file:",
        "/wiki/talk:",
        "/wiki/user:",
        "/wiki/user_talk:",
        "/wiki/wikipedia:",
        "/wiki/template:",
        "/wiki/category:",
        "/wiki/special:",
        "/wiki/help:",
        "/wiki/portal:",
        "/wiki/draft:",
        "/wiki/module:",
    ];
    for prefix in WIKI_JUNK {
        if path_lower.contains(prefix) {
            return true;
        }
    }

    if title.starts_with("File:") || title.starts_with("Talk:") || title.starts_with("User:") {
        return true;
    }

    let ext_junk = [".flac", ".mp3", ".wav", ".ogg", ".pdf", ".zip"];
    for ext in &ext_junk {
        if path_lower.ends_with(ext) {
            return true;
        }
    }

    false
}

fn is_profile_page(path_lower: &str) -> bool {
    static PROFILE_PATTERNS: &[&str] = &["/@", "/users/", "/user/", "/profile/", "/people/", "/u/"];
    for pat in PROFILE_PATTERNS {
        if path_lower.starts_with(pat) || path_lower.contains(pat) {
            return true;
        }
    }
    false
}

fn is_error_page(title_lower: &str) -> bool {
    static ERROR_PATTERNS: &[&str] = &[
        "502: bad gateway",
        "502 bad gateway",
        "503 service",
        "504 gateway",
        "404 not found",
        "403 forbidden",
        "500 internal",
        "wikimedia error",
        "error 502",
        "error 503",
        "error 404",
        "page not found",
        "access denied",
    ];
    for pat in ERROR_PATTERNS {
        if title_lower.contains(pat) {
            return true;
        }
    }
    false
}

fn parse_domain_path(url: &str) -> Option<(String, String)> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;

    let (host_port, path) = match without_scheme.find('/') {
        Some(i) => (&without_scheme[..i], &without_scheme[i..]),
        None => (without_scheme, ""),
    };

    let host = match host_port.find(':') {
        Some(i) => &host_port[..i],
        None => host_port,
    };

    Some((host.to_string(), path.to_string()))
}

fn normalize_url_key(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let without_www = without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme);

    let without_query = match without_www.find('?') {
        Some(i) => &without_www[..i],
        None => without_www,
    };

    let without_fragment = match without_query.find('#') {
        Some(i) => &without_query[..i],
        None => without_query,
    };

    let trimmed = without_fragment.trim_end_matches('/');
    trimmed.to_lowercase()
}

fn dedup_by_url(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut deduped: Vec<SearchResult> = Vec::with_capacity(results.len());

    for result in results {
        let key = normalize_url_key(&result.url);
        if let Some(&idx) = seen.get(&key) {
            if result.score > deduped[idx].score {
                deduped[idx] = result;
            }
        } else {
            seen.insert(key, deduped.len());
            deduped.push(result);
        }
    }

    deduped.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    deduped
}

fn dedup_by_domain(results: Vec<SearchResult>, query_lower: &str) -> Vec<SearchResult> {
    let mut domain_counts: HashMap<String, usize> = HashMap::new();
    let mut deduped = Vec::with_capacity(results.len());

    let query_no_spaces: String = query_lower.chars().filter(|c| !c.is_whitespace()).collect();

    for result in results {
        let (root, domain_name) = match parse_domain_path(&result.url) {
            Some((d, _)) => {
                let d_lower = d.to_lowercase();
                let bare = d_lower.strip_prefix("www.").unwrap_or(&d_lower);
                let name = bare.split('.').next().unwrap_or(bare).to_string();
                let root = root_domain(&d_lower).to_string();
                (root, name)
            }
            None => (String::new(), String::new()),
        };

        let is_nav_match = !domain_name.is_empty()
            && (domain_name == query_no_spaces
                || domain_name == query_lower
                || (query_lower.len() >= 4 && domain_name.starts_with(query_lower)));

        let max_per_domain: usize = if is_nav_match { 5 } else { 3 };

        let count = domain_counts.entry(root).or_insert(0);
        if *count < max_per_domain {
            *count += 1;
            deduped.push(result);
        }
    }

    deduped
}

fn root_domain(domain: &str) -> &str {
    let bare = domain.strip_prefix("www.").unwrap_or(domain);
    let parts: Vec<&str> = bare.split('.').collect();
    if parts.len() >= 2 {
        let tld = parts[parts.len() - 1];
        let sld = parts[parts.len() - 2];
        let is_compound_tld = matches!(
            (sld, tld),
            ("co", "uk")
                | ("co", "jp")
                | ("com", "au")
                | ("co", "in")
                | ("org", "uk")
                | ("ac", "uk")
                | ("com", "br")
                | ("co", "kr")
        );
        if is_compound_tld && parts.len() >= 3 {
            let start = bare.len() - tld.len() - 1 - sld.len() - 1 - parts[parts.len() - 3].len();
            return &bare[start..];
        }
        let start = bare.len() - tld.len() - 1 - sld.len();
        return &bare[start..];
    }
    bare
}
