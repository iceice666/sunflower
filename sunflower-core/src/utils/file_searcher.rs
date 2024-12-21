use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct FolderSearcher {
    folder: PathBuf,
    recursive: bool,
    max_results: Option<usize>,
}

impl FolderSearcher {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            folder: dir.as_ref().to_owned(),
            recursive: false,
            max_results: None,
        }
    }

    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    pub fn max_results(mut self, limit: Option<usize>) -> Self {
        self.max_results = limit;
        self
    }

    pub fn search(&self, pattern: Regex) -> std::io::Result<HashMap<String, String>> {
        let dir = &self.folder;
        let results = Arc::new(Mutex::new(HashMap::new()));
        let count = Arc::new(Mutex::new(0usize));

        self.search_internal(dir, &pattern, &results, &count)?;

        Ok(Arc::try_unwrap(results).unwrap().into_inner().unwrap())
    }

    fn search_internal(
        &self,
        dir: &Path,
        pattern: &Regex,
        results: &Arc<Mutex<HashMap<String, String>>>,
        count: &Arc<Mutex<usize>>,
    ) -> std::io::Result<()> {
        let entries: Vec<_> = dir.read_dir()?.collect::<Result<_, _>>()?;

        entries
            .par_iter()
            .try_for_each(|entry| self.process_entry(entry, pattern, results, count))?;

        Ok(())
    }

    fn process_entry(
        &self,
        entry: &std::fs::DirEntry,
        pattern: &Regex,
        results: &Arc<Mutex<HashMap<String, String>>>,
        count: &Arc<Mutex<usize>>,
    ) -> std::io::Result<()> {
        let path = entry.path();
        let metadata = entry.metadata()?;

        // Check if we've hit the limit
        if let Some(limit) = self.max_results {
            if *count.lock().unwrap() >= limit {
                return Ok(());
            }
        }

        if metadata.is_dir() && self.recursive {
            self.search_internal(&path, pattern, results, count)?;
        } else if metadata.is_file() {
            // Check filename match
            if let Some(filename) = path.file_name() {
                if let Some(filename_str) = filename.to_str() {
                    if pattern.is_match(filename_str) {
                        let mut results = results.lock().unwrap();
                        let mut count = count.lock().unwrap();
                        results
                            .insert(filename_str.to_string(), path.to_string_lossy().to_string());
                        *count += 1;
                    }
                }
            }
        }

        Ok(())
    }
}
