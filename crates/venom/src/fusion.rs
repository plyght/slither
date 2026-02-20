use std::collections::HashMap;

use slither_core::SearchResult;

const RRF_K: usize = 60;

const DOMAIN_MATCH_BOOST: f64 = 0.012;
const HOMEPAGE_BOOST: f64 = 0.006;
const TITLE_EXACT_BOOST: f64 = 0.008;

pub fn reciprocal_rank_fusion(
    lists: &[Vec<SearchResult>],
    limit: usize,
    query: &str,
) -> Vec<SearchResult> {
    let mut scores: HashMap<u64, f64> = HashMap::new();
    let mut doc_map: HashMap<u64, SearchResult> = HashMap::new();

    for list in lists {
        for (rank, result) in list.iter().enumerate() {
            let score = 1.0 / (RRF_K + rank + 1) as f64;
            *scores.entry(result.doc_id).or_insert(0.0) += score;
            doc_map
                .entry(result.doc_id)
                .or_insert_with(|| result.clone());
        }
    }

    let query_lower = query.trim().to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

    for (doc_id, score) in scores.iter_mut() {
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

    fused = dedup_by_domain(fused);
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

    if query_terms.len() == 1 {
        if domain_name == query_lower {
            boost += DOMAIN_MATCH_BOOST;
        } else if domain_bare.starts_with(&format!("{}.", query_lower)) {
            boost += DOMAIN_MATCH_BOOST * 0.5;
        }
    } else {
        for term in query_terms {
            if domain_name == *term {
                boost += DOMAIN_MATCH_BOOST * 0.3;
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
            boost += HOMEPAGE_BOOST * 0.3;
        }
    }

    let title_lower = title.to_lowercase();
    if query_terms.len() == 1 && title_lower.starts_with(query_lower) {
        boost += TITLE_EXACT_BOOST;
    } else if title_lower.contains(query_lower) {
        boost += TITLE_EXACT_BOOST * 0.5;
    }

    boost
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

fn dedup_by_domain(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut domain_counts: HashMap<String, usize> = HashMap::new();
    let mut deduped = Vec::with_capacity(results.len());

    for result in results {
        let domain = match parse_domain_path(&result.url) {
            Some((d, _)) => {
                let d_lower = d.to_lowercase();
                root_domain(&d_lower).to_string()
            }
            None => String::new(),
        };

        let count = domain_counts.entry(domain).or_insert(0);
        if *count < 3 {
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
