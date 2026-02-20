use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use memmap2::Mmap;
use slither_core::{SlitherError, SlitherResult};
use std::fs::File;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"IRIS";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 20;

pub struct VecStore {
    path: PathBuf,
    dimensions: usize,
    records: Vec<(u64, Vec<f32>)>,
    dirty: bool,
}

impl VecStore {
    pub fn open(path: impl Into<PathBuf>, dimensions: usize) -> SlitherResult<Self> {
        let path = path.into();

        let records = if path.exists() {
            load_from_file(&path, dimensions)?
        } else {
            Vec::new()
        };

        Ok(Self {
            path,
            dimensions,
            records,
            dirty: false,
        })
    }

    pub fn insert(&mut self, doc_id: u64, vector: &[f32]) {
        self.records.push((doc_id, vector.to_vec()));
        self.dirty = true;
    }

    pub fn get(&self, index: usize) -> Option<(u64, &[f32])> {
        self.records.get(index).map(|(id, v)| (*id, v.as_slice()))
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn all_vectors(&self) -> impl Iterator<Item = (u64, &[f32])> {
        self.records.iter().map(|(id, v)| (*id, v.as_slice()))
    }

    pub fn flush(&mut self) -> SlitherResult<()> {
        if !self.dirty {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        flush_to_file(&self.path, self.dimensions, &self.records)?;
        self.dirty = false;

        Ok(())
    }
}

fn load_from_file(path: &Path, dimensions: usize) -> SlitherResult<Vec<(u64, Vec<f32>)>> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    if mmap.len() < HEADER_SIZE {
        return Err(SlitherError::Storage(
            "vector store file is too small to contain a valid header".to_string(),
        ));
    }

    let mut cursor = Cursor::new(mmap.as_ref());

    let mut magic = [0u8; 4];
    cursor
        .read_exact(&mut magic)
        .map_err(|e| SlitherError::Storage(e.to_string()))?;

    if &magic != MAGIC {
        return Err(SlitherError::Storage(
            "invalid IRIS magic bytes in vector store file".to_string(),
        ));
    }

    let version = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| SlitherError::Storage(e.to_string()))?;
    if version != VERSION {
        return Err(SlitherError::Storage(format!(
            "unsupported IRIS vector store version: {}",
            version
        )));
    }

    let stored_dims = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| SlitherError::Storage(e.to_string()))? as usize;
    if stored_dims != dimensions {
        return Err(SlitherError::Storage(format!(
            "dimension mismatch: file has {} dimensions, expected {}",
            stored_dims, dimensions
        )));
    }

    let count = cursor
        .read_u64::<LittleEndian>()
        .map_err(|e| SlitherError::Storage(e.to_string()))? as usize;

    let mut records = Vec::with_capacity(count);

    for _ in 0..count {
        let doc_id = cursor
            .read_u64::<LittleEndian>()
            .map_err(|e| SlitherError::Storage(e.to_string()))?;

        let mut vector = vec![0f32; dimensions];
        for v in vector.iter_mut() {
            *v = cursor
                .read_f32::<LittleEndian>()
                .map_err(|e| SlitherError::Storage(e.to_string()))?;
        }

        records.push((doc_id, vector));
    }

    Ok(records)
}

fn flush_to_file(path: &Path, dimensions: usize, records: &[(u64, Vec<f32>)]) -> SlitherResult<()> {
    let mut file = File::create(path)?;

    file.write_all(MAGIC)?;
    file.write_u32::<LittleEndian>(VERSION)
        .map_err(io_to_storage)?;
    file.write_u32::<LittleEndian>(dimensions as u32)
        .map_err(io_to_storage)?;
    file.write_u64::<LittleEndian>(records.len() as u64)
        .map_err(io_to_storage)?;

    for (doc_id, vector) in records {
        file.write_u64::<LittleEndian>(*doc_id)
            .map_err(io_to_storage)?;
        for &v in vector.iter() {
            file.write_f32::<LittleEndian>(v).map_err(io_to_storage)?;
        }
    }

    file.flush()?;
    Ok(())
}

fn io_to_storage(e: io::Error) -> SlitherError {
    SlitherError::Storage(e.to_string())
}
