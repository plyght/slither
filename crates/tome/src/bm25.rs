const K1: f64 = 1.2;
const B: f64 = 0.75;

pub struct Bm25Params {
    pub doc_count: u64,
    pub avg_doc_length: f64,
}

pub fn idf(doc_count: u64, doc_freq: u64) -> f64 {
    let n = doc_count as f64;
    let df = doc_freq as f64;
    ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
}

pub fn term_score(tf: u32, doc_len: u64, params: &Bm25Params, doc_freq: u64) -> f64 {
    let tf = tf as f64;
    let dl = doc_len as f64;
    let idf_val = idf(params.doc_count, doc_freq);
    let tf_norm = (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * dl / params.avg_doc_length));
    idf_val * tf_norm
}
