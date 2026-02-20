use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use memmap2::Mmap;
use slither_core::SlitherError;

pub const MAGIC_DOC: &[u8; 8] = b"SLITHDOC";
pub const MAGIC_IDX: &[u8; 8] = b"SLITHIDX";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct StoredDoc {
    pub url: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy)]
pub struct TermEntry {
    pub hash: u64,
    pub postings_offset: u64,
    pub postings_len: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Posting {
    pub doc_id: u64,
    pub term_freq: u32,
}

pub struct DocStoreWriter {
    path: PathBuf,
    offsets: Vec<(u64, u32)>,
    data: Vec<u8>,
}

impl DocStoreWriter {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            offsets: Vec::new(),
            data: Vec::new(),
        }
    }

    pub fn load_existing(path: PathBuf) -> Result<Self, SlitherError> {
        let raw = std::fs::read(&path)?;
        if raw.len() < 20 {
            return Ok(Self::new(path));
        }
        if &raw[0..8] != MAGIC_DOC {
            return Ok(Self::new(path));
        }
        let doc_count = u64::from_le_bytes(raw[12..20].try_into().unwrap()) as usize;
        if doc_count == 0 {
            return Ok(Self::new(path));
        }
        let entry_table_offset = 20;
        let data_section_offset = entry_table_offset + doc_count * 12;
        let mut offsets = Vec::with_capacity(doc_count);
        for i in 0..doc_count {
            let pos = entry_table_offset + i * 12;
            let offset = u64::from_le_bytes(raw[pos..pos + 8].try_into().unwrap());
            let len = u32::from_le_bytes(raw[pos + 8..pos + 12].try_into().unwrap());
            offsets.push((offset, len));
        }
        let data = raw[data_section_offset..].to_vec();
        Ok(Self { path, offsets, data })
    }

    pub fn append(&mut self, doc: &StoredDoc) -> Result<u64, SlitherError> {
        let doc_id = self.offsets.len() as u64;
        let offset = self.data.len() as u64;

        write_lp_str(&mut self.data, &doc.url);
        write_lp_str(&mut self.data, &doc.title);
        write_lp_str(&mut self.data, &doc.body);

        let len = (self.data.len() as u64 - offset) as u32;
        self.offsets.push((offset, len));

        Ok(doc_id)
    }

    pub fn flush(self) -> Result<(), SlitherError> {
        let file = File::create(&self.path)?;
        let mut w = BufWriter::new(file);

        w.write_all(MAGIC_DOC)?;
        w.write_all(&VERSION.to_le_bytes())?;
        w.write_all(&(self.offsets.len() as u64).to_le_bytes())?;

        for (offset, len) in &self.offsets {
            w.write_all(&offset.to_le_bytes())?;
            w.write_all(&len.to_le_bytes())?;
        }

        w.write_all(&self.data)?;
        w.flush()?;

        Ok(())
    }
}

pub struct DocStoreReader {
    mmap: Mmap,
    doc_count: u64,
    entry_table_offset: usize,
    data_section_offset: usize,
}

impl DocStoreReader {
    pub fn open(path: &Path) -> Result<Self, SlitherError> {
        let file = File::open(path)?;
        // SAFETY: We open the file read-only and treat the mmap as immutable bytes.
        // The file is only written during flush() before any reader is created.
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| SlitherError::Storage(format!("mmap docs: {e}")))?;

        if mmap.len() < 20 {
            return Err(SlitherError::Storage("docs.bin too small".into()));
        }

        if &mmap[0..8] != MAGIC_DOC {
            return Err(SlitherError::Storage("docs.bin bad magic".into()));
        }

        let version = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
        if version != VERSION {
            return Err(SlitherError::Storage(format!(
                "docs.bin version mismatch: {version}"
            )));
        }

        let doc_count = u64::from_le_bytes(mmap[12..20].try_into().unwrap());
        let entry_table_offset = 20usize;
        let data_section_offset = entry_table_offset + (doc_count as usize) * 12;

