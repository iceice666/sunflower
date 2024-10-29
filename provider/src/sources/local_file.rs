use std::{borrow::Borrow, collections::HashMap, path::PathBuf};

use lofty::{file::TaggedFileExt, read_from_path, tag::Accessor};
use sunflower_player::{
    error::{PlayerError, PlayerResult},
    Track, TrackInfo, TrackObject, TrackSource,
};

use crate::{
    error::{ProviderError, ProviderResult},
    Provider,
};

pub struct LocalFileProvider {
    music_folder: PathBuf,
    __search_cache: HashMap<String, String>,
}

impl Provider for LocalFileProvider {
    fn get_name(&self) -> String {
        "LocalFileProvider".to_string()
    }

    fn search(
        &mut self,
        keyword: impl AsRef<str>,
    ) -> ProviderResult<impl Borrow<HashMap<String, String>> + '_> {
        let keyword = keyword.as_ref().to_lowercase();
        let mut result = HashMap::new();

        for entry in self.music_folder.read_dir()? {
            let path = entry?.path();

            if path.is_dir() {
                continue;
            }

            if let Some(filename_str) = path.file_name().and_then(|name| name.to_str()) {
                if filename_str.to_lowercase().contains(&keyword) {
                    if let Some(filepath_str) = path.to_str() {
                        result.insert(filename_str.to_string(), filepath_str.to_string());
                    }
                }
            }
        }

        self.__search_cache = result;
        Ok(self.__search_cache.borrow())
    }

    fn get_track(&self, name: impl AsRef<str>) -> ProviderResult<TrackObject> {
        let name = name.as_ref();
        let target_path = self
            .__search_cache
            .get(name)
            .ok_or(ProviderError::TrackNotFound(name.to_string()))?;

        let track = LocalFileSource::new(target_path);

        Ok(Box::new(track))
    }
}

pub(crate) struct LocalFileSource {
    path: PathBuf,
}

impl LocalFileSource {
    fn new(path: impl AsRef<str>) -> Self {
        let path = path.as_ref();
        let path = PathBuf::from(path);
        Self { path }
    }
}

impl Track for LocalFileSource {
    fn build_source(&self) -> PlayerResult<TrackSource> {
        todo!()
    }

    fn get_unique_id(&self) -> String {
        self.path.to_string_lossy().to_string()
    }

    fn info(&self) -> PlayerResult<TrackInfo> {
        let mut result = HashMap::new();

        let tagged_file = read_from_path(&self.path)
            .map_err(|e| PlayerError::UnableToFetchSourceInfo(format!("Lofty error: {}", e)))?;

        let Some(tag) = tagged_file.primary_tag().or(tagged_file.first_tag()) else {
            return Ok(result);
        };

        let title = tag.title().unwrap_or("<missing>".into()).to_string();
        let artist = tag.artist().unwrap_or("<missing>".into()).to_string();
        let album = tag.album().unwrap_or("<missing>".into()).to_string();
        let genre = tag.genre().unwrap_or("<missing>".into()).to_string();

        result.insert("title".to_string(), title);
        result.insert("artist".to_string(), artist);
        result.insert("album".to_string(), album);
        result.insert("genre".to_string(), genre);

        Ok(result)
    }
}
