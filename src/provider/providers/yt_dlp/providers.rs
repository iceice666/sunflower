use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::providers::yt_dlp::helper::{SearchOption, SearchPlatform, YtDlp};
use crate::provider::sources::TrackObject;
use crate::provider::{Provider, SearchResult};
use std::collections::HashMap;

use super::helper::DownloadOption;

#[derive(Debug,PartialEq, Eq)]
pub struct YoutubeDownloadProvider {
    yt_dlp: YtDlp,
    __cache: HashMap<String, String>,
}

impl TryFrom<HashMap<String, String>> for YoutubeDownloadProvider {
    type Error = ProviderError;

    fn try_from(_: HashMap<String, String>) -> Result<Self, Self::Error> {
        Ok(Self {
            yt_dlp: YtDlp::try_new()?,
            __cache: HashMap::new(),
        })
    }
}

#[async_trait::async_trait]
impl Provider for YoutubeDownloadProvider {
    async fn get_name(&self) -> String {
        "YoutubeDownloadProvider".to_string()
    }

    async fn search(&mut self, keyword: &str) -> SearchResult {
        let query = SearchOption {
            platform: SearchPlatform::Youtube,
            len: 0,
            keyword: keyword.to_string(),
        };

        self.__cache = self.yt_dlp.search(query).await?;

        Ok(&self.__cache)
    }

    async fn get_track(&self, video_id: &str) -> ProviderResult<TrackObject> {
        let query = DownloadOption {
            platform: SearchPlatform::Youtube,
            video_id: video_id.to_string(),
        };

        let track = self.yt_dlp.download(query).await?;
        Ok(track)
    }
}
