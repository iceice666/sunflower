use std::path::Path;
use std::{
    collections::HashMap,
    io::{self},
    path::PathBuf,
};

use crate::provider::SearchResult;
use crate::provider::{
    error::{ProviderError, ProviderResult},
    ProviderTrait,
};
use crate::source::local_file::LocalFileTrack;
use crate::source::SourceKinds;
use crate::utils::file_searcher::FolderSearcher;
use regex::Regex;

#[derive(PartialEq, Eq, Debug)]
pub struct LocalFileProvider {
    music_folder: PathBuf,
    recursive_scan: bool,
    __search_cache: HashMap<String, String>,
}

impl LocalFileProvider {
    pub fn new(music_folder: impl AsRef<Path>, recursive_scan: bool) -> Self {
        let music_folder = PathBuf::from(music_folder.as_ref());
        Self {
            music_folder,
            recursive_scan,
            __search_cache: HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl ProviderTrait for LocalFileProvider {
    fn get_name(&self) -> String {
        "LocalFileProvider".to_string()
    }

    fn search(&mut self, pattern_regex: &str, max_result: Option<usize>) -> SearchResult {
        // Create a regex pattern, case-insensitive by default
        let regex = Regex::new(&format!("(?i){}", pattern_regex))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let searcher = FolderSearcher::new(&self.music_folder)
            .recursive(self.recursive_scan)
            .max_results(max_result);

        let result = searcher.search(regex)?;

        self.__search_cache = result.clone();
        Ok(result)
    }

    fn get_track(&self, name: &str) -> ProviderResult<SourceKinds> {
        let target_path = self
            .__search_cache
            .get(name)
            .ok_or(ProviderError::TrackNotFound(name.to_string()))?;

        let track = LocalFileTrack::new(target_path);

        Ok(track.into())
    }
}

impl LocalFileProvider {
    fn search_folder(
        dir: impl AsRef<Path>,
        pattern_regex: &Regex,
        recursive_scan: bool,
        max_result: Option<usize>,
    ) -> ProviderResult<(HashMap<String, String>, usize)> {
        let mut result = HashMap::new();
        let mut count = 0;
        let dir: &Path = dir.as_ref();

        for entry in dir.read_dir()? {
            // Check if we've hit the limit
            if let Some(limit) = max_result {
                if count >= limit {
                    break;
                }
            }

            let path = entry?.path();

            if let Some(filename_str) = path.file_name().and_then(|name| name.to_str()) {
                // Use regex matching instead of simple contains
                if pattern_regex.is_match(filename_str) {
                    if let Some(filepath_str) = path.to_str() {
                        result.insert(filename_str.to_string(), filepath_str.to_string());
                        count += 1;
                    }
                }
            }

            if path.is_dir() && recursive_scan {
                let (r, c) = Self::search_folder(path, pattern_regex, recursive_scan, max_result)?;

                result.extend(r);
                count += c;
            }
        }

        Ok((result, count))
    }
}
