use std::{borrow::Borrow, collections::HashMap};

use error::ProviderResult;

pub mod error;
pub mod sources;

#[cfg(test)]
mod tests;

use sunflower_player::track::TrackObject;

/// A trait for providing music tracks.
pub trait Provider {
    /// Get the name of the provider.
    fn get_name(&self) -> String;

    /// Search for tracks by keyword. It returns a HashMap of track name and its unique id.
    fn search(
        &mut self,
        keyword: impl AsRef<str>,
    ) -> ProviderResult<impl Borrow<HashMap<String, String>> + '_>;

    /// Get a track by its unique id.
    fn get_track(&self, id: impl AsRef<str>) -> ProviderResult<TrackObject>;
}
