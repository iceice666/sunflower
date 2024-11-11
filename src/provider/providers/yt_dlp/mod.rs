pub mod utils;

use crate::provider::error::ProviderResult;
use crate::provider::providers::yt_dlp::utils::run_cmd;
use crate::provider::sources::local_file::LocalFileTrack;
use crate::provider::sources::TrackObject;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;

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
pub struct DownloadOption {
    pub platform: SearchPlatform,
    pub video_id: String,
}

impl Display for DownloadOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.platform {
            SearchPlatform::Youtube => {
                write!(f, "https://www.youtube.com/watch?v={}", self.video_id)
            }
            SearchPlatform::SoundCloud => {
                write!(f, "https://api.soundcloud.com/tracks/{}", self.video_id)
            }
            SearchPlatform::BiliBili => {
                write!(f, "https://www.bilibili.com/video/{}", self.video_id)
            }
        }
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

    pub async fn search(&self, query: SearchOption) -> ProviderResult<HashMap<String, String>> {
        let query = query.to_string();
        let output = run_cmd(&[
            "yt-dlp",
            "--no-playlist",
            "--print",
            "id",
            "--print",
            "fulltitle",
            &query,
        ])?;

        let mut result = HashMap::new();

        let mut iter = output.lines();

        while let Some(vid) = iter.next() {
            let title = iter.next().unwrap();

            result.insert(vid.to_string(), title.to_string());
        }

        println!("Search output: {:#?}", result);

        Ok(result)
    }

    pub async fn download(&self, query: DownloadOption) -> ProviderResult<TrackObject> {
        // Try to find existing download first
        if let Some(track) = self.find_existing_track(&query)? {
            return Ok(track);
        }

        // Download new track
        let output = self.download_track(&query.to_string())?;

        // Update index
        self.update_index(&query, &output)?;

        Ok(Box::new(LocalFileTrack::new(output)))
    }

    const INDEX_CSV: &str = "music/yt-dlp/index.csv";
    const OUTPUT_TEMPLATE: &str = "music/yt-dlp/%(extractor_key)s/%(fulltitle)s.%(ext)s";

    fn find_existing_track(&self, query: &DownloadOption) -> ProviderResult<Option<TrackObject>> {
        let csv_content = fs::read_to_string(Self::INDEX_CSV)?;
        let csv_query = format!("{},{}", query.video_id, query.platform);

        csv_content
            .lines()
            .find(|line| line.starts_with(&csv_query))
            .and_then(|line| line.split(',').nth(3))
            .map(|path| {
                let track: TrackObject = Box::new(LocalFileTrack::new(path));
                Ok(track)
            })
            .transpose()
    }

    fn download_track(&self, url: &str) -> ProviderResult<String> {
        run_cmd(&[
            "yt-dlp",
            "--no-keep-video",
            "--extract-audio",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "0",
            "--print",
            "after_move:filepath",
            "--output",
            Self::OUTPUT_TEMPLATE,
            url,
        ])
    }

    fn update_index(&self, query: &DownloadOption, output: &str) -> ProviderResult<()> {
        let mut file = OpenOptions::new().append(true).open(Self::INDEX_CSV)?;
        writeln!(file, "{},{},{}\n", query.video_id, query.platform, output)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_youtube() -> anyhow::Result<()> {
        let dlp = YtDlp::try_new()?;

        let query = SearchOption {
            platform: SearchPlatform::Youtube,
            len: 5,
            keyword: "maimai world's end loneliness".to_string(),
        };

        let result = dlp.search(query).await?;

        let vid = result.keys().next().unwrap();
        let query = DownloadOption {
            platform: SearchPlatform::Youtube,
            video_id: vid.to_string(),
        };
        dlp.download(query).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_soundcloud() -> anyhow::Result<()> {
        let dlp = YtDlp::try_new()?;

        let query = SearchOption {
            platform: SearchPlatform::SoundCloud,
            len: 5,
            keyword: "Viyella's".to_string(),
        };
        let result = dlp.search(query).await?;

        let vid = result.keys().next().unwrap();
        let query = DownloadOption {
            platform: SearchPlatform::SoundCloud,
            video_id: vid.to_string(),
        };
        dlp.download(query).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_bilibili() -> anyhow::Result<()> {
        let dlp = YtDlp::try_new()?;

        let query = SearchOption {
            platform: SearchPlatform::BiliBili,
            len: 1,
            keyword: "ak+q".to_string(),
        };

        let result = dlp.search(query).await?;

        let vid = result.keys().next().unwrap();
        let query = DownloadOption {
            platform: SearchPlatform::BiliBili,
            video_id: vid.to_string(),
        };
        dlp.download(query).await?;
        Ok(())
    }
}
