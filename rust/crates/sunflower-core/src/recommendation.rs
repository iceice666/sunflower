use std::collections::{HashMap, HashSet};

use crate::{LocalStatsSnapshot, RecommendationCandidate, RecommendationSource, TrackStats};

pub const DEFAULT_RECOMMENDATION_LIMIT: usize = 20;

#[derive(Clone, Copy, Debug)]
pub struct LocalRankerWeights {
    pub affinity: f32,
    pub availability: f32,
    pub completion: f32,
    pub novelty: f32,
    pub remote: f32,
    pub skip_penalty: f32,
    /// Score penalty applied to tracks that appear in the recent-play window to
    /// suppress immediate repeats. Named here so it is tunable alongside the
    /// other weights.
    pub recent_repeat_penalty: f32,
}

impl Default for LocalRankerWeights {
    fn default() -> Self {
        Self {
            affinity: 0.30,
            availability: 0.25,
            completion: 0.15,
            novelty: 0.15,
            remote: 0.10,
            skip_penalty: 0.20,
            recent_repeat_penalty: 0.35,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalRecommendationEngine {
    weights: LocalRankerWeights,
}

impl LocalRecommendationEngine {
    pub fn new(weights: LocalRankerWeights) -> Self {
        Self { weights }
    }

    pub fn rank(
        &self,
        candidates: &[RecommendationCandidate],
        stats: &LocalStatsSnapshot,
        limit: usize,
    ) -> Vec<RecommendationCandidate> {
        let by_media: HashMap<_, _> = stats
            .tracks
            .iter()
            .map(|track| (track.media_id.clone(), track))
            .collect();
        let recent: HashSet<_> = stats.recent_media_ids.iter().cloned().collect();

        let mut scored: Vec<_> = candidates
            .iter()
            .cloned()
            .map(|candidate| {
                let stat = by_media.get(&candidate.media_id).copied();
                let mut score = self.score(&candidate, stat);
                if recent.contains(&candidate.media_id) {
                    score -= self.weights.recent_repeat_penalty;
                }
                (score, candidate)
            })
            .collect();

        scored.sort_by(|(a_score, a), (b_score, b)| {
            b_score
                .total_cmp(a_score)
                .then_with(|| a.media_id.0.cmp(&b.media_id.0))
        });

        scored
            .into_iter()
            .take(limit)
            .map(|(_, candidate)| candidate)
            .collect()
    }

    fn score(&self, candidate: &RecommendationCandidate, stats: Option<&TrackStats>) -> f32 {
        let affinity = stats.map(track_affinity).unwrap_or(0.15);
        let availability = stats.map(availability_score).unwrap_or(0.0);
        let completion = stats.map(completion_score).unwrap_or(0.0);
        let novelty = stats.map(novelty_score).unwrap_or(0.5);
        let skip_penalty = stats.map(skip_penalty).unwrap_or(0.0);
        let remote = if candidate.source == RecommendationSource::Remote
            || candidate.source == RecommendationSource::Mixed
        {
            candidate.remote_score.clamp(0.0, 1.0)
        } else {
            0.0
        };

        self.weights.affinity * affinity
            + self.weights.availability * availability
            + self.weights.completion * completion
            + self.weights.novelty * novelty
            + self.weights.remote * remote
            - self.weights.skip_penalty * skip_penalty
    }
}

impl Default for LocalRecommendationEngine {
    fn default() -> Self {
        Self::new(LocalRankerWeights::default())
    }
}

fn track_affinity(stats: &TrackStats) -> f32 {
    let like = if stats.liked { 0.55 } else { 0.0 };
    let plays = ((stats.play_count as f32).ln_1p() / 4.0).clamp(0.0, 0.35);
    (like + plays).clamp(0.0, 1.0)
}

fn availability_score(stats: &TrackStats) -> f32 {
    match (stats.downloaded, stats.local_available) {
        (true, _) => 1.0,
        (_, true) => 0.85,
        _ => 0.0,
    }
}

fn completion_score(stats: &TrackStats) -> f32 {
    let total = stats.completion_count + stats.skip_count;
    if total == 0 {
        return 0.35;
    }
    (stats.completion_count as f32 / total as f32).clamp(0.0, 1.0)
}

fn novelty_score(stats: &TrackStats) -> f32 {
    (1.0 / (1.0 + stats.impression_count as f32)).clamp(0.0, 1.0)
}

fn skip_penalty(stats: &TrackStats) -> f32 {
    let total = stats.completion_count + stats.skip_count;
    if total == 0 {
        return 0.0;
    }
    (stats.skip_count as f32 / total as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MediaId, TrackStats};
    use chrono::Utc;

    fn candidate(id: &str, remote_score: f32) -> RecommendationCandidate {
        RecommendationCandidate {
            media_id: MediaId::new(id),
            title: id.into(),
            artists: vec![],
            album_id: None,
            duration_ms: 0,
            source: RecommendationSource::Mixed,
            remote_score,
            reason: None,
        }
    }

    #[test]
    fn local_ranker_prefers_available_liked_tracks() {
        let engine = LocalRecommendationEngine::default();
        let stats = LocalStatsSnapshot {
            generated_at: Utc::now(),
            tracks: vec![
                TrackStats {
                    media_id: MediaId::new("yt:remote-only"),
                    play_count: 1,
                    skip_count: 4,
                    completion_count: 0,
                    impression_count: 8,
                    liked: false,
                    downloaded: false,
                    local_available: false,
                    last_played_at: None,
                },
                TrackStats {
                    media_id: MediaId::new("local:fav"),
                    play_count: 8,
                    skip_count: 0,
                    completion_count: 7,
                    impression_count: 1,
                    liked: true,
                    downloaded: true,
                    local_available: true,
                    last_played_at: None,
                },
            ],
            recent_media_ids: vec![],
            recent_artist_names: vec![],
        };

        let ranked = engine.rank(
            &[
                candidate("yt:remote-only", 1.0),
                candidate("local:fav", 0.2),
            ],
            &stats,
            10,
        );

        assert_eq!(ranked[0].media_id, MediaId::new("local:fav"));
    }

    #[test]
    fn local_ranker_penalizes_recent_repeats() {
        let engine = LocalRecommendationEngine::default();
        let stats = LocalStatsSnapshot {
            generated_at: Utc::now(),
            tracks: vec![
                TrackStats {
                    media_id: MediaId::new("local:recent"),
                    play_count: 10,
                    liked: true,
                    downloaded: true,
                    local_available: true,
                    ..TrackStats::default()
                },
                TrackStats {
                    media_id: MediaId::new("local:next"),
                    play_count: 6,
                    liked: true,
                    downloaded: true,
                    local_available: true,
                    ..TrackStats::default()
                },
            ],
            recent_media_ids: vec![MediaId::new("local:recent")],
            recent_artist_names: vec![],
        };

        let ranked = engine.rank(
            &[candidate("local:recent", 0.1), candidate("local:next", 0.1)],
            &stats,
            10,
        );

        assert_eq!(ranked[0].media_id, MediaId::new("local:next"));
    }
}
