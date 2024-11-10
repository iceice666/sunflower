pub mod utils;

use tracing::debug;

use crate::provider::error::ProviderResult;
use crate::provider::providers::yt_dlp::utils::run_cmd;
use crate::provider::sources::local_file::LocalFileTrack;
use crate::provider::sources::TrackObject;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::process::Command;

#[derive(Debug)]
pub enum SearchPlatform {
    Youtube,
    SoundCloud,
    BiliBili,
}

impl Display for SearchPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Self::Youtube => "ytsearch",
            Self::SoundCloud => "scsearch",
            Self::BiliBili => "bilisearch",
        };

        write!(f, "{}", str)
    }
}

#[derive(Debug)]
pub struct SearchOption {
    pub platform: SearchPlatform,
    pub len: u8,
    pub keyword: String,
}

impl Display for SearchOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}:{}", self.platform, self.len, self.keyword)
    }
}

#[derive(Debug)]
pub struct YtDlp;

impl YtDlp {
    pub fn try_new() -> ProviderResult<Self> {
        run_cmd(&["yt-dlp", "--version"])?;
        run_cmd(&["ffmpeg", "-version"])?;
        run_cmd(&["ffprobe", "-version"])?;
        Ok(YtDlp)
    }

    async fn search(&self, query: SearchOption) -> ProviderResult<HashMap<String, String>> {
        let query = query.to_string();
        let output = run_cmd(&[
            "yt-dlp",
            "--no-playlist",
            format!("\"{}\"", query).as_ref(),
            "--print id",
            "--print fulltitle",
        ])?;

        let mut result = HashMap::new();

        let mut iter = output.lines();

        while let Some(vid) = iter.next() {
            let title = iter.next().unwrap();

            result.insert(vid.to_string(), title.to_string());
        }

        Ok(result)
    }

    async fn get_track(&self, url: &str) -> ProviderResult<TrackObject> {
        let output = run_cmd(&[
            "yt-dlp",
            "--no-keep-video",
            "--extract-audio",
            "--audio-format mp3",
            "--audio-quality 0",
            "--print after_move:filepath",
            "--output-format music/yt-dlp/%(extractor_key)/%(id)+%(fulltitle).%(ext)",
            url,
        ])?;

        Ok(Box::new(LocalFileTrack::new(output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search() -> anyhow::Result<()> {
        let query = SearchOption {
            platform: SearchPlatform::Youtube,
            len: 10,
            keyword: "maimai world's end loneliness".to_string(),
        };

        let dlp = YtDlp::try_new()?;
        dlp.search(query).await?;

        println!("Search result: {:?}", dlp);

        Ok(())
    }
}
