pub(crate) mod local_file;
pub(crate) mod sine_wave;

use sine_wave::SineWaveProvider;

#[cfg(feature = "local")]
use local_file::LocalFileProvider;

use crate::player::track::TrackObject;
use crate::provider::error::ProviderResult;
use crate::provider::{Provider, SearchResult};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::LazyLock;
use tracing::error;

static JUST_A_EMPTY_HASHMAP: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

#[derive(Eq, PartialEq)]
pub enum Providers {
    SineWave {
        inner: SineWaveProvider,
    },

    #[cfg(feature = "local")]
    LocalFile {
        inner: LocalFileProvider,
    },
}

macro_rules! manipulate {
    ($this:expr ,$func:ident $(, $arg:expr)*) => {
        match $this {
            Self::SineWave { inner } => inner.$func($($arg),*).await,

            #[cfg(feature = "local")]
            Self::LocalFile { inner } => inner.$func($($arg),*).await,
        }
    };
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
struct ProviderRegistry {
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
}

impl ProviderRegistry {
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
}
