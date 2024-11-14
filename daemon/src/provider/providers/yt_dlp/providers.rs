use super::helper::DownloadOption;
use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::providers::yt_dlp::helper::{SearchOption, SearchPlatform, YtDlp};
use crate::provider::sources::TrackObject;
use crate::provider::{Provider, SearchResult};
use std::collections::HashMap;
use tracing::debug;

macro_rules! add_provider {
    (
        $name: ident,
        $platform: ident
    ) => {
        #[derive(Debug, PartialEq, Eq)]
        pub struct $name {
            yt_dlp: YtDlp,
            __cache: HashMap<String, String>,
        }

        impl TryFrom<HashMap<String, String>> for $name {
            type Error = ProviderError;

            fn try_from(_: HashMap<String, String>) -> Result<Self, Self::Error> {
                Ok(Self {
                    yt_dlp: YtDlp::try_new()?,
                    __cache: HashMap::new(),
                })
            }
        }

        #[async_trait::async_trait]
        impl Provider for $name {
            async fn get_name(&self) -> String {
                stringify!($name).to_string()
            }

            async fn search(&mut self, query: &str) -> SearchResult {
                // pattern: search amonut + keyword
                let (len, keyword) = query
                    .split_once('+')
                    .ok_or(ProviderError::InvalidData("Invalid query".to_string()))?;
                let len = len
                    .trim()
                    .parse()
                    .map_err(|e| ProviderError::InvalidData(format!("{}", e)))?;

                let query = SearchOption {
                    platform: SearchPlatform::$platform,
                    len,
                    keyword: keyword.to_string(),
                };

                self.__cache = self.yt_dlp.search(query).await?;

                Ok(&self.__cache)
            }

            async fn get_track(&self, video_id: &str) -> ProviderResult<TrackObject> {
                let query = DownloadOption {
                    platform: SearchPlatform::$platform,
                    video_id: video_id.to_string(),
                };

                let track = self.yt_dlp.download(query).await?;
                Ok(track)
            }
        }
    };
}

add_provider!(YoutubeDownloadProvider, Youtube);
add_provider!(BiliBiliDownloadProvider, BiliBili);
add_provider!(SoundCloudDownloadProvider, SoundCloud);
add_provider!(UrlDownloadProvider, UrlSpecified);
