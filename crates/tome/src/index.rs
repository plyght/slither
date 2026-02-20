use std::collections::HashMap;
use std::path::Path;

use xxhash_rust::xxh3::xxh3_64;

use slither_core::SlitherError;

use crate::storage::{IndexWriter, Posting, TermDictWriter};

pub struct InMemoryIndex {
    pub postings: HashMap<u64, Vec<Posting>>,
    pub term_text: HashMap<u64, String>,
    pub doc_lengths: Vec<u64>,
    pub total_doc_len: u64,
}

impl InMemoryIndex {
    pub fn new() -> Self {
        Self {
            postings: HashMap::new(),
            term_text: HashMap::new(),
            doc_lengths: Vec::new(),
            total_doc_len: 0,
        }
    }

    pub fn add_document(&mut self, doc_id: u64, tokens: &[String]) {
        let doc_len = tokens.len() as u64;
        self.doc_lengths.push(doc_len);
        self.total_doc_len += doc_len;

        let mut term_freqs: HashMap<u64, u32> = HashMap::new();
        for token in tokens {
            let hash = xxh3_64(token.as_bytes());
            *term_freqs.entry(hash).or_insert(0) += 1;
            self.term_text.entry(hash).or_insert_with(|| token.clone());
        }

        for (hash, freq) in term_freqs {
            self.postings.entry(hash).or_default().push(Posting {
                doc_id,
                term_freq: freq,
            });
        }
    }

    pub fn avg_doc_length(&self) -> f64 {
        if self.doc_lengths.is_empty() {
            return 0.0;
        }
        self.total_doc_len as f64 / self.doc_lengths.len() as f64
    }
}

pub fn flush_index(
    mem: &InMemoryIndex,
    index_path: &Path,
    docs_path: &Path,
    terms_path: &Path,
) -> Result<(), SlitherError> {
    let mut sorted_terms: Vec<(u64, &Vec<Posting>)> =
        mem.postings.iter().map(|(h, p)| (*h, p)).collect();
    sorted_terms.sort_by_key(|(h, _)| *h);

    let mut idx_writer = IndexWriter::new(index_path.to_path_buf());
    let mut term_writer = TermDictWriter::new(terms_path.to_path_buf());

    for (hash, postings) in &sorted_terms {
        idx_writer.append_term(*hash, postings)?;
        if let Some(term) = mem.term_text.get(hash) {
            term_writer.append(*hash, term)?;
        }
    }

    idx_writer.flush()?;
    term_writer.flush()?;

    let _ = docs_path;

    Ok(())
}

pub fn hash_term(term: &str) -> u64 {
    xxh3_64(term.as_bytes())
}
