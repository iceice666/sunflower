use super::error::{ProviderError, ProviderResult};
use crate::provider::{ProviderTrait, SearchResult};
use crate::source::local_file::LocalFileTrack;
use crate::source::SourceKinds;
use duct::cmd;
use log::{debug, error, info, warn};
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::LazyLock;
use tracing::instrument;

const OUTPUT_TEMPLATE: &str = "audio/yt-dlp/%(extractor_key)s/%(fulltitle)s.%(ext)s";
const DB_PATH: &str = "audio/yt-dlp/ytdl.sqlite";

#[derive(Debug)]
pub struct YtdlProvider {
    db_conn: Connection,
    binary_path: String,
    extra_args: Vec<String>,
}

impl PartialEq<Self> for YtdlProvider {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for YtdlProvider {}

impl YtdlProvider {
    pub fn try_new(
        binary_path: impl Into<String>,
        extra_args: Vec<String>,
    ) -> ProviderResult<Self> {
        info!("Initializing YtdlProvider");
        let binary_path = binary_path.into();

        match cmd!(&binary_path, "--version").read() {
            Ok(version) => debug!("yt-dlp version: {}", version),
            Err(e) => {
                error!("Failed to verify yt-dlp installation: {}", e);
                return Err(e.into());
            }
        }

        let conn = match Connection::open(DB_PATH) {
            Ok(conn) => {
                debug!("Successfully opened database connection");
                conn
            }
            Err(e) => {
                error!("Failed to open database connection: {}", e);
                return Err(e.into());
            }
        };

        // Enable foreign keys and WAL mode for better performance
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS ytdl (
                id          INTEGER PRIMARY KEY,
                vid_url     TEXT NOT NULL,
                vid_title   TEXT NOT NULL,
                vid_path    TEXT NOT NULL,
                created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_vid_url ON ytdl(vid_url);
            CREATE INDEX IF NOT EXISTS idx_vid_title ON ytdl(vid_title);
        ",
        )?;

        Ok(Self {
            db_conn: conn,
            binary_path,
            extra_args,
        })
    }

    #[instrument(skip(self))]
    fn search_cache(&self, search_term: &str) -> ProviderResult<HashMap<String, String>> {
        let mut result = HashMap::new();
        let mut stmt = self.db_conn.prepare(
            "
            SELECT id, vid_title, vid_url
            FROM ytdl
            WHERE vid_title LIKE ?1
            ORDER BY created_at DESC
            LIMIT 50;
        ",
        )?;

        let search_pattern = format!("%{}%", search_term);
        let rows = stmt.query_map([search_pattern], |row| {
            Ok((
                row.get::<_, usize>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row_result in rows {
            match row_result {
                Ok((id, title, vid_url)) => {
                    result.insert(format!("cached_{}", id), format!("{}: {}", vid_url, title));
                }
                Err(e) => warn!("Error processing cached result: {}", e),
            }
        }

        debug!("Found {} cached results", result.len());
        Ok(result)
    }
}

static SEARCH_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::from_str(r"\w+\d+:(.+)$").unwrap());

static CACHED_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::from_str(r"cached_(\d+)$").unwrap());

impl ProviderTrait for YtdlProvider {
    fn get_name(&self) -> String {
        "YtdlProvider".to_string()
    }

    #[instrument(skip(self))]
    fn search(&mut self, query: &str, max_results: Option<usize>) -> SearchResult {
        let query = query.trim();
        let mut result = HashMap::new();

        let cache_search_term;

        // Check does the query match the form: {search provider}{len}: {keyword}
        let ytdl_search_term = if let Some(cap) = SEARCH_PATTERN.captures(query) {
            cache_search_term = cap.iter().next().unwrap().unwrap().as_str();
            query
        } else {
            cache_search_term = query;
            match max_results {
                Some(len) => &format!("{}{}:{}", "ytsearch", len, query),
                None => query,
            }
        };

        std::mem::swap(&mut result, &mut self.search_cache(cache_search_term)?);

        if max_results == Some(0) {
            info!("Requested max search result is 0, return cached result");
            return Ok(result);
        }

        debug!("Searching term:({}) with yt-dlp", ytdl_search_term);
        #[rustfmt::skip]
        let mut args = vec![
            "--no-playlist",
            "--print", "id",
            "--print", "fulltitle",
            ytdl_search_term,
        ];

        args.extend(self.extra_args.iter().map(String::as_str));

        let output = cmd(&self.binary_path, args).read();

        match output {
            Ok(output) => {
                let mut count = 0;
                let mut iter = output.lines();
                while let Some(vid) = iter.next() {
                    if let Some(title) = iter.next() {
                        result.insert(vid.to_string(), title.to_string());
                        count += 1;
                    }
                }
                debug!("Found {} new results from yt-dlp", count);
            }
            Err(e) => {
                error!("yt-dlp search failed: {}", e);
                return Err(e.into());
            }
        }

        Ok(result)
    }

    #[instrument(skip(self))]
    fn get_track(&self, uri: &str) -> ProviderResult<SourceKinds> {
        info!("Fetching track...");

        debug!("Finding cache in database...");
        let path = if let Some(cap) = CACHED_PATTERN.captures(uri) {
            // Captured requested id
            self.db_conn.query_row(
                "SELECT vid_path FROM ytdl WHERE id = ?1",
                (cap.get(1).map_or("", |m| m.as_str()),),
                |row| row.get::<_, String>(1),
            )
        } else {
            // Check if track already exists in the cache
            self.db_conn.query_row(
                "SELECT vid_path FROM ytdl WHERE vid_url = ?1",
                (uri,),
                |row| row.get::<_, String>(1),
            )
        }?;

        debug!("Found cached track at: {}", path);
        if PathBuf::from(&path).exists() {
            return Ok(LocalFileTrack::new(path).into());
        }
        warn!("Cached file not found, re-downloading...");

        debug!("Downloading track with yt-dlp...");
        #[rustfmt::skip]
        let mut args = vec![
            "--no-keep-video",
            "--extract-audio",
            "--audio-format", "mp3",
            "--audio-quality", "0",
            "--print", "webpage_url",
            "--print", "fulltitle",
            "--print", "after_move:filepath",
            "--output", OUTPUT_TEMPLATE,
            uri,
        ];

        args.extend(self.extra_args.iter().map(String::as_str));

        let output = cmd(&self.binary_path, args).read()?;
        let mut iter = output.trim().lines();
        let (url, title, path) = match (iter.next(), iter.next(), iter.next()) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => {
                error!("Invalid output format from yt-dlp");
                return Err(ProviderError::Other("Invalid yt-dlp output format".into()));
            }
        };

        debug!("Downloaded track to: {}", path);

        // Update database
        match self.db_conn.execute(
            "INSERT OR REPLACE INTO ytdl (vid_path, vid_title, vid_url)
             VALUES (?1, ?2, ?3)",
            (path, title, url),
        ) {
            Ok(_) => debug!("Successfully updated database"),
            Err(e) => error!("Failed to update database: {}", e),
        }

        Ok(LocalFileTrack::new(path).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_logger;

    #[test]
    fn test() -> anyhow::Result<()> {
        init_logger();

        let mut provider = YtdlProvider::try_new("ytdl", vec![])?;

        let result = provider.search("never gonna give you up", Some(5))?;
        println!("{:?}", result);
        let _ = provider.get_track("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let _ = provider.search("never gonna give you up", Some(0))?;

        Ok(())
    }
}
