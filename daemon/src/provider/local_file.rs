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
use regex::Regex;

#[derive(PartialEq, Eq, Debug)]
pub struct LocalFileProvider {
    music_folder: PathBuf,
    __search_cache: HashMap<String, String>,
}

impl LocalFileProvider {
    pub fn new(music_folder: impl AsRef<Path>) -> Self {
        let music_folder = PathBuf::from(music_folder.as_ref());
        Self {
            music_folder,
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

        let mut result = HashMap::new();
        let mut count = 0;

        for entry in self.music_folder.read_dir()? {
            // Check if we've hit the limit
            if let Some(limit) = max_result {
                if count >= limit {
                    break;
                }
            }

            let path = entry?.path();

            if path.is_dir() {
                continue;
            }

            if let Some(filename_str) = path.file_name().and_then(|name| name.to_str()) {
                // Use regex matching instead of simple contains
                if regex.is_match(filename_str) {
                    if let Some(filepath_str) = path.to_str() {
                        result.insert(filename_str.to_string(), filepath_str.to_string());
                        count += 1;
                    }
                }
            }
        }

        self.__search_cache = result;
        Ok(self.__search_cache.clone())
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
