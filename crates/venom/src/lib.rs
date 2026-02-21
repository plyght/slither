pub mod fusion;

use std::collections::{HashMap, HashSet};

use slither_core::{Document, SearchMode, SearchQuery, SearchResult, SlitherResult};
use tracing::debug;

const HYBRID_CANDIDATE_POOL: usize = 200;

pub struct Ranker {
    pub index: tome::Index,
    embedder: iris::Iris,
    doc_metadata: HashMap<u64, RankerDocMeta>,
    index_path: std::path::PathBuf,
    last_reload_mtime: std::time::SystemTime,
}

#[derive(Clone)]
struct RankerDocMeta {
    url: String,
    title: String,
    snippet: String,
}

impl Ranker {
    pub fn new(index: tome::Index, embedder: iris::Iris) -> Self {
        let index_path = index.data_dir().to_path_buf();
        let last_reload_mtime = std::fs::metadata(&index_path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        Self {
            index,
            embedder,
            doc_metadata: HashMap::new(),
            index_path,
            last_reload_mtime,
        }
    }

    fn check_and_reload(&mut self) -> SlitherResult<()> {
        let current_mtime = std::fs::metadata(&self.index_path)
            .and_then(|m| m.modified())
            .unwrap_or(self.last_reload_mtime);

        if current_mtime > self.last_reload_mtime {
            self.index = tome::Index::open(&self.index_path)?;
            self.last_reload_mtime = current_mtime;
        }
        Ok(())
    }

    pub fn search(
        &mut self,
        query: &SearchQuery,
        mode: SearchMode,
    ) -> SlitherResult<Vec<SearchResult>> {
        self.check_and_reload()?;
        match mode {
            SearchMode::Text => {
                debug!(query = %query.text, limit = query.limit, "text search");
                self.index.search(&query.text, query.limit)
            }
            SearchMode::Semantic => {
                debug!(query = %query.text, limit = query.limit, "semantic search");
                let embedding = self.embedder.embed_text(&query.text)?;
                let hits = self.embedder.search(&embedding, query.limit)?;
                Ok(self.hits_to_results(hits))
            }
            SearchMode::Hybrid => {
                debug!(query = %query.text, limit = query.limit, "hybrid search");

                let pool = HYBRID_CANDIDATE_POOL.max(query.limit * 4);

                let raw_text_results = self.index.search(&query.text, pool * 2)?;

                let text_results: Vec<SearchResult> = raw_text_results
                    .into_iter()
                    .filter(|r| !fusion::is_junk_candidate(&r.url, &r.title))
                    .take(pool)
                    .collect();

                let candidate_ids: HashSet<u64> = text_results.iter().map(|r| r.doc_id).collect();

                let embedding = self.embedder.embed_text(&query.text)?;

                let query_terms: Vec<&str> = query.text.split_whitespace().collect();

                let vector_hits = if candidate_ids.is_empty() {
                    self.embedder.search(&embedding, query.limit)?
                } else if query_terms.len() <= 2 {
                    let mut hits =
                        self.embedder
                            .search_filtered(&embedding, pool, &candidate_ids)?;
                    let unconstrained = self.embedder.search(&embedding, pool / 4)?;
                    for hit in unconstrained {
                        if !candidate_ids.contains(&hit.0) {
                            hits.push(hit);
                        }
                    }
                    hits
                } else {
                    self.embedder
                        .search_filtered(&embedding, pool, &candidate_ids)?
                };
                let semantic_results = self.hits_to_results(vector_hits);

                Ok(fusion::reciprocal_rank_fusion(
                    &[text_results, semantic_results],
                    query.limit,
                    &query.text,
                ))
            }
        }
    }

    pub fn index_document(&mut self, doc: &Document) -> SlitherResult<()> {
        if self.index.is_indexed(doc.id) {
            return Ok(());
        }
        self.index.index_document(doc)?;
        let text = format!("{} {}", doc.title, doc.body);
        let embedding = self.embedder.embed_text(&text)?;
        self.embedder.store_vector(doc.id, &embedding)?;
        let snippet: String = doc.body.chars().take(200).collect();
        self.doc_metadata.insert(
            doc.id,
            RankerDocMeta {
                url: doc.url.clone(),
                title: doc.title.clone(),
                snippet,
            },
        );
        Ok(())
    }

    pub fn doc_count(&self) -> usize {
        self.index.doc_count()
    }

    pub fn flush(&mut self) -> SlitherResult<()> {
        self.index.flush()
    }

    fn hits_to_results(&self, hits: Vec<(u64, f32)>) -> Vec<SearchResult> {
        hits.into_iter()
            .filter_map(|(doc_id, score)| {
                if let Some(meta) = self.doc_metadata.get(&doc_id) {
                    Some(SearchResult {
                        doc_id,
                        url: meta.url.clone(),
                        title: meta.title.clone(),
                        snippet: meta.snippet.clone(),
                        score,
                    })
                } else if let Some((url, title, body)) = self.index.lookup_doc_meta(doc_id) {
                    let snippet: String = body.chars().take(200).collect();
                    Some(SearchResult {
                        doc_id,
                        url,
                        title,
                        snippet,
                        score,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

const RRF_K: f32 = 60.0;

pub struct Venom;

impl Venom {
    pub fn new() -> Self {
        Self
    }

    pub fn rank_rrf(
        &self,
        text_results: &[SearchResult],
        vector_results: &[(u64, f32)],
        doc_lookup: &dyn Fn(u64) -> Option<(String, String)>,
        limit: usize,
    ) -> Vec<SearchResult> {
        let mut scores: HashMap<u64, f32> = HashMap::new();

        for (rank, result) in text_results.iter().enumerate() {
            let rrf = 1.0 / (RRF_K + rank as f32 + 1.0);
            *scores.entry(result.doc_id).or_insert(0.0) += rrf;
        }

        for (rank, (doc_id, _)) in vector_results.iter().enumerate() {
            let rrf = 1.0 / (RRF_K + rank as f32 + 1.0);
            *scores.entry(*doc_id).or_insert(0.0) += rrf;
        }

        debug!(
            candidates = scores.len(),
            "RRF fusion over {} text + {} vector results",
            text_results.len(),
            vector_results.len()
        );

        self.build_results(scores, text_results, vector_results, doc_lookup, limit)
    }

    pub fn rank_weighted(
        &self,
        text_results: &[SearchResult],
        vector_results: &[(u64, f32)],
        doc_lookup: &dyn Fn(u64) -> Option<(String, String)>,
        alpha: f32,
        limit: usize,
    ) -> Vec<SearchResult> {
        let alpha = alpha.clamp(0.0, 1.0);

        let text_norm = normalize_text_scores(text_results);
        let vec_norm = normalize_vector_scores(vector_results);

        let mut scores: HashMap<u64, f32> = HashMap::new();

        for (doc_id, norm_score) in &text_norm {
            *scores.entry(*doc_id).or_insert(0.0) += alpha * norm_score;
        }

        for (doc_id, norm_score) in &vec_norm {
            *scores.entry(*doc_id).or_insert(0.0) += (1.0 - alpha) * norm_score;
        }

        debug!(
            alpha,
            candidates = scores.len(),
            "weighted fusion over {} text + {} vector results",
            text_results.len(),
            vector_results.len()
        );

        self.build_results(scores, text_results, vector_results, doc_lookup, limit)
    }

    fn build_results(
        &self,
        scores: HashMap<u64, f32>,
        text_results: &[SearchResult],
        vector_results: &[(u64, f32)],
        doc_lookup: &dyn Fn(u64) -> Option<(String, String)>,
        limit: usize,
    ) -> Vec<SearchResult> {
        let text_map: HashMap<u64, &SearchResult> =
            text_results.iter().map(|r| (r.doc_id, r)).collect();

        let vec_ids: HashMap<u64, f32> = vector_results.iter().map(|(id, s)| (*id, *s)).collect();

        let mut sorted: Vec<(u64, f32)> = scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(limit);

        let mut results = Vec::with_capacity(sorted.len());

        for (doc_id, combined_score) in sorted {
            if let Some(existing) = text_map.get(&doc_id) {
                results.push(SearchResult {
                    doc_id,
                    url: existing.url.clone(),
                    title: existing.title.clone(),
                    snippet: existing.snippet.clone(),
                    score: combined_score,
                });
            } else if vec_ids.contains_key(&doc_id) {
                if let Some((url, title)) = doc_lookup(doc_id) {
                    results.push(SearchResult {
                        doc_id,
                        url,
                        title,
                        snippet: String::new(),
                        score: combined_score,
                    });
                }
            }
        }

        results
    }
}

impl Default for Venom {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_text_scores(results: &[SearchResult]) -> Vec<(u64, f32)> {
    if results.is_empty() {
        return Vec::new();
    }

    let min = results
        .iter()
        .map(|r| r.score)
        .fold(f32::INFINITY, f32::min);
    let max = results
        .iter()
        .map(|r| r.score)
        .fold(f32::NEG_INFINITY, f32::max);
    let range = max - min;

    results
        .iter()
        .map(|r| {
            let norm = if range < f32::EPSILON {
                1.0
            } else {
                (r.score - min) / range
            };
            (r.doc_id, norm)
        })
        .collect()
}

fn normalize_vector_scores(results: &[(u64, f32)]) -> Vec<(u64, f32)> {
    if results.is_empty() {
        return Vec::new();
    }

    results
        .iter()
        .map(|(doc_id, score)| {
            let norm = (score + 1.0) / 2.0;
            (*doc_id, norm.clamp(0.0, 1.0))
        })
        .collect()
}
