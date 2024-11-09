use crate::player::error::{PlayerError, PlayerResult};
use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::sources::{Track, TrackInfo, TrackSource};
use lofty::file::TaggedFileExt;
use lofty::prelude::Accessor;
use lofty::read_from_path;
use rodio::Decoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

pub(crate) struct LocalFileTrack {
    path: PathBuf,
}

impl LocalFileTrack {
    pub(crate) fn new(path: impl AsRef<str>) -> Self {
        let path = path.as_ref();
        let path = PathBuf::from(path);
        Self { path }
    }
}

impl Track for LocalFileTrack {
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

    fn build_source(&self) -> PlayerResult<TrackSource> {
        let file = BufReader::new(File::open(&self.path)?);
        let source = Decoder::new(file)?;
        let result = Box::new(source);

        Ok(TrackSource::I16(result))
    }

    fn get_unique_id(&self) -> String {
        self.path.to_string_lossy().to_string()
    }

    fn try_from_config(config: HashMap<String, String>) -> ProviderResult<Self>
    where
        Self: Sized,
    {
        let path = config
            .get("path")
            .ok_or(ProviderError::MissingField("path".to_string()))?;
        let path = PathBuf::from(path);
        Ok(Self { path })
    }
}
