pub mod sinewave;

mod error;
mod local_file;
pub mod ytdl;

use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::local_file::LocalFileProvider;
use crate::provider::sinewave::SineWaveProvider;
use crate::provider::ytdl::YtdlProvider;
use crate::source::SourceKinds;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::error;

pub type SearchResult = ProviderResult<HashMap<String, String>>;

/// A trait for providing music tracks.
pub trait ProviderTrait {
    /// Get the name of the provider.
    ///
    /// This is used to identify the provider.
    /// It Should be unique and does not contain any whitespaces.
    fn get_name(&self) -> String;

    /// Search for tracks by keyword.
    /// It returns a HashMap of track name and its unique id.
    /// When no search result, return `ProviderError::EmptySearchResult`
    ///
    /// This operation might be expensive.
    fn search(&mut self, keyword: &str, max_results: Option<usize>) -> SearchResult;

    /// Get a track by its unique id.
    fn get_track(&self, id: &str) -> ProviderResult<SourceKinds>;
}

macro_rules! define_provider_kinds {
    (
        $f_name:ident=>$f_clz:ident
        $(,#[$feature:literal] $name:ident=>$clz:ident)*

    ) => {
        #[derive(Debug, Eq, PartialEq)]
        pub enum ProviderKinds{
            $f_name($f_clz)
            $(,#[cfg(feature=$feature)] $name ($clz))*
        }

        impl ProviderTrait for ProviderKinds {
            fn get_name(&self) -> String {
                match self {
                    Self::$f_name(kind) => kind.get_name()
                    $(,#[cfg(feature=$feature)] Self::$name(kind) => kind.get_name())*
                }
            }

            fn search(&mut self, term:&str, max_results: Option<usize>) -> SearchResult {
                match self {
                    Self::$f_name(kind) => kind.search(term, max_results)
                    $(,#[cfg(feature=$feature)] Self::$name(kind) => kind.search(term, max_results))*
                }
            }

            fn get_track(&self,input: &str) -> ProviderResult<SourceKinds> {
                match self {
                    Self::$f_name(kind) => kind.get_track(input)
                    $(,#[cfg(feature=$feature)] Self::$name(kind) => kind.get_track(input))*
                }
            }
        }
    };
}

define_provider_kinds! {
    Sinewave => SineWaveProvider,

    #["provider-local_file"]
    LocalFile => LocalFileProvider,

    #["provider-yt-dlp"]
    Ytdl => YtdlProvider
}

#[derive(Debug)]
pub struct ProviderRegistry {
    providers: HashMap<String, ProviderKinds>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn all_providers(&self) -> HashSet<String> {
        self.providers.keys().cloned().collect()
    }

    pub fn register(&mut self, kind: ProviderKinds) {
        self.providers.insert(kind.get_name(), kind);
    }

    pub fn unregister(&mut self, name: &str) {
        self.providers.remove(name);
    }

    pub fn create(&mut self, fields: ProviderFields) -> ProviderResult<()> {
        self.register(fields.try_into()?);
        Ok(())
    }

    pub fn search(
        &mut self,
        keyword: &str,
        max_results: Option<usize>,
        mut filter: impl FnMut(&String) -> bool,
    ) -> ProviderResult<HashMap<String, HashMap<String, String>>> {
        let keyword = keyword.trim();
        let mut result = HashMap::new();

        for (name, provider) in self.providers.iter_mut() {
            if !filter(name) {
                continue;
            }

            match provider.search(keyword, max_results) {
                Ok(results) => {
                    result.insert(name.to_string(), results.to_owned());
                }
                Err(error) => {
                    error!("Unable to search with {}: {}", name, error);
                    continue;
                }
            }
        }

        Ok(result)
    }

    pub fn get_track(&self, provider: &str, id: &str) -> ProviderResult<SourceKinds> {
        match self.providers.get(provider) {
            Some(provider) => provider.get_track(id),
            None => Err(ProviderError::ProviderNotFound(provider.to_string())),
        }
    }
}

/// Used for create new provider
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ProviderFields {
    Sinewave,

    #[cfg(feature = "provider-local_file")]
    LocalFile {
        music_folder: String,
    },

    #[cfg(feature = "provider-yt-dlp")]
    Ytdl,
}

impl TryFrom<ProviderFields> for ProviderKinds {
    type Error = ProviderError;

    fn try_from(fields: ProviderFields) -> Result<Self, Self::Error> {
        Ok(match fields {
            ProviderFields::Sinewave => ProviderKinds::Sinewave(SineWaveProvider),

            #[cfg(feature = "provider-local_file")]
            ProviderFields::LocalFile { music_folder } => {
                ProviderKinds::LocalFile(LocalFileProvider::new(music_folder))
            }

            #[cfg(feature = "provider-yt-dlp")]
            ProviderFields::Ytdl => ProviderKinds::Ytdl(YtdlProvider::try_new()?),
        })
    }
}
