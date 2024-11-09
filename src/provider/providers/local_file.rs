use std::path::Path;
use std::{
    borrow::Borrow,
    collections::HashMap,
    io::{self},
    path::PathBuf,
};

use crate::provider::sources::TrackObject;
use crate::provider::sources::local_file::LocalFileTrack;
use crate::provider::SearchResult;
use crate::provider::{
    error::{ProviderError, ProviderResult},
    Provider,
};
use regex::Regex;

#[derive(PartialEq, Eq)]
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

impl TryFrom<HashMap<String, String>> for LocalFileProvider {
    type Error = ProviderError;

    fn try_from(value: HashMap<String, String>) -> Result<Self, Self::Error> {
        let music_folder =
            value
                .get("music_folder")
                .ok_or(ProviderError::MissingField(
                    "music_folder".to_string()
                ))?;

        Ok(Self::new(music_folder))
    }
}

#[async_trait::async_trait]
impl Provider for LocalFileProvider {
    async fn get_name(&self) -> String {
        "LocalFileProvider".to_string()
    }

    async fn search(&mut self, pattern_regex: &str) -> SearchResult {
        // Create a regex pattern, case-insensitive by default
        let regex = Regex::new(&format!("(?i){}", pattern_regex))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let mut result = HashMap::new();

        for entry in self.music_folder.read_dir()? {
            let path = entry?.path();

            if path.is_dir() {
                continue;
            }

            if let Some(filename_str) = path.file_name().and_then(|name| name.to_str()) {
                // Use regex matching instead of simple contains
                if regex.is_match(filename_str) {
                    if let Some(filepath_str) = path.to_str() {
                        result.insert(filename_str.to_string(), filepath_str.to_string());
                    }
                }
            }
        }

        self.__search_cache = result;
        Ok(self.__search_cache.borrow())
    }

    async fn get_track(&self, name: &str) -> ProviderResult<TrackObject> {
        let target_path = self
            .__search_cache
            .get(name)
            .ok_or(ProviderError::TrackNotFound(name.to_string()))?;

        let track = LocalFileTrack::new(target_path);

        Ok(Box::new(track))
    }
}
