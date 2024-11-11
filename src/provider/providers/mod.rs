// HINT: $PROVIDER_IMPL$: Remember adding others provider/track implementations here
pub(crate) mod sine_wave;
use sine_wave::SineWaveProvider;

#[cfg(feature = "provider-local_file")]
pub(crate) mod local_file;
#[cfg(feature = "provider-local_file")]
use local_file::*;

#[cfg(feature = "provider-yt-dlp")]
pub(crate) mod yt_dlp;
#[cfg(feature = "provider-yt-dlp")]
use yt_dlp::*;

////////////////////////////////////////////////////////////////////////

use crate::provider::error::{ProviderError, ProviderResult};
use crate::provider::sources::TrackObject;
use crate::provider::{Provider, SearchResult};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::LazyLock;
use tracing::error;

pub(crate) static JUST_A_EMPTY_HASHMAP: LazyLock<HashMap<String, String>> =
    LazyLock::new(HashMap::new);

#[derive(Eq, PartialEq)]
pub enum Providers {
    // HINT: $PROVIDER_IMPL$: Remember adding others provider/track implementations here
    SineWave {
        inner: SineWaveProvider,
    },

    #[cfg(feature = "provider-local_file")]
    LocalFile {
        inner: LocalFileProvider,
    },

    #[cfg(feature = "provider-yt-dlp")]
    YoutubeDownload {
        inner: YoutubeDownloadProvider,
    },
}

// HINT: $PROVIDER_IMPL$: Remember adding others provider/track implementations here
macro_rules! manipulate {
    ($this:expr ,$func:ident $(, $arg:expr)*) => {
        match $this {
            Self::SineWave { inner } => inner.$func($($arg),*).await,

            #[cfg(feature = "provider-local_file")]
            Self::LocalFile { inner } => inner.$func($($arg),*).await,

            #[cfg(feature = "provider-yt-dlp")]
            Self::YoutubeDownload { inner } => inner.$func($($arg),*).await,
        }
    };
}

impl TryFrom<HashMap<String, String>> for Providers {
    type Error = ProviderError;

    fn try_from(mut value: HashMap<String, String>) -> Result<Self, Self::Error> {
        let provider = value
            .remove("provider_name")
            .ok_or(ProviderError::MissingField("provider_name".to_string()))?;

        // HINT: $PROVIDER_IMPL$: Remember adding others provider/track implementations here
        // Use lowercase and underscore for provider name
        match provider.as_str() {
            "sine_wave" => Ok(Self::SineWave {
                inner: SineWaveProvider,
            }),

            #[cfg(feature = "provider-local_file")]
            "local_file" => Ok(Self::LocalFile {
                inner: LocalFileProvider::try_from(value)?,
            }),

            #[cfg(feature = "provider-yt-dlp")]
            "youtube_download" => Ok(Self::YoutubeDownload {
                inner: YoutubeDownloadProvider::try_from(value)?,
            }),

            _ => Err(ProviderError::ProviderNotFound(provider)),
        }
    }
}

#[async_trait::async_trait]
impl Provider for Providers {
    async fn get_name(&self) -> String {
        manipulate!(self, get_name)
    }

    async fn search(&mut self, keyword: &str) -> SearchResult {
        manipulate!(self, search, keyword)
    }

    async fn get_track(&self, id: &str) -> ProviderResult<TrackObject> {
        manipulate!(self, get_track, id)
    }
}
pub struct ProviderRegistry {
    inner: HashMap<String, Providers>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub async fn register(&mut self, reg: Providers) {
        let key = reg.get_name().await;
        self.inner.insert(key, reg);
    }

    pub fn unregister(&mut self, reg_name: impl AsRef<str>) {
        let reg_name = reg_name.as_ref();
        self.inner.remove(reg_name);
    }

    pub fn providers(&self) -> Vec<&String> {
        self.inner.keys().collect()
    }

    pub async fn search_all(
        &mut self,
        keyword: impl AsRef<str>,
    ) -> ProviderResult<HashMap<String, &HashMap<String, String>>> {
        self.search(keyword, |_| true).await
    }

    pub async fn search(
        &mut self,
        keyword: impl AsRef<str>,
        mut filter: impl FnMut(&String) -> bool,
    ) -> ProviderResult<HashMap<String, &HashMap<String, String>>> {
        let keyword = keyword.as_ref();
        let mut result = HashMap::new();

        for (name, provider) in &mut self.inner {
            if !filter(name) {
                continue;
            }

            match provider.search(keyword).await {
                Ok(search_result) => result.insert(name.to_string(), search_result),
                Err(e) => {
                    error!("{e}");
                    result.insert(format!("err_{name}"), JUST_A_EMPTY_HASHMAP.deref())
                }
            };
        }

        Ok(result)
    }

    pub async fn get_track(
        &self,
        provider: impl AsRef<str>,
        id: impl AsRef<str>,
    ) -> ProviderResult<TrackObject> {
        let provider = provider.as_ref();
        let id = id.as_ref();

        match self.inner.get(provider) {
            Some(provider) => provider.get_track(id).await,
            None => Err(ProviderError::ProviderNotFound(provider.to_string())),
        }
    }
}
