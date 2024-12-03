pub mod sinewave;

mod error;
mod local_file;

use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::local_file::LocalFileProvider;
use crate::provider::sinewave::SineWaveProvider;
use crate::source::SourceKinds;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::error;

pub type SearchResult<'a> = ProviderResult<&'a HashMap<String, String>>;

/// A trait for providing music tracks.
pub trait ProviderTrait: PartialEq + Eq {
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
    fn search(&mut self, keyword: &str) -> SearchResult;

    /// Get a track by its unique id.
    fn get_track(&self, id: &str) -> ProviderResult<SourceKinds>;
}

macro_rules! define_provider_kinds {
    (
        $f_name:ident=>$f_clz:ident
        $(,$name:ident=>$clz:ident)*

    ) => {
        #[derive(Debug, Eq, PartialEq)]
        pub enum ProviderKinds{
            $f_name($f_clz)
            $(,$name ($clz))*
        }

        impl ProviderTrait for ProviderKinds {
            fn get_name(&self) -> String {
                match self {
                    Self::$f_name(kind) => kind.get_name()
                    $(,Self::$name(kind) => kind.get_name())*
                }
            }

            fn search(&mut self, term:&str) -> SearchResult {
                match self {
                    Self::$f_name(kind) => kind.search(term)
                    $(,Self::$name(kind) => kind.search(term))*
                }
            }

            fn get_track(&self,input: &str) -> ProviderResult<SourceKinds> {
                match self {
                    Self::$f_name(kind) => kind.get_track(input)
                    $(,Self::$name(kind) => kind.get_track(input))*
                }
            }
        }
    };
}

define_provider_kinds! {
    Sinewave => SineWaveProvider,
    LocalFile => LocalFileProvider
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

    pub fn create(&mut self, fields: ProviderFields) {
        let provider = match fields {
            ProviderFields::Sinewave => ProviderKinds::Sinewave(SineWaveProvider),
            ProviderFields::LocalFile { music_folder } => {
                ProviderKinds::LocalFile(LocalFileProvider::new(music_folder))
            }
        };

        self.register(provider);
    }

    pub fn search(
        &mut self,
        keyword: &str,
        max_results: usize,
        mut filter: impl FnMut(&String) -> bool,
    ) -> ProviderResult<HashMap<String, HashMap<String, String>>> {
        let keyword = keyword.trim();
        let mut result = HashMap::new();

        for (name, provider) in self.providers.iter_mut() {
            if !filter(name) {
                continue;
            }

            match provider.search(keyword) {
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

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ProviderFields {
    Sinewave,
    LocalFile { music_folder: String },
}
