use crate::*;

pub(crate) struct RouterBuildConfig {
    pub(crate) auth_mode: AuthMode,
    pub(crate) store: Option<PostgresStore>,
    pub(crate) data_dir: String,
    pub(crate) setup_token: String,
    pub(crate) public_base_url: String,
    pub(crate) cookie_key: Option<[u8; 32]>,
    pub(crate) hub: Option<Arc<NowPlayingHub>>,
    pub(crate) proxy: Option<Arc<StreamProxy>>,
    pub(crate) proxy_youtube: bool,
    pub(crate) yt: Option<Arc<dyn innertube::InnerTubeBackend>>,
    pub(crate) dev_open_registration: bool,
}

impl RouterBuildConfig {
    pub(crate) fn new(
        auth_mode: AuthMode,
        store: Option<PostgresStore>,
        data_dir: impl Into<String>,
        setup_token: impl Into<String>,
        public_base_url: impl Into<String>,
        cookie_key: Option<[u8; 32]>,
    ) -> Self {
        Self {
            auth_mode,
            store,
            data_dir: data_dir.into(),
            setup_token: setup_token.into(),
            public_base_url: public_base_url.into(),
            cookie_key,
            hub: None,
            proxy: None,
            proxy_youtube: false,
            yt: None,
            dev_open_registration: false,
        }
    }

    pub(crate) fn with_hub(mut self, hub: Option<Arc<NowPlayingHub>>) -> Self {
        self.hub = hub;
        self
    }

    pub(crate) fn with_proxy(mut self, proxy: Option<Arc<StreamProxy>>) -> Self {
        self.proxy = proxy;
        self
    }

    pub(crate) fn with_proxy_youtube(mut self, proxy_youtube: bool) -> Self {
        self.proxy_youtube = proxy_youtube;
        self
    }

    pub(crate) fn with_yt(mut self, yt: Option<Arc<dyn innertube::InnerTubeBackend>>) -> Self {
        self.yt = yt;
        self
    }

    pub(crate) fn with_dev_open_registration(mut self, dev_open_registration: bool) -> Self {
        self.dev_open_registration = dev_open_registration;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthMode {
    Database,
    RejectAllTokens,
    #[cfg(test)]
    AllowAllForContractTests,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) auth_mode: AuthMode,
    pub(crate) server_version: String,
    pub(crate) setup_token: String,
    pub(crate) public_base_url: String,
    pub(crate) cookie_key: Option<[u8; 32]>,
    pub(crate) hub: Option<Arc<NowPlayingHub>>,
    pub(crate) proxy: Option<Arc<StreamProxy>>,
    pub(crate) proxy_youtube: bool,
    pub(crate) yt: Option<Arc<dyn innertube::InnerTubeBackend>>,
    pub(crate) jobs: Arc<JobRegistry>,
    pub(crate) started_at: SystemTime,
    pub(crate) data_dir: String,
    pub(crate) dev_open_registration: bool,
    pub(crate) setup_limiter: RateLimiter,
    pub(crate) admin_login_limiter: RateLimiter,
    pub(crate) pairing_limiter: RateLimiter,
    pub(crate) store: Option<PostgresStore>,
}

#[derive(Clone)]
pub(crate) struct RateLimiter {
    limit: usize,
    window: Duration,
    entries: Arc<Mutex<HashMap<String, RateEntry>>>,
}

#[derive(Clone, Copy)]
pub(crate) struct RateEntry {
    start: SystemTime,
    count: usize,
}

impl RateLimiter {
    pub(crate) fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn allow(&self, key: &str) -> bool {
        let now = SystemTime::now();
        let Ok(mut entries) = self.entries.lock() else {
            return true;
        };
        let entry = entries.entry(key.to_string()).or_insert(RateEntry {
            start: now,
            count: 0,
        });
        if now.duration_since(entry.start).unwrap_or_default() > self.window {
            *entry = RateEntry {
                start: now,
                count: 0,
            };
        }
        if entry.count >= self.limit {
            return false;
        }
        entry.count += 1;
        true
    }

    pub(crate) fn reset(&self, key: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(key);
        }
    }
}

pub(crate) fn rate_limit_key(connect_info: Option<ConnectInfo<SocketAddr>>) -> String {
    connect_info
        .map(|ConnectInfo(addr)| addr.to_string())
        .unwrap_or_default()
}
