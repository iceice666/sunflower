use crate::*;

#[cfg(test)]
pub(crate) fn router_with_store(store: Option<PostgresStore>) -> Router {
    let auth_mode = if store.is_some() {
        AuthMode::Database
    } else {
        AuthMode::RejectAllTokens
    };
    router_with_state(auth_mode, store)
}

#[cfg(test)]
pub(crate) fn router_with_auth(auth_mode: AuthMode) -> Router {
    router_with_state(auth_mode, None)
}

#[cfg(test)]
pub(crate) fn router_with_state(auth_mode: AuthMode, store: Option<PostgresStore>) -> Router {
    router_with_state_and_data_dir(
        auth_mode,
        store,
        configured_data_dir(env::var("DATA_DIR").ok()),
    )
}

#[cfg(test)]
pub(crate) fn router_with_state_and_data_dir(
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
    data_dir: impl Into<String>,
) -> Router {
    router_with_state_and_config(auth_mode, store, data_dir, DEFAULT_SETUP_TOKEN, "", None)
}

#[cfg(test)]
pub(crate) fn router_with_state_and_config(
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
    data_dir: impl Into<String>,
    setup_token: impl Into<String>,
    public_base_url: impl Into<String>,
    cookie_key: Option<[u8; 32]>,
) -> Router {
    router_with_state_and_config_and_hub(
        auth_mode,
        store,
        data_dir,
        setup_token,
        public_base_url,
        cookie_key,
        Some(Arc::new(NowPlayingHub::default())),
    )
}

#[cfg(test)]
pub(crate) fn router_with_state_and_config_and_hub(
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
    data_dir: impl Into<String>,
    setup_token: impl Into<String>,
    public_base_url: impl Into<String>,
    cookie_key: Option<[u8; 32]>,
    hub: Option<Arc<NowPlayingHub>>,
) -> Router {
    router_with_config(
        RouterBuildConfig::new(
            auth_mode,
            store,
            data_dir,
            setup_token,
            public_base_url,
            cookie_key,
        )
        .with_hub(hub)
        .with_proxy(Some(Arc::new(StreamProxy::new(ProxySigner::new(
            random_stream_proxy_key(),
        ))))),
    )
}

#[cfg(test)]
pub(crate) fn test_router_config(
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
) -> RouterBuildConfig {
    RouterBuildConfig::new(auth_mode, store, "./data", DEFAULT_SETUP_TOKEN, "", None)
}

pub(crate) struct FakeInnerTube {
    pub(crate) home_page: innertube::HomePage,
    pub(crate) search_page: innertube::SearchPage,
    pub(crate) next_pages: Mutex<Vec<innertube::NextPage>>,
    pub(crate) player: innertube::PlayerResponse,
}

impl FakeInnerTube {
    pub(crate) fn with_player(player: innertube::PlayerResponse) -> Self {
        Self {
            home_page: innertube::HomePage::default(),
            search_page: innertube::SearchPage::default(),
            next_pages: Mutex::new(vec![]),
            player,
        }
    }
}

impl innertube::InnerTubeBackend for FakeInnerTube {
    fn browse<'a>(
        &'a self,
        _browse_id: &'a str,
        _continuation: Option<&'a str>,
    ) -> BoxFuture<'a, Result<innertube::HomePage, innertube::InnerTubeError>> {
        let page = self.home_page.clone();
        Box::pin(async move { Ok(page) })
    }

    fn search<'a>(
        &'a self,
        _query: &'a str,
    ) -> BoxFuture<'a, Result<innertube::SearchPage, innertube::InnerTubeError>> {
        let page = self.search_page.clone();
        Box::pin(async move { Ok(page) })
    }

    fn next<'a>(
        &'a self,
        _video_id: &'a str,
        _continuation: Option<&'a str>,
    ) -> BoxFuture<'a, Result<innertube::NextPage, innertube::InnerTubeError>> {
        let page = self.next_pages.lock().unwrap().remove(0);
        Box::pin(async move { Ok(page) })
    }

    fn player<'a>(
        &'a self,
        _video_id: &'a str,
    ) -> BoxFuture<'a, Result<innertube::PlayerResponse, innertube::InnerTubeError>> {
        let player = self.player.clone();
        Box::pin(async move { Ok(player) })
    }
}
