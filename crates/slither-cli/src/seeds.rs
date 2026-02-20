use slither_core::{SlitherError, SlitherResult};
use std::path::{Path, PathBuf};

pub fn seeds_path(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("seeds.json")
}

pub fn load_seeds(data_dir: &str) -> Vec<String> {
    let path = seeds_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_seeds(data_dir: &str, seeds: &[String]) -> SlitherResult<()> {
    let path = seeds_path(data_dir);
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(seeds)?;
    std::fs::write(&tmp_path, json).map_err(SlitherError::Io)?;
    std::fs::rename(&tmp_path, &path).map_err(SlitherError::Io)?;
    Ok(())
}
