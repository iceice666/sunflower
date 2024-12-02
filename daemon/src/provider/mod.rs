pub mod sinewave;

mod error;
mod local_file;

use crate::provider::error::ProviderResult;
use crate::provider::sinewave::SineWaveProvider;
use crate::source::SourceKinds;
use std::collections::HashMap;

pub type SearchResult<'a> = ProviderResult<&'a HashMap<String, String>>;

/// A trait for providing music tracks.
pub trait ProviderTrait{
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
        #[derive(Debug)]
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
    Sinewave => SineWaveProvider
}
