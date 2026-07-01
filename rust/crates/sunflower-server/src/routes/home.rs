use crate::*;

pub(crate) async fn get_home(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "recs_unavailable");
    };
    let hide_explicit = bool_param(uri.query(), "hide_explicit");
    let hide_video = bool_param(uri.query(), "hide_video");
    let hide_shorts = bool_param(uri.query(), "hide_shorts");
    let cached_home = store
        .cached_home(auth.user_id, hide_explicit, hide_video, hide_shorts)
        .await
        .ok()
        .flatten();
    if let Some(cached) = cached_home.as_ref().filter(|cached| cached.fresh) {
        return Json(cached.home.clone()).into_response();
    }

    let (candidates, stats) = match store
        .local_home_inputs(
            auth.user_id,
            auth.device_id,
            HOME_LOCAL_CANDIDATE_LIMIT,
            hide_explicit,
            hide_video,
        )
        .await
    {
        Ok(inputs) => inputs,
        Err(_) => {
            if let Some(cached) = cached_home {
                return Json(cached.home).into_response();
            }
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    let ranked =
        LocalRecommendationEngine::default().rank(&candidates, &stats, HOME_QUICK_PICKS_LIMIT);
    let items: Vec<_> = ranked
        .into_iter()
        .map(|candidate| HomeItemResponse {
            source: candidate.media_id.source().to_string(),
            media_id: candidate.media_id.0,
            title: candidate.title,
            artists: candidate.artists,
            album_id: candidate.album_id.map(|album_id| album_id.0),
            duration_ms: candidate.duration_ms,
            thumbnail_url: None,
            score: 0.0,
        })
        .collect();
    let mut sections = if items.is_empty() {
        vec![]
    } else {
        vec![HomeSectionResponse {
            id: "quick_picks".into(),
            title: "Quick Picks".into(),
            kind: "quick_picks".into(),
            seed: None,
            items,
        }]
    };

    // Fetch impression counts once; all remote section builders share this map.
    let impressions = store
        .recent_impression_counts(auth.user_id)
        .await
        .unwrap_or_default();

    // Fan out all four remote section builders concurrently.
    let (daily, similar_artists, yt_home_result, community) = tokio::join!(
        daily_discover_section(
            &state,
            store,
            auth.user_id,
            &impressions,
            hide_explicit,
            hide_video,
            hide_shorts
        ),
        similar_artist_sections(
            &state,
            store,
            auth.user_id,
            &impressions,
            hide_explicit,
            hide_video,
            hide_shorts
        ),
        youtube_home_section(&state, &impressions, hide_explicit, hide_video, hide_shorts),
        community_playlists_section(&state, &impressions, hide_explicit, hide_video, hide_shorts),
    );

    if let Some(daily_discover) = daily {
        sections.push(daily_discover);
    }
    for similar_artist in similar_artists {
        sections.push(similar_artist);
    }
    let mut chips = vec![];
    if let Some((yt_home, yt_chips)) = yt_home_result {
        chips = yt_chips;
        sections.push(yt_home);
    }
    if let Some(community_playlists) = community {
        sections.push(community_playlists);
    }

    let home = HomeResponse {
        sections,
        chips,
        stale: false,
    };
    let _ = store
        .put_home_cache(auth.user_id, hide_explicit, hide_video, hide_shorts, &home)
        .await;
    Json(home).into_response()
}

pub(crate) async fn youtube_home_section(
    state: &AppState,
    impressions: &std::collections::HashMap<String, i32>,
    hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Option<(HomeSectionResponse, Vec<String>)> {
    let yt = state.yt.as_ref()?;
    let page = tokio::time::timeout(Duration::from_secs(8), yt.browse(YT_HOME_BROWSE_ID, None))
        .await
        .ok()?
        .ok()?;

    let mut candidates = Vec::new();
    for section in page.sections {
        for song in section.songs {
            if let Some(candidate) = remote_home_candidate_from_song(song, 0.5)
                && !(hide_explicit && candidate.is_explicit)
            {
                candidates.push(candidate);
            }
        }
    }
    let items = rank_remote_home(candidates, impressions, YT_HOME_LIMIT);
    if items.is_empty() {
        return None;
    }
    Some((
        HomeSectionResponse {
            id: "yt_home".into(),
            title: "From YouTube Music".into(),
            kind: "yt_home".into(),
            seed: None,
            items,
        },
        page.chips,
    ))
}

pub(crate) async fn similar_artist_sections(
    state: &AppState,
    store: &PostgresStore,
    user_id: Uuid,
    impressions: &std::collections::HashMap<String, i32>,
    hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Vec<HomeSectionResponse> {
    let Some(yt) = state.yt.as_ref() else {
        return vec![];
    };
    let artists = store
        .most_played_artists(user_id, SIMILAR_ARTIST_SECTIONS)
        .await
        .unwrap_or_default();
    if artists.is_empty() {
        return vec![];
    }

    // Collect (browse_id, artist_name) pairs up front so the async blocks
    // borrow stable `&str` / `&String` references from a local Vec.
    let browse_pairs: Vec<(String, String)> = artists
        .into_iter()
        .filter(|a| !a.artist_id.is_empty())
        .map(|a| {
            let browse_id = a
                .artist_id
                .strip_prefix("yt:")
                .unwrap_or(&a.artist_id)
                .to_string();
            (browse_id, a.artist_name)
        })
        .collect();

    // Fan out all artist browse calls concurrently; each has its own 8 s timeout.
    // `into_iter` gives owned Strings so each future can move them in.
    let section_futures: Vec<_> = browse_pairs
        .into_iter()
        .map(|(browse_id, artist_name)| async move {
            let page =
                match tokio::time::timeout(Duration::from_secs(8), yt.browse(&browse_id, None))
                    .await
                {
                    Ok(Ok(page)) => page,
                    Ok(Err(_)) | Err(_) => return None,
                };
            let candidates = page
                .sections
                .into_iter()
                .flat_map(|section| section.songs)
                .filter_map(|song| remote_home_candidate_from_song(song, 0.6))
                .filter(|c| !(hide_explicit && c.is_explicit))
                .collect::<Vec<_>>();
            let items = rank_remote_home(candidates, impressions, SIMILAR_ARTIST_LIMIT);
            if items.is_empty() {
                return None;
            }
            Some(HomeSectionResponse {
                id: format!("similar_artist:{browse_id}"),
                title: format!("Similar to {}", artist_name),
                kind: "similar_artist".into(),
                seed: Some(artist_name.clone()),
                items,
            })
        })
        .collect();

    futures_util::future::join_all(section_futures)
        .await
        .into_iter()
        .flatten()
        .collect()
}

pub(crate) async fn daily_discover_section(
    state: &AppState,
    store: &PostgresStore,
    user_id: Uuid,
    impressions: &std::collections::HashMap<String, i32>,
    hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Option<HomeSectionResponse> {
    let yt = state.yt.as_ref()?;
    let seeds = store
        .liked_yt_seed_media_ids(user_id, DAILY_DISCOVER_SEEDS as i64)
        .await
        .ok()?;
    if seeds.is_empty() {
        return None;
    }
    let seed_set = seeds
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for seed in seeds {
        let video_id = seed.strip_prefix("yt:").unwrap_or(&seed);
        let page = match tokio::time::timeout(Duration::from_secs(8), yt.next(video_id, None)).await
        {
            Ok(Ok(page)) => page,
            Ok(Err(_)) | Err(_) => continue,
        };
        for song in page.related {
            let Some(candidate) = remote_home_candidate_from_song(song, 0.7) else {
                continue;
            };
            if hide_explicit && candidate.is_explicit {
                continue;
            }
            if seed_set.contains(&candidate.media_id) || !seen.insert(candidate.media_id.clone()) {
                continue;
            }
            candidates.push(candidate);
        }
    }

    let items = rank_remote_home(candidates, impressions, DAILY_DISCOVER_LIMIT);
    if items.is_empty() {
        return None;
    }
    Some(HomeSectionResponse {
        id: "daily_discover".into(),
        title: "Daily Discover".into(),
        kind: "daily_discover".into(),
        seed: None,
        items,
    })
}

pub(crate) async fn community_playlists_section(
    state: &AppState,
    impressions: &std::collections::HashMap<String, i32>,
    hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Option<HomeSectionResponse> {
    let yt = state.yt.as_ref()?;
    let page =
        match tokio::time::timeout(Duration::from_secs(8), yt.search("popular music playlists"))
            .await
        {
            Ok(Ok(page)) => page,
            Ok(Err(_)) | Err(_) => return None,
        };
    let candidates = page
        .songs
        .into_iter()
        .filter_map(|song| remote_home_candidate_from_song(song, 0.4))
        .filter(|c| !(hide_explicit && c.is_explicit))
        .collect::<Vec<_>>();
    let items = rank_remote_home(candidates, impressions, COMMUNITY_PLAYLIST_LIMIT);
    if items.is_empty() {
        return None;
    }
    Some(HomeSectionResponse {
        id: "community_playlists".into(),
        title: "Community Playlists".into(),
        kind: "community_playlists".into(),
        seed: None,
        items,
    })
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteHomeCandidate {
    media_id: String,
    title: String,
    artists: Vec<String>,
    duration_ms: i32,
    thumbnail_url: Option<String>,
    remote_confidence: f64,
    is_explicit: bool,
}

pub(crate) fn remote_home_candidate_from_song(
    song: innertube::SongItem,
    remote_confidence: f64,
) -> Option<RemoteHomeCandidate> {
    if song.video_id.is_empty() {
        return None;
    }
    Some(RemoteHomeCandidate {
        media_id: format!("yt:{}", song.video_id.trim_start_matches("yt:")),
        title: song.title,
        artists: song.artists,
        duration_ms: song.duration_ms,
        thumbnail_url: non_empty_string(song.thumbnail_url),
        remote_confidence,
        is_explicit: song.is_explicit,
    })
}

pub(crate) fn rank_remote_home(
    candidates: Vec<RemoteHomeCandidate>,
    impressions: &std::collections::HashMap<String, i32>,
    limit: usize,
) -> Vec<HomeItemResponse> {
    if limit == 0 || candidates.is_empty() {
        return vec![];
    }

    let mut seen = std::collections::HashSet::new();
    let mut pool = candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.media_id.clone()))
        .map(|candidate| {
            let base = remote_home_score(
                &candidate,
                impressions
                    .get(&candidate.media_id)
                    .copied()
                    .unwrap_or_default(),
                0.0,
            );
            (candidate, base)
        })
        .collect::<Vec<_>>();
    pool.sort_by(|(_, left), (_, right)| right.total_cmp(left));

    let mut used = vec![false; pool.len()];
    let mut artist_counts = std::collections::HashMap::<String, i32>::new();
    let mut out = Vec::with_capacity(limit.min(pool.len()));
    while out.len() < limit {
        let mut best = None;
        let mut best_score = f64::NEG_INFINITY;
        for (index, (candidate, _)) in pool.iter().enumerate() {
            if used[index] {
                continue;
            }
            let score = remote_home_score(
                candidate,
                impressions
                    .get(&candidate.media_id)
                    .copied()
                    .unwrap_or_default(),
                diversity_boost(candidate, &artist_counts),
            );
            if score > best_score {
                best = Some(index);
                best_score = score;
            }
        }
        let Some(index) = best else {
            break;
        };
        used[index] = true;
        let candidate = pool[index].0.clone();
        let artist = candidate.artists.first().cloned().unwrap_or_default();
        *artist_counts.entry(artist).or_default() += 1;
        out.push(HomeItemResponse {
            media_id: candidate.media_id,
            title: candidate.title,
            artists: candidate.artists,
            album_id: None,
            duration_ms: candidate.duration_ms,
            source: "yt".into(),
            thumbnail_url: candidate.thumbnail_url,
            score: best_score as f32,
        });
    }
    out
}

/// Base score for remote (InnerTube) home candidates.
///
/// Affinity (0.35) and seed-strength (0.20) are not yet available for InnerTube
/// candidates, so they are folded into a constant base of 0.21. Recency (0.15)
/// is likewise absent; novelty (0.15), remote-confidence (0.10), and diversity
/// (0.05) are fully wired.
const REMOTE_BASE_SCORE: f64 = 0.35 * 0.6 + 0.20 * 0.0 + 0.15 * 0.0;

pub(crate) fn remote_home_score(
    candidate: &RemoteHomeCandidate,
    impressions: i32,
    diversity: f64,
) -> f64 {
    REMOTE_BASE_SCORE
        + 0.15 * novelty_score(impressions)
        + 0.10 * candidate.remote_confidence.clamp(0.0, 1.0)
        + 0.05 * diversity.clamp(0.0, 1.0)
}

pub(crate) fn novelty_score(impressions: i32) -> f64 {
    if impressions <= 0 {
        return 1.0;
    }
    if impressions >= 5 {
        return 0.0;
    }
    1.0 - f64::from(impressions) / 5.0
}

pub(crate) fn diversity_boost(
    candidate: &RemoteHomeCandidate,
    artist_counts: &std::collections::HashMap<String, i32>,
) -> f64 {
    let artist = candidate.artists.first().cloned().unwrap_or_default();
    let artist_count = artist_counts.get(&artist).copied().unwrap_or_default();
    let artist_score = 1.0 / f64::from(1 + artist_count);
    (artist_score + 1.0) / 2.0
}
