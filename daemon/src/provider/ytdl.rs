use super::error::ProviderResult;
use crate::provider::{ProviderTrait, SearchResult};
use crate::source::local_file::LocalFileTrack;
use crate::source::SourceKinds;
use duct::cmd;
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct YtdlProvider {
    db_conn: Connection,
}

impl YtdlProvider {
    pub fn try_new() -> ProviderResult<Self> {
        cmd!("yt-dlp", "--version").read()?;
        let conn = Connection::open("ytdl.sqlite")?;

        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS ytdl (
                id          INTEGER PRIMARY KEY,
                provider    TEXT NOT NULL,
                vid         TEXT NOT NULL,
                vid_title   TEXT,
                vid_path    TEXT NOT NULL,
            );
            ",
            (),
        )?;

        Ok(Self { db_conn: conn })
    }
}

static SEARCH_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::from_str(r"\w+\d+:(.+)$").unwrap());

impl ProviderTrait for YtdlProvider {
    fn get_name(&self) -> String {
        "YtdlProvider".to_string()
    }

    fn search(&mut self, query: &str, max_results: Option<usize>) -> SearchResult {
        let query = query.trim();
        let mut result = HashMap::new();

        let search_term;

        // Check does the query match the form: {search provider}{len}: {keyword}
        let ytdl_search_term = if let Some(cap) = SEARCH_PATTERN.captures(query) {
            search_term = cap.iter().next().unwrap().unwrap().as_str();
            query
        } else {
            search_term = query;
            match max_results {
                Some(len) => &format!("{}{}:{}", "ytsearch", len, query),
                None => query,
            }
        };

        // Search cached result and add them to return value
        let mut stmt = self.db_conn.prepare(
            "
            SELECT id, provider, vid_title, vid
            FROM ytdl
            WHERE vid_title LIKE ?1;
            ",
        )?;

        let mut rows = stmt.query((search_term,))?;

        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let provider: String = row.get(1)?;
            let title: String = row.get(2)?;
            let vid: String = row.get(3)?;

            result.insert(
                format!("cached_{}", id),
                format!("{}({}): {}", provider, vid, title),
            );
        }

        let output = cmd![
            "yt-dlp",
            "--no-playlist",
            "--print",
            "id",
            "--print",
            "fulltitle",
            ytdl_search_term,
        ]
        .read()?;

        let mut iter = output.lines();

        while let Some(vid) = iter.next() {
            let title = iter.next().unwrap();

            result.insert(vid.to_string(), title.to_string());
        }

        Ok(result)
    }

    fn get_track(&self, url: &str) -> ProviderResult<SourceKinds> {
        const OUTPUT_TEMPLATE: &str = "audio/yt-dlp/%(extractor_key)s/%(fulltitle)s.%(ext)s";

        let output = cmd![
            "yt-dlp",
            "--no-keep-video",
            "--extract-audio",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "0",
            "--print",
            "id",
            "--print",
            "extractor_key",
            "--print",
            "after_move:filepath",
            "--print",
            "fulltitle",
            "--output",
            OUTPUT_TEMPLATE,
            url,
        ]
        .read()?;

        let mut iter = output.trim().lines();

        let vid = iter.next().unwrap().to_string();
        let extractor_key = iter.next().unwrap().to_string();
        let path = iter.next().unwrap().to_string();
        let title = iter.next().unwrap().to_string();

        // Fill missing field
        self.db_conn.execute(
            "
            INSERT INTO ytdl (provider, vid_path, vid_title, vid)
            VALUE (?1, ?2, ?3, ?4);
        ",
            (extractor_key, &path, title, vid),
        )?;

        let track = LocalFileTrack::new(path);

        Ok(track.into())
    }
}
