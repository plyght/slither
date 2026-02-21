// vectors.bin layout:
//   [0..8] b"SLITHVEC"  [8..12] u32 version  [12..20] u64 count  [20..24] u32 dims
//   [24..] packed f32 data (count × dims), row-major LE
// vecmap.bin layout:
//   [0..] u64 LE doc_ids, one per vector, same order

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use memmap2::Mmap;
use slither_core::{EmbedderConfig, SlitherError};
use std::collections::{BinaryHeap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use tracing::debug;

const MAGIC: &[u8; 8] = b"SLITHVEC";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 24;

pub(crate) struct VectorStorage {
    vectors_path: PathBuf,
    vecmap_path: PathBuf,
    dimensions: usize,
    vector_count: u64,
    stored_ids: HashSet<u64>,
}

#[derive(PartialEq)]
struct MinScoreEntry(f32, u64);

impl Eq for MinScoreEntry {}

impl PartialOrd for MinScoreEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MinScoreEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .0
            .partial_cmp(&self.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl VectorStorage {
    pub fn open(config: &EmbedderConfig) -> Result<Self, SlitherError> {
        let data_dir = std::path::Path::new(&config.data_dir);

        std::fs::create_dir_all(data_dir)?;

        let vectors_path = data_dir.join("vectors.bin");
        let vecmap_path = data_dir.join("vecmap.bin");
        let dimensions = config.dimensions;

        let (vector_count, stored_ids) = if vectors_path.exists() {
            let count = Self::read_header(&vectors_path, dimensions)?;
            let ids = Self::load_stored_ids(&vecmap_path, count)?;
            (count, ids)
        } else {
            Self::init_vectors_file(&vectors_path, dimensions as u32)?;
            File::create(&vecmap_path)?;
            (0, HashSet::new())
        };

        debug!(
            "VectorStorage: {} vectors, {} unique IDs, dim={}, path={:?}",
            vector_count,
            stored_ids.len(),
            dimensions,
            vectors_path
        );

        Ok(Self {
            vectors_path,
            vecmap_path,
            dimensions,
            vector_count,
            stored_ids,
        })
    }

    fn load_stored_ids(
        vecmap_path: &std::path::Path,
        expected_count: u64,
    ) -> Result<HashSet<u64>, SlitherError> {
        let mut ids = HashSet::with_capacity(expected_count as usize);
        if !vecmap_path.exists() {
            File::create(vecmap_path)?;
            return Ok(ids);
        }
        let data = std::fs::read(vecmap_path)?;
        let n = data.len() / 8;
        for i in 0..n {
            let offset = i * 8;
            if offset + 8 <= data.len() {
                let id = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                ids.insert(id);
            }
        }
        Ok(ids)
    }

    fn init_vectors_file(path: &std::path::Path, dims: u32) -> Result<(), SlitherError> {
        let mut f = File::create(path)?;
        f.write_all(MAGIC)?;
        f.write_u32::<LittleEndian>(VERSION)
            .map_err(|e| SlitherError::Storage(e.to_string()))?;
        f.write_u64::<LittleEndian>(0)
            .map_err(|e| SlitherError::Storage(e.to_string()))?;
        f.write_u32::<LittleEndian>(dims)
            .map_err(|e| SlitherError::Storage(e.to_string()))?;
        f.flush()?;
        Ok(())
    }

    fn read_header(path: &std::path::Path, expected_dims: usize) -> Result<u64, SlitherError> {
        let mut f = File::open(path)?;
        let mut magic = [0u8; 8];
        f.read_exact(&mut magic)
            .map_err(|e| SlitherError::Storage(e.to_string()))?;
        if &magic != MAGIC {
            return Err(SlitherError::Storage(
                "invalid vectors.bin magic bytes".into(),
            ));
        }
        let version = f
            .read_u32::<LittleEndian>()
            .map_err(|e| SlitherError::Storage(e.to_string()))?;
        if version != VERSION {
            return Err(SlitherError::Storage(format!(
                "unsupported vectors.bin version: {version}"
            )));
        }
        let count = f
            .read_u64::<LittleEndian>()
            .map_err(|e| SlitherError::Storage(e.to_string()))?;
        let stored_dims = f
            .read_u32::<LittleEndian>()
            .map_err(|e| SlitherError::Storage(e.to_string()))? as usize;
        if stored_dims != expected_dims {
            return Err(SlitherError::Storage(format!(
                "dimension mismatch: file has {stored_dims}, config expects {expected_dims}"
            )));
        }
        Ok(count)
    }

    pub fn store(&mut self, doc_id: u64, vector: &[f32]) -> Result<(), SlitherError> {
        if self.stored_ids.contains(&doc_id) {
            return Ok(());
        }

        if vector.len() != self.dimensions {
            return Err(SlitherError::Storage(format!(
                "vector length {} does not match configured dimensions {}",
                vector.len(),
                self.dimensions
            )));
        }

        {
            let mut vf = OpenOptions::new()
                .write(true)
                .open(&self.vectors_path)
                .map_err(|e| SlitherError::Storage(e.to_string()))?;

            let data_offset = HEADER_SIZE as u64 + self.vector_count * self.dimensions as u64 * 4;
            vf.seek(SeekFrom::Start(data_offset))
                .map_err(|e| SlitherError::Storage(e.to_string()))?;

            for &v in vector {
                vf.write_f32::<LittleEndian>(v)
                    .map_err(|e| SlitherError::Storage(e.to_string()))?;
            }

            vf.seek(SeekFrom::Start(12))
                .map_err(|e| SlitherError::Storage(e.to_string()))?;
            vf.write_u64::<LittleEndian>(self.vector_count + 1)
                .map_err(|e| SlitherError::Storage(e.to_string()))?;
            vf.flush()
                .map_err(|e| SlitherError::Storage(e.to_string()))?;
        }

        {
            let mut mf = OpenOptions::new()
                .append(true)
                .open(&self.vecmap_path)
                .map_err(|e| SlitherError::Storage(e.to_string()))?;
            mf.write_u64::<LittleEndian>(doc_id)
                .map_err(|e| SlitherError::Storage(e.to_string()))?;
            mf.flush()
                .map_err(|e| SlitherError::Storage(e.to_string()))?;
        }

        self.vector_count += 1;
        self.stored_ids.insert(doc_id);
        debug!(
            "stored vector idx={} doc_id={}",
            self.vector_count - 1,
            doc_id
        );
        Ok(())
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(u64, f32)>, SlitherError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        // Read fresh count from file header to support live-updating
        let count = Self::read_header(&self.vectors_path, self.dimensions)?;
        if count == 0 {
            return Ok(Vec::new());
        }
        self.scan_vectors(query, limit, count as usize, None)
    }

    pub fn search_filtered(
        &self,
        query: &[f32],
        limit: usize,
        allowed_ids: &HashSet<u64>,
    ) -> Result<Vec<(u64, f32)>, SlitherError> {
        if limit == 0 || allowed_ids.is_empty() {
            return Ok(Vec::new());
        }
        // Read fresh count from file header to support live-updating
        let count = Self::read_header(&self.vectors_path, self.dimensions)?;
        if count == 0 {
            return Ok(Vec::new());
        }
        self.scan_vectors(query, limit, count as usize, Some(allowed_ids))
    }

    fn scan_vectors(
        &self,
        query: &[f32],
        limit: usize,
        count: usize,
        filter: Option<&HashSet<u64>>,
    ) -> Result<Vec<(u64, f32)>, SlitherError> {
        let dim = self.dimensions;
        let vector_bytes = dim * 4;
        let expected_data_len = HEADER_SIZE + count * vector_bytes;

        let vf =
            File::open(&self.vectors_path).map_err(|e| SlitherError::Storage(e.to_string()))?;
        let mmap = unsafe { Mmap::map(&vf) }.map_err(|e| SlitherError::Storage(e.to_string()))?;

        let mf = File::open(&self.vecmap_path).map_err(|e| SlitherError::Storage(e.to_string()))?;
        let mmap_ids =
            unsafe { Mmap::map(&mf) }.map_err(|e| SlitherError::Storage(e.to_string()))?;

        if mmap.len() < expected_data_len {
            return Err(SlitherError::Storage(format!(
                "vectors.bin too small: {} < {}",
                mmap.len(),
                expected_data_len
            )));
        }

        let data_region = &mmap[HEADER_SIZE..HEADER_SIZE + count * vector_bytes];
        let ids_region = &mmap_ids[..];

        let mut heap: BinaryHeap<MinScoreEntry> = BinaryHeap::with_capacity(limit + 1);
        let mut threshold = f32::NEG_INFINITY;

        for i in 0..count {
            let id_offset = i * 8;
            if id_offset + 8 > ids_region.len() {
                break;
            }
            let doc_id =
                u64::from_le_bytes(ids_region[id_offset..id_offset + 8].try_into().unwrap());

            if let Some(allowed) = filter {
                if !allowed.contains(&doc_id) {
                    continue;
                }
            }

            let vec_offset = i * vector_bytes;
            let vec_bytes = &data_region[vec_offset..vec_offset + vector_bytes];

            // x86_64 is little-endian and mmap is page-aligned, so the base is
            // aligned.  Each row starts at a multiple of dim*4 bytes from the
            // page-aligned base, so the pointer is always 4-byte aligned — safe
            // to reinterpret as &[f32].
            let vec_f32: &[f32] =
                unsafe { std::slice::from_raw_parts(vec_bytes.as_ptr() as *const f32, dim) };

            let score = crate::search::dot_product(query, vec_f32);

            if score > threshold || heap.len() < limit {
                heap.push(MinScoreEntry(score, doc_id));
                if heap.len() > limit {
                    heap.pop();
                }
                if heap.len() == limit {
                    threshold = heap.peek().map(|e| e.0).unwrap_or(f32::NEG_INFINITY);
                }
            }
        }

        let mut results: Vec<(u64, f32)> = heap
            .into_iter()
            .map(|MinScoreEntry(score, id)| (id, score))
            .collect();
        results.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    pub fn vector_count(&self) -> u64 {
        self.vector_count
    }
}