        Ok(Self {
            mmap,
            doc_count,
            entry_table_offset,
            data_section_offset,
        })
    }

    pub fn doc_count(&self) -> u64 {
        self.doc_count
    }

    pub fn read(&self, doc_id: u64) -> Result<StoredDoc, SlitherError> {
        if doc_id >= self.doc_count {
            return Err(SlitherError::Storage(format!(
                "doc_id {doc_id} out of range"
            )));
        }

        let entry_pos = self.entry_table_offset + (doc_id as usize) * 12;
        let offset =
            u64::from_le_bytes(self.mmap[entry_pos..entry_pos + 8].try_into().unwrap()) as usize;
        let base = self.data_section_offset + offset;
        let mut cursor = base;

        let url = read_lp_str(&self.mmap, &mut cursor)?;
        let title = read_lp_str(&self.mmap, &mut cursor)?;
        let body = read_lp_str(&self.mmap, &mut cursor)?;

        Ok(StoredDoc { url, title, body })
    }
}

pub struct IndexWriter {
    path: PathBuf,
    term_entries: Vec<TermEntry>,
    postings_data: Vec<u8>,
}

impl IndexWriter {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            term_entries: Vec::new(),
            postings_data: Vec::new(),
        }
    }

    pub fn append_term(&mut self, hash: u64, postings: &[Posting]) -> Result<(), SlitherError> {
        let offset = self.postings_data.len() as u64;
        let len = postings.len() as u32;

        for p in postings {
            self.postings_data
                .extend_from_slice(&p.doc_id.to_le_bytes());
            self.postings_data
                .extend_from_slice(&p.term_freq.to_le_bytes());
        }

        self.term_entries.push(TermEntry {
            hash,
            postings_offset: offset,
            postings_len: len,
        });

        Ok(())
    }

    pub fn flush(self) -> Result<(), SlitherError> {
        let file = File::create(&self.path)?;
        let mut w = BufWriter::new(file);

        w.write_all(MAGIC_IDX)?;
        w.write_all(&VERSION.to_le_bytes())?;
        w.write_all(&(self.term_entries.len() as u64).to_le_bytes())?;

        for entry in &self.term_entries {
            w.write_all(&entry.hash.to_le_bytes())?;
            w.write_all(&entry.postings_offset.to_le_bytes())?;
            w.write_all(&entry.postings_len.to_le_bytes())?;
        }

        w.write_all(&self.postings_data)?;
        w.flush()?;

        Ok(())
    }
}

pub struct IndexReader {
    mmap: Mmap,
    term_count: u64,
    dict_offset: usize,
    postings_section_offset: usize,
}

impl IndexReader {
    pub fn open(path: &Path) -> Result<Self, SlitherError> {
        let file = File::open(path)?;
        // SAFETY: We open the file read-only. The index is written once during flush()
        // and never mutated while any reader holds a reference.
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| SlitherError::Storage(format!("mmap index: {e}")))?;

        if mmap.len() < 20 {
            return Err(SlitherError::Storage("index.bin too small".into()));
        }

        if &mmap[0..8] != MAGIC_IDX {
            return Err(SlitherError::Storage("index.bin bad magic".into()));
        }

        let version = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
        if version != VERSION {
            return Err(SlitherError::Storage(format!(
                "index.bin version mismatch: {version}"
            )));
        }

        let term_count = u64::from_le_bytes(mmap[12..20].try_into().unwrap());
        let dict_offset = 20usize;
        let postings_section_offset = dict_offset + (term_count as usize) * 20;

