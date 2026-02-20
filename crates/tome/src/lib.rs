mod bm25;
mod index;
mod snippet;
mod storage;
mod tokenizer;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{debug, info};

use slither_core::{Document, IndexConfig, SearchQuery, SearchResult, SlitherError, SlitherResult};

use bm25::Bm25Params;
use index::{flush_index, hash_term, InMemoryIndex};
use snippet::extract_snippet;
use storage::{DocStoreReader, DocStoreWriter, IndexReader, StoredDoc};
use tokenizer::{tokenize, tokenize_query};

const INDEX_FILE: &str = "index.bin";
const DOCS_FILE: &str = "docs.bin";
const TERMS_FILE: &str = "terms.bin";
const META_FILE: &str = "meta.json";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct IndexMeta {
    doc_count: u64,
    total_doc_len: u64,
    doc_lengths: Vec<u64>,
    doc_ids: Vec<u64>,
}

pub struct Index {
    dir: PathBuf,
    mem: InMemoryIndex,
    doc_store_writer: DocStoreWriter,
    doc_id_to_seq: HashMap<u64, u64>,
    seq_to_doc_id: Vec<u64>,
    flushed: bool,
}

impl Index {
    pub fn open(path: &Path) -> SlitherResult<Self> {
        std::fs::create_dir_all(path)?;

        let meta_path = path.join(META_FILE);
        let docs_path = path.join(DOCS_FILE);
        let index_path = path.join(INDEX_FILE);

        let mut mem = InMemoryIndex::new();
        let mut doc_id_to_seq: HashMap<u64, u64> = HashMap::new();
        let mut seq_to_doc_id: Vec<u64> = Vec::new();
        let flushed;

        if meta_path.exists() && docs_path.exists() && index_path.exists() {
            let meta_bytes = std::fs::read(&meta_path)?;
            let meta: IndexMeta = serde_json::from_slice(&meta_bytes)?;

            mem.doc_lengths = meta.doc_lengths.clone();
            mem.total_doc_len = meta.total_doc_len;

            for (seq, &doc_id) in meta.doc_ids.iter().enumerate() {
                doc_id_to_seq.insert(doc_id, seq as u64);
                seq_to_doc_id.push(doc_id);
            }

            flushed = true;
            info!(
                doc_count = meta.doc_count,
                "opened existing tome index at {}",
                path.display()
            );
        } else {
            flushed = false;
            info!("created new tome index at {}", path.display());
        }

        let doc_store_writer = DocStoreWriter::new(docs_path);

        Ok(Self {
            dir: path.to_path_buf(),
            mem,
            doc_store_writer,
            doc_id_to_seq,
            seq_to_doc_id,
            flushed,
        })
    }

    pub fn index_document(&mut self, doc: &Document) -> SlitherResult<()> {
        if self.doc_id_to_seq.contains_key(&doc.id) {
            debug!(doc_id = doc.id, "skipping already-indexed document");
            return Ok(());
        }

        let title_tokens = tokenize(&doc.title);
        let title_boosted: Vec<String> = title_tokens
            .iter()
            .cloned()
            .cycle()
            .take(title_tokens.len() * 3)
            .collect();

        let body_tokens = tokenize(&doc.body);

        let mut all_tokens = title_boosted;
        all_tokens.extend(body_tokens);

        let seq = self.seq_to_doc_id.len() as u64;
        self.mem.add_document(seq, &all_tokens);

        let body_preview: String = doc.body.chars().take(300).collect();
        let stored = StoredDoc {
            url: doc.url.clone(),
            title: doc.title.clone(),
            body: body_preview,
        };
        self.doc_store_writer.append(&stored)?;

        self.doc_id_to_seq.insert(doc.id, seq);
        self.seq_to_doc_id.push(doc.id);
        self.flushed = false;

        debug!(doc_id = doc.id, seq, "indexed document");
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> SlitherResult<Vec<SearchResult>> {
        let query_tokens = tokenize_query(query);
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }

        let doc_count = self.seq_to_doc_id.len();
        if doc_count == 0 {
            return Ok(Vec::new());
        }

        let avg_doc_len = self.mem.avg_doc_length();

        if self.flushed {
            self.search_from_disk(&query_tokens, limit, avg_doc_len)
        } else {
            self.search_in_memory(&query_tokens, limit, doc_count, avg_doc_len)
        }
    }

