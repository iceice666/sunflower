use crate::{MediaId, NextDecision, QueueItem, QueueSession, RecommendationSource, ResolvedStream};
use thiserror::Error;

pub const DEFAULT_LOOKAHEAD_COUNT: usize = 8;
pub const RADIO_MAX_PAGES: usize = 10;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("position {position} is outside queue length {len}")]
    PositionOutOfRange { position: usize, len: usize },
}

pub fn next_window(
    session: &QueueSession,
    position: usize,
    current: ResolvedStream,
    lookahead_count: usize,
    recommender_source: RecommendationSource,
) -> Result<NextDecision, QueueError> {
    if position >= session.items.len() {
        return Err(QueueError::PositionOutOfRange {
            position,
            len: session.items.len(),
        });
    }

    let end = (position + 1 + lookahead_count).min(session.items.len());
    Ok(NextDecision {
        queue_id: session.id,
        position,
        current: Some(current),
        lookahead: session.items[position + 1..end].to_vec(),
        continuation: None,
        automix: vec![],
        has_more: end < session.items.len(),
        queue_version: session.version,
        recommender_source,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LikedSong {
    pub media_id: MediaId,
    pub title: String,
    pub duration_ms: i32,
}

/// Builds the "shuffle_liked" materialized queue.
///
/// This preserves the established `shuffle_liked` contract: all input songs
/// survive, the input slice is not mutated, and empty input returns an empty
/// queue. The exact shuffled order is intentionally not a wire contract.
pub fn build_automix(liked: &[LikedSong], seed: u64) -> Vec<QueueItem> {
    let mut shuffled = liked.to_vec();
    let mut rng = SplitMix64::new(seed);
    for i in (1..shuffled.len()).rev() {
        let j = rng.next_index(i + 1);
        shuffled.swap(i, j);
    }

    shuffled
        .into_iter()
        .map(|song| QueueItem {
            media_id: song.media_id,
            title: song.title,
            artists: vec![],
            duration_ms: song.duration_ms,
        })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RadioPage {
    pub related: Vec<QueueItem>,
    pub continuation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedRadio {
    pub items: Vec<QueueItem>,
    pub continuation: Option<String>,
}

/// Expands already-normalized radio pages into a materialized queue.
///
/// The page source/parser is deliberately outside this function. The behavior
/// matches the established radio expansion contract after parsing: skip empty
/// media ids, dedupe by media id, continue until the floor is reached,
/// continuation ends, a hard page cap is hit, or a continuation page adds no new
/// items.
pub fn expand_radio_pages<I>(pages: I, min_items: usize) -> ExpandedRadio
where
    I: IntoIterator<Item = RadioPage>,
{
    let mut pages = pages.into_iter();
    let mut items = Vec::with_capacity(min_items);
    let mut seen = std::collections::HashSet::new();

    let mut add = |items_out: &mut Vec<QueueItem>, page_items: Vec<QueueItem>| {
        let before = items_out.len();
        for item in page_items {
            if item.media_id.0.is_empty() || !seen.insert(item.media_id.0.clone()) {
                continue;
            }
            items_out.push(item);
        }
        items_out.len() - before
    };

    let Some(first) = pages.next() else {
        return ExpandedRadio {
            items,
            continuation: None,
        };
    };
    let mut continuation = first.continuation;
    add(&mut items, first.related);

    let mut followed_pages = 0;
    while items.len() < min_items && continuation.is_some() && followed_pages < RADIO_MAX_PAGES {
        let Some(page) = pages.next() else {
            break;
        };
        followed_pages += 1;
        let added = add(&mut items, page.related);
        continuation = page.continuation;
        if added == 0 {
            break;
        }
    }

    ExpandedRadio {
        items,
        continuation,
    }
}

#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn next_index(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            return 0;
        }
        (self.next_u64() % upper_exclusive as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn next_window_returns_current_and_capped_lookahead() {
        let session = QueueSession {
            id: Uuid::new_v4(),
            seed_kind: "local_radio".into(),
            seed_id: "local".into(),
            title: "Local".into(),
            version: 7,
            items: (0..12)
                .map(|i| QueueItem {
                    media_id: MediaId::new(format!("local:{i}")),
                    title: format!("Track {i}"),
                    artists: vec![],
                    duration_ms: 1000,
                })
                .collect(),
        };
        let current = ResolvedStream {
            media_id: MediaId::new("local:3"),
            source: "local".into(),
            stream_url: "file:///tmp/3.flac".into(),
            stream_expires_at: None,
            mime_type: None,
            content_length: None,
            loudness_db: None,
            playback_tracking_url: None,
            metadata: json!({ "resolved_at": Utc::now() }),
        };

        let decision = next_window(&session, 3, current, 4, RecommendationSource::Local).unwrap();

        assert_eq!(decision.position, 3);
        assert_eq!(decision.lookahead.len(), 4);
        assert_eq!(decision.lookahead[0].media_id, MediaId::new("local:4"));
        assert!(decision.has_more);
        assert_eq!(decision.queue_version, 7);
    }

    #[test]
    fn build_automix_preserves_all_items() {
        let liked: Vec<_> = (0..12)
            .map(|i| LikedSong {
                media_id: MediaId::new(format!("local:{}", (b'a' + i) as char)),
                title: "t".into(),
                duration_ms: 0,
            })
            .collect();

        let got = build_automix(&liked, 1);
        assert_eq!(got.len(), liked.len());
        for song in &liked {
            assert!(
                got.iter().any(|item| item.media_id == song.media_id),
                "media_id {:?} missing after shuffle",
                song.media_id
            );
        }
    }

    #[test]
    fn build_automix_does_not_mutate_input() {
        let liked = vec![
            LikedSong {
                media_id: MediaId::new("local:a"),
                title: String::new(),
                duration_ms: 0,
            },
            LikedSong {
                media_id: MediaId::new("local:b"),
                title: String::new(),
                duration_ms: 0,
            },
            LikedSong {
                media_id: MediaId::new("local:c"),
                title: String::new(),
                duration_ms: 0,
            },
        ];
        let original = liked.clone();

        let _ = build_automix(&liked, 99);

        assert_eq!(liked, original);
    }

    #[test]
    fn build_automix_empty() {
        assert!(build_automix(&[], 1).is_empty());
    }

    #[test]
    fn expand_radio_collects_across_continuations() {
        let pages = vec![
            radio_page(&["yt:a", "yt:b", "yt:c"], Some("cont1")),
            radio_page(&["yt:d", "yt:e", "yt:f"], Some("cont2")),
            radio_page(&["yt:g", "yt:h", "yt:i", "yt:j", "yt:k"], None),
        ];

        let expanded = expand_radio_pages(pages, 10);

        assert!(expanded.items.len() >= 10);
        assert_eq!(expanded.items[0].media_id, MediaId::new("yt:a"));
    }

    #[test]
    fn expand_radio_deduplicates() {
        let expanded = expand_radio_pages(
            vec![radio_page(&["yt:a", "yt:b", "yt:a", "yt:b"], None)],
            10,
        );

        assert_eq!(expanded.items.len(), 2);
        assert_eq!(expanded.items[0].media_id, MediaId::new("yt:a"));
        assert_eq!(expanded.items[1].media_id, MediaId::new("yt:b"));
    }

    #[test]
    fn expand_radio_stops_when_no_progress() {
        let pages = vec![
            radio_page(&["yt:a", "yt:b"], Some("cont")),
            radio_page(&["yt:a", "yt:b"], Some("cont")),
            radio_page(&["yt:c"], None),
        ];

        let expanded = expand_radio_pages(pages, 10);

        assert_eq!(expanded.items.len(), 2);
    }

    #[test]
    fn expand_radio_skips_empty_media_ids() {
        let expanded = expand_radio_pages(vec![radio_page(&["", "yt:a"], None)], 10);

        assert_eq!(expanded.items.len(), 1);
        assert_eq!(expanded.items[0].media_id, MediaId::new("yt:a"));
    }

    fn radio_page(ids: &[&str], continuation: Option<&str>) -> RadioPage {
        RadioPage {
            related: ids
                .iter()
                .map(|id| QueueItem {
                    media_id: MediaId::new(*id),
                    title: format!("Song {id}"),
                    artists: vec![],
                    duration_ms: 0,
                })
                .collect(),
            continuation: continuation.map(str::to_string),
        }
    }
}
