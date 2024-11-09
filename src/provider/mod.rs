use std::{collections::HashMap, hash::Hash};

use error::ProviderResult;

pub mod error;
pub mod sources;

#[cfg(test)]
mod tests;

use crate::player::track::TrackObject;

pub type SearchResult<'a> = ProviderResult<&'a HashMap<String, String>>;

#[async_trait::async_trait]
/// A trait for providing music tracks.
pub trait Provider: TryFrom<HashMap<String, String>> {
    /// Get the name of the provider.
    ///
    /// This is used to identify the provider.
    /// Should be unique and does not contain any whitespaces.
    async fn get_name(&self) -> String;

    /// Search for tracks by keyword. It returns a HashMap of track name and its unique id.
    ///
    /// This operation might be expensive.
    async fn search(&mut self, keyword: &str) -> SearchResult;

    /// Get a track by its unique id.
    async fn get_track(&self, id: &str) -> ProviderResult<TrackObject>;
}