        Ok(Self {
            mmap,
            term_count,
            dict_offset,
            postings_section_offset,
        })
    }

    pub fn lookup(&self, hash: u64) -> Option<Vec<Posting>> {
        let entry = self.binary_search(hash)?;
        let base = self.postings_section_offset + entry.postings_offset as usize;
        let mut postings = Vec::with_capacity(entry.postings_len as usize);

        for i in 0..entry.postings_len as usize {
            let pos = base + i * 12;
            let doc_id = u64::from_le_bytes(self.mmap[pos..pos + 8].try_into().ok()?);
            let term_freq = u32::from_le_bytes(self.mmap[pos + 8..pos + 12].try_into().ok()?);
            postings.push(Posting { doc_id, term_freq });
        }

        Some(postings)
    }

    pub fn doc_freq(&self, hash: u64) -> u64 {
        self.binary_search(hash)
            .map(|e| e.postings_len as u64)
            .unwrap_or(0)
    }

    pub fn iter_all(&self) -> Vec<(u64, Vec<Posting>)> {
        let mut result = Vec::with_capacity(self.term_count as usize);
        for i in 0..self.term_count as usize {
            let pos = self.dict_offset + i * 20;
            let hash = u64::from_le_bytes(self.mmap[pos..pos + 8].try_into().unwrap());
            let postings_offset =
                u64::from_le_bytes(self.mmap[pos + 8..pos + 16].try_into().unwrap()) as usize;
            let postings_len =
                u32::from_le_bytes(self.mmap[pos + 16..pos + 20].try_into().unwrap()) as usize;

            let base = self.postings_section_offset + postings_offset;
            let mut postings = Vec::with_capacity(postings_len);
            for j in 0..postings_len {
                let p = base + j * 12;
                let doc_id = u64::from_le_bytes(self.mmap[p..p + 8].try_into().unwrap());
                let term_freq = u32::from_le_bytes(self.mmap[p + 8..p + 12].try_into().unwrap());
                postings.push(Posting { doc_id, term_freq });
            }
            result.push((hash, postings));
        }
        result
    }

    fn binary_search(&self, hash: u64) -> Option<TermEntry> {
        let n = self.term_count as usize;
        if n == 0 {
            return None;
        }

        let mut lo = 0usize;
        let mut hi = n;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let pos = self.dict_offset + mid * 20;
            let mid_hash = u64::from_le_bytes(self.mmap[pos..pos + 8].try_into().ok()?);

            match mid_hash.cmp(&hash) {
                std::cmp::Ordering::Equal => {
                    let postings_offset =
                        u64::from_le_bytes(self.mmap[pos + 8..pos + 16].try_into().ok()?);
                    let postings_len =
                        u32::from_le_bytes(self.mmap[pos + 16..pos + 20].try_into().ok()?);
                    return Some(TermEntry {
                        hash,
                        postings_offset,
                        postings_len,
                    });
                }
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }

        None
    }
}

pub struct TermDictWriter {
    path: PathBuf,
    buf: Vec<u8>,
}

impl TermDictWriter {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            buf: Vec::new(),
        }
    }

    pub fn append(&mut self, hash: u64, term: &str) -> Result<(), SlitherError> {
        let bytes = term.as_bytes();
        self.buf.extend_from_slice(&hash.to_le_bytes());
        self.buf
            .extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    pub fn flush(self) -> Result<(), SlitherError> {
        let mut file = File::create(&self.path)?;
        file.write_all(&self.buf)?;
        Ok(())
    }
}

fn write_lp_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn read_lp_str(mmap: &[u8], cursor: &mut usize) -> Result<String, SlitherError> {
    if *cursor + 4 > mmap.len() {
        return Err(SlitherError::Storage(
            "unexpected EOF reading string len".into(),
        ));
    }
    let len = u32::from_le_bytes(mmap[*cursor..*cursor + 4].try_into().unwrap()) as usize;
    *cursor += 4;

    if *cursor + len > mmap.len() {
        return Err(SlitherError::Storage(
            "unexpected EOF reading string data".into(),
        ));
    }
    let s = std::str::from_utf8(&mmap[*cursor..*cursor + len])
        .map_err(|e| SlitherError::Storage(format!("utf8 error: {e}")))?
        .to_string();
    *cursor += len;

    Ok(s)
}