    fn search_in_memory(
        &self,
        query_tokens: &[String],
        limit: usize,
        doc_count: usize,
        avg_doc_len: f64,
    ) -> SlitherResult<Vec<SearchResult>> {
        let params = Bm25Params {
            doc_count: doc_count as u64,
            avg_doc_length: avg_doc_len,
        };

        let mut scores: HashMap<u64, f64> = HashMap::new();

        for token in query_tokens {
            let hash = hash_term(token);
            let postings = match self.mem.postings.get(&hash) {
                Some(p) => p,
                None => continue,
            };

            let doc_freq = postings.len() as u64;

            for posting in postings {
                let doc_len = self
                    .mem
                    .doc_lengths
                    .get(posting.doc_id as usize)
                    .copied()
                    .unwrap_or(1);
                let s = bm25::term_score(posting.term_freq, doc_len, &params, doc_freq);
                *scores.entry(posting.doc_id).or_insert(0.0) += s;
            }
        }

        let mut ranked: Vec<(u64, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(limit);

        let mut results = Vec::with_capacity(ranked.len());
        for (seq, score) in ranked {
            let doc_id = match self.seq_to_doc_id.get(seq as usize) {
                Some(&id) => id,
                None => continue,
            };

            let stored = self.read_stored_doc(seq)?;
            let snippet = extract_snippet(&stored.body, query_tokens);

            results.push(SearchResult {
                doc_id,
                url: stored.url,
                title: stored.title,
                snippet,
                score: score as f32,
            });
        }

        Ok(results)
    }

    fn search_from_disk(
        &self,
        query_tokens: &[String],
        limit: usize,
        avg_doc_len: f64,
    ) -> SlitherResult<Vec<SearchResult>> {
        let docs_path = self.dir.join(DOCS_FILE);
        let index_path = self.dir.join(INDEX_FILE);

        let doc_store = DocStoreReader::open(&docs_path)?;
        let index_reader = IndexReader::open(&index_path)?;

        let doc_count = doc_store.doc_count();
        let params = Bm25Params {
            doc_count,
            avg_doc_length: avg_doc_len,
        };

        let mut scores: HashMap<u64, f64> = HashMap::new();

        for token in query_tokens {
            let hash = hash_term(token);
            let doc_freq = index_reader.doc_freq(hash);
            if doc_freq == 0 {
                continue;
            }

            if let Some(postings) = index_reader.lookup(hash) {
                for posting in postings {
                    let doc_len = self
                        .mem
                        .doc_lengths
                        .get(posting.doc_id as usize)
                        .copied()
                        .unwrap_or(1);
                    let s = bm25::term_score(posting.term_freq, doc_len, &params, doc_freq);
                    *scores.entry(posting.doc_id).or_insert(0.0) += s;
                }
            }
        }

        let mut ranked: Vec<(u64, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(limit);

        let mut results = Vec::with_capacity(ranked.len());
        for (seq, score) in ranked {
            let doc_id = match self.seq_to_doc_id.get(seq as usize) {
                Some(&id) => id,
                None => continue,
            };

            let stored = doc_store.read(seq)?;
            let snippet = extract_snippet(&stored.body, query_tokens);

            results.push(SearchResult {
                doc_id,
                url: stored.url,
                title: stored.title,
                snippet,
                score: score as f32,
            });
        }

        Ok(results)
    }

    fn read_stored_doc(&self, seq: u64) -> SlitherResult<StoredDoc> {
        let docs_path = self.dir.join(DOCS_FILE);
        if docs_path.exists() {
            let reader = DocStoreReader::open(&docs_path)?;
            if seq < reader.doc_count() {
                return reader.read(seq);
            }
        }
        Err(SlitherError::Storage(format!(
            "doc seq {seq} not found in store"
        )))
    }

    pub fn doc_count(&self) -> usize {
        self.seq_to_doc_id.len()
    }

    pub fn lookup_doc_meta(&self, doc_id: u64) -> Option<(String, String, String)> {
        let seq = self.doc_id_to_seq.get(&doc_id)?;
        let stored = self.read_stored_doc(*seq).ok()?;
        Some((stored.url, stored.title, stored.body))
    }

    pub fn flush(&mut self) -> SlitherResult<()> {
        if self.seq_to_doc_id.is_empty() {
            return Ok(());
        }

        let index_path = self.dir.join(INDEX_FILE);
        let docs_path = self.dir.join(DOCS_FILE);
        let terms_path = self.dir.join(TERMS_FILE);
        let meta_path = self.dir.join(META_FILE);

        let writer = std::mem::replace(
            &mut self.doc_store_writer,
            DocStoreWriter::new(docs_path.clone()),
        );
        writer.flush()?;

        flush_index(&self.mem, &index_path, &docs_path, &terms_path)?;

        let meta = IndexMeta {
            doc_count: self.seq_to_doc_id.len() as u64,
            total_doc_len: self.mem.total_doc_len,
            doc_lengths: self.mem.doc_lengths.clone(),
            doc_ids: self.seq_to_doc_id.clone(),
        };
        let meta_bytes = serde_json::to_vec(&meta)?;
        std::fs::write(&meta_path, &meta_bytes)?;

        self.flushed = true;
        info!(
            doc_count = self.seq_to_doc_id.len(),
            "flushed tome index to disk"
        );
        Ok(())
    }
}

pub struct Tome {
    inner: Index,
}

impl Tome {
    pub fn new(config: IndexConfig) -> Result<Self, SlitherError> {
        let inner = Index::open(std::path::Path::new(&config.data_dir))?;
        Ok(Self { inner })
    }

    pub fn index_document(&mut self, doc: &Document) -> Result<u64, SlitherError> {
        let seq = self.inner.doc_count() as u64;
        self.inner.index_document(doc)?;
        Ok(seq)
    }

    pub fn flush(&mut self) -> Result<(), SlitherError> {
        self.inner.flush()
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SlitherError> {
        self.inner.search(&query.text, query.limit)
    }

    pub fn doc_count(&self) -> u64 {
        self.inner.doc_count() as u64
    }
}
