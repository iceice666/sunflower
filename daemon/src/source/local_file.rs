use crate::source::error::SourceResult;
use crate::source::{RawAudioSource, SourceTrait, SourceInfo};
use rodio::Decoder;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Debug)]
pub struct LocalFileTrack {
    path: PathBuf,
}

impl LocalFileTrack {
    pub(crate) fn new(path: impl AsRef<str>) -> Self {
        let path = path.as_ref();
        let path = PathBuf::from(path);
        Self { path }
    }
}

impl SourceTrait for LocalFileTrack {
    fn info(&self) -> SourceResult<SourceInfo> {
        let metadata = HashMap::new();
        // TODO: Extract data from file
        Ok(metadata)
    }

    fn build_source(&self) -> SourceResult<RawAudioSource> {
        let file = BufReader::new(File::open(&self.path)?);
        let source = Decoder::new(file)?;
        let result = Box::new(source);

        Ok(RawAudioSource::I16(result))
    }

    fn get_unique_id(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}
