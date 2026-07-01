use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use image::ImageFormat;
use sha1::{Digest, Sha1};
use sunflower_core::{JobResponse, legacy_rfc3339_nano};
use sunflower_storage_postgres::{PostgresStore, ScannedLocalSong};
use uuid::Uuid;

const STATUS_PENDING: &str = "pending";
const STATUS_RUNNING: &str = "running";
const STATUS_COMPLETED: &str = "completed";
const STATUS_FAILED: &str = "failed";

#[derive(Clone)]
struct JobRecord {
    id: String,
    status: String,
    processed_files: i32,
    error: String,
    created_at: SystemTime,
    updated_at: SystemTime,
}

impl JobRecord {
    fn response(&self) -> JobResponse {
        JobResponse {
            id: self.id.clone(),
            status: self.status.clone(),
            processed_files: self.processed_files,
            error: self.error.clone(),
            created_at: rfc3339_nano(self.created_at),
            updated_at: rfc3339_nano(self.updated_at),
        }
    }
}

#[derive(Default)]
pub struct JobRegistry {
    jobs: Mutex<HashMap<String, JobRecord>>,
}

impl JobRegistry {
    fn lock_jobs(&self) -> MutexGuard<'_, HashMap<String, JobRecord>> {
        self.jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn create(&self) -> JobResponse {
        let now = SystemTime::now();
        let job = JobRecord {
            id: Uuid::new_v4().to_string(),
            status: STATUS_PENDING.to_string(),
            processed_files: 0,
            error: String::new(),
            created_at: now,
            updated_at: now,
        };
        let response = job.response();
        self.lock_jobs().insert(job.id.clone(), job);
        response
    }

    pub fn get(&self, id: &str) -> Option<JobResponse> {
        self.lock_jobs().get(id).map(JobRecord::response)
    }

    pub fn list_recent(&self, limit: usize) -> Vec<JobResponse> {
        let mut jobs = self.lock_jobs().values().cloned().collect::<Vec<_>>();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        if limit > 0 && jobs.len() > limit {
            jobs.truncate(limit);
        }
        jobs.into_iter().map(|job| job.response()).collect()
    }

    fn update(&self, id: &str, update: impl FnOnce(&mut JobRecord)) {
        let mut jobs = self.lock_jobs();
        let Some(job) = jobs.get_mut(id) else {
            return;
        };
        update(job);
        job.updated_at = SystemTime::now();
    }
}

pub async fn run_scan_job(
    registry: std::sync::Arc<JobRegistry>,
    store: PostgresStore,
    job_id: String,
    roots: Vec<String>,
    data_dir: String,
) {
    registry.update(&job_id, |job| job.status = STATUS_RUNNING.to_string());
    let result = scan_roots(registry.clone(), store, &job_id, roots, data_dir).await;
    match result {
        Ok(processed) => registry.update(&job_id, |job| {
            job.status = STATUS_COMPLETED.to_string();
            job.processed_files = processed;
        }),
        Err(err) => registry.update(&job_id, |job| {
            job.status = STATUS_FAILED.to_string();
            job.error = err;
        }),
    }
}

async fn scan_roots(
    registry: std::sync::Arc<JobRegistry>,
    store: PostgresStore,
    job_id: &str,
    roots: Vec<String>,
    data_dir: String,
) -> Result<i32, String> {
    let mut processed = 0;
    for root in roots {
        let root = absolute_path(Path::new(&root)).map_err(|err| err.to_string())?;
        let files = audio_files_under(&root).map_err(|err| err.to_string())?;
        for path in files {
            if let Ok(extracted) = extract_tags(&path) {
                let song = extracted.song;
                let _ = store.upsert_scanned_local_song(&song).await;
                if let Some(cover) = extracted.cover_art {
                    let _ = save_cover_art(&cover, &song.album_media_id, &data_dir);
                }
            }
            processed += 1;
            registry.update(job_id, |job| job.processed_files = processed);
        }
    }
    Ok(processed)
}

fn audio_files_under(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut out = Vec::new();
    collect_audio_files(root, &mut out)?;
    Ok(out)
}

fn collect_audio_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            collect_audio_files(&entry.path(), out)?;
        }
    } else if is_audio_file(path) {
        out.push(absolute_path(path)?);
    }
    Ok(())
}

fn is_audio_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mp3" | "flac" | "m4a" | "ogg" | "opus")
    )
}

fn absolute_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn local_media_id(input: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    format!("local:{:x}", digest)[..22].to_string()
}

struct ExtractedTrack {
    song: ScannedLocalSong,
    cover_art: Option<Vec<u8>>,
}

fn extract_tags(path: &Path) -> Result<ExtractedTrack, String> {
    let path = absolute_path(path).map_err(|err| err.to_string())?;
    let local_path = path.to_string_lossy().to_string();
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if extension != "mp3" {
        // Keep scan coverage aligned with the accepted audio extensions while
        // richer container-specific tag parsers are added.
        return Ok(fallback_track_from_path(&path));
    }

    let bytes = fs::read(&path).map_err(|err| err.to_string())?;
    let tags = parse_id3v2_text_frames(&bytes)?;
    let mut title = tags.title.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .unwrap_or("Untitled")
            .to_string()
    });
    if title.is_empty() {
        title = "Untitled".to_string();
    }
    let artist = tags.artist.unwrap_or_default();
    let album = tags.album.unwrap_or_default();
    let album_media_id = local_media_id(&format!(
        "l:{}|{}",
        album.to_ascii_lowercase(),
        artist.to_ascii_lowercase()
    ));
    Ok(ExtractedTrack {
        song: ScannedLocalSong {
            media_id: local_media_id(&local_path),
            title,
            artist_media_id: local_media_id(&format!("a:{}", artist.to_ascii_lowercase())),
            album_media_id,
            artist,
            album,
            year: tags.year,
            local_path,
        },
        cover_art: tags.cover_art,
    })
}

fn fallback_track_from_path(path: &Path) -> ExtractedTrack {
    let local_path = path.to_string_lossy().to_string();
    let mut title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Untitled")
        .to_string();
    if title.is_empty() {
        title = "Untitled".to_string();
    }
    ExtractedTrack {
        song: ScannedLocalSong {
            media_id: local_media_id(&local_path),
            title,
            artist_media_id: local_media_id("a:"),
            album_media_id: local_media_id("l:|"),
            artist: String::new(),
            album: String::new(),
            year: None,
            local_path,
        },
        cover_art: None,
    }
}

fn save_cover_art(bytes: &[u8], album_media_id: &str, data_dir: &str) -> Result<(), String> {
    if bytes.is_empty() {
        return Ok(());
    }
    let image = image::load_from_memory(bytes).map_err(|err| format!("decode cover art: {err}"))?;
    let dir = Path::new(data_dir).join("art").join(album_media_id);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    for size in [256, 512, 1024] {
        let resized = image.thumbnail(size, size);
        let mut out = Cursor::new(Vec::new());
        resized
            .write_to(&mut out, ImageFormat::Jpeg)
            .map_err(|err| format!("save {size}px: {err}"))?;
        fs::write(dir.join(format!("{size}.jpg")), out.into_inner())
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[derive(Default)]
struct Id3TextFrames {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    year: Option<i32>,
    track: Option<i32>,
    cover_art: Option<Vec<u8>>,
}

fn parse_id3v2_text_frames(bytes: &[u8]) -> Result<Id3TextFrames, String> {
    if bytes.len() < 10 || &bytes[..3] != b"ID3" {
        return Err("missing id3v2 tag".to_string());
    }
    let version = bytes[3];
    if version != 3 && version != 4 {
        return Err("unsupported id3v2 version".to_string());
    }
    let tag_size =
        syncsafe_u32(&bytes[6..10]).ok_or_else(|| "invalid id3 size".to_string())? as usize;
    if bytes.len() < 10 + tag_size {
        return Err("truncated id3 tag".to_string());
    }
    let mut frames = Id3TextFrames::default();
    let mut offset = 10;
    let end = 10 + tag_size;
    while offset + 10 <= end {
        let id = &bytes[offset..offset + 4];
        if id.iter().all(|byte| *byte == 0) {
            break;
        }
        if !id.iter().all(|byte| byte.is_ascii_alphanumeric()) {
            break;
        }
        let size = if version == 4 {
            syncsafe_u32(&bytes[offset + 4..offset + 8])
        } else {
            Some(u32::from_be_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]))
        }
        .ok_or_else(|| "invalid id3 frame size".to_string())? as usize;
        offset += 10;
        if size == 0 || offset + size > end {
            break;
        }
        let payload = &bytes[offset..offset + size];
        match id {
            b"APIC" if frames.cover_art.is_none() => {
                frames.cover_art = parse_apic_frame(payload);
            }
            b"TIT2" | b"TPE1" | b"TALB" | b"TRCK" | b"TYER" | b"TDRC" => {
                if let Ok(text) = decode_text_frame(payload) {
                    match id {
                        b"TIT2" => frames.title = Some(text),
                        b"TPE1" => frames.artist = Some(text),
                        b"TALB" => frames.album = Some(text),
                        b"TRCK" => frames.track = first_int(&text),
                        b"TYER" | b"TDRC" => frames.year = first_int(&text),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        offset += size;
    }
    Ok(frames)
}

fn parse_apic_frame(payload: &[u8]) -> Option<Vec<u8>> {
    let (&encoding, rest) = payload.split_first()?;
    let mime_end = rest.iter().position(|byte| *byte == 0)?;
    let mut cursor = mime_end + 1;
    if cursor >= rest.len() {
        return None;
    }
    cursor += 1; // picture type
    if cursor >= rest.len() {
        return None;
    }

    let description_end = match encoding {
        1 | 2 => rest[cursor..]
            .windows(2)
            .position(|pair| pair == [0, 0])
            .map(|position| cursor + position + 2)?,
        _ => rest[cursor..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|position| cursor + position + 1)?,
    };
    (description_end < rest.len()).then(|| rest[description_end..].to_vec())
}

fn syncsafe_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.len() != 4 || bytes.iter().any(|byte| byte & 0x80 != 0) {
        return None;
    }
    Some(
        ((bytes[0] as u32) << 21)
            | ((bytes[1] as u32) << 14)
            | ((bytes[2] as u32) << 7)
            | bytes[3] as u32,
    )
}

fn decode_text_frame(payload: &[u8]) -> Result<String, String> {
    let Some((&encoding, data)) = payload.split_first() else {
        return Ok(String::new());
    };
    let text = match encoding {
        0 | 3 => String::from_utf8_lossy(data).into_owned(),
        1 | 2 => decode_utf16(data),
        _ => String::new(),
    };
    Ok(text.trim_matches(char::from(0)).trim().to_string())
}

fn decode_utf16(data: &[u8]) -> String {
    let (little_endian, body) = match data {
        [0xff, 0xfe, rest @ ..] => (true, rest),
        [0xfe, 0xff, rest @ ..] => (false, rest),
        _ => (false, data),
    };
    let units = body
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
}

fn first_int(text: &str) -> Option<i32> {
    text.split(|ch: char| !ch.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

fn rfc3339_nano(time: SystemTime) -> String {
    let time: DateTime<Utc> = time.into();
    legacy_rfc3339_nano(time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_media_id_matches_legacy_shape() {
        assert_eq!(local_media_id("/music/Track.mp3").len(), 22);
        assert!(local_media_id("/music/Track.mp3").starts_with("local:"));
        assert_eq!(
            local_media_id("/music/Track.mp3"),
            local_media_id("/music/Track.mp3")
        );
    }

    #[test]
    fn audio_file_filter_matches_go_extensions() {
        assert!(is_audio_file(Path::new("a.MP3")));
        assert!(is_audio_file(Path::new("a.flac")));
        assert!(is_audio_file(Path::new("a.m4a")));
        assert!(is_audio_file(Path::new("a.ogg")));
        assert!(is_audio_file(Path::new("a.opus")));
        assert!(!is_audio_file(Path::new("a.wav")));
    }

    #[test]
    fn job_response_omits_empty_error_like_go() {
        let registry = JobRegistry::default();
        let job = registry.create();
        let value = serde_json::to_value(job).unwrap();
        assert!(value.get("error").is_none());
        assert_eq!(value["status"], STATUS_PENDING);
        assert_eq!(value["processed_files"], 0);
    }

    #[test]
    fn job_registry_recovers_from_poisoned_lock() {
        let registry = JobRegistry::default();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = registry.jobs.lock().unwrap();
            panic!("poison job registry");
        }));
        assert!(poisoned.is_err());

        let job = registry.create();

        assert_eq!(registry.get(&job.id).unwrap().status, STATUS_PENDING);
        assert_eq!(registry.list_recent(10).len(), 1);
    }

    #[test]
    fn job_timestamps_preserve_go_rfc3339nano_precision() {
        let time: SystemTime = DateTime::parse_from_rfc3339("2026-07-01T00:00:00.123400000Z")
            .unwrap()
            .with_timezone(&Utc)
            .into();
        assert_eq!(rfc3339_nano(time), "2026-07-01T00:00:00.1234Z");
    }

    #[test]
    fn id3v23_parser_extracts_legacy_fixture_tags() {
        let dir = std::env::temp_dir().join(format!("sunflower-id3-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fixture.mp3");
        fs::write(
            &path,
            make_id3v23_mp3("Track 1", "Artist One", "Album Alpha", 1, 2024),
        )
        .unwrap();
        let extracted = extract_tags(&path).unwrap();
        let song = extracted.song;
        assert_eq!(song.title, "Track 1");
        assert_eq!(song.artist, "Artist One");
        assert_eq!(song.album, "Album Alpha");
        assert_eq!(song.year, Some(2024));
        assert_eq!(song.artist_media_id, local_media_id("a:artist one"));
        assert_eq!(
            song.album_media_id,
            local_media_id("l:album alpha|artist one")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn id3v23_apic_cover_art_is_saved_as_legacy_sizes() {
        let dir = std::env::temp_dir().join(format!("sunflower-id3-{}", Uuid::new_v4()));
        let data_dir = dir.join("data");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cover.mp3");
        fs::write(
            &path,
            make_id3v23_mp3_with_cover(
                "Track 1",
                "Artist One",
                "Album Alpha",
                1,
                2024,
                &tiny_jpeg(),
            ),
        )
        .unwrap();
        let extracted = extract_tags(&path).unwrap();
        let cover = extracted.cover_art.expect("cover art");
        save_cover_art(
            &cover,
            &extracted.song.album_media_id,
            &data_dir.to_string_lossy(),
        )
        .unwrap();
        for size in [256, 512, 1024] {
            let art = data_dir
                .join("art")
                .join(&extracted.song.album_media_id)
                .join(format!("{size}.jpg"));
            assert!(art.exists(), "missing {}", art.display());
            assert!(fs::metadata(art).unwrap().len() > 0);
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn id3v23_parser_rejects_invalid_mp3_like_go_tag_reader() {
        let dir = std::env::temp_dir().join(format!("sunflower-id3-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.mp3");
        fs::write(&path, b"not really an mp3").unwrap();
        assert!(extract_tags(&path).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn non_mp3_audio_files_fallback_to_filename_metadata() {
        let dir = std::env::temp_dir().join(format!("sunflower-non-mp3-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        for extension in ["flac", "m4a", "ogg", "opus"] {
            let path = dir.join(format!("Fallback {extension}.{extension}"));
            fs::write(&path, b"not enough metadata for a parser").unwrap();

            let extracted = extract_tags(&path).unwrap();
            let song = extracted.song;
            let path_text = path.to_string_lossy().to_string();
            assert_eq!(song.title, format!("Fallback {extension}"));
            assert_eq!(song.artist, "");
            assert_eq!(song.album, "");
            assert_eq!(song.year, None);
            assert_eq!(song.media_id, local_media_id(&path_text), "{extension}");
            assert_eq!(song.local_path, path_text);
            assert!(extracted.cover_art.is_none());
        }

        let _ = fs::remove_dir_all(dir);
    }

    fn make_id3v23_mp3(title: &str, artist: &str, album: &str, track: i32, year: i32) -> Vec<u8> {
        make_id3v23_mp3_with_cover(title, artist, album, track, year, &[])
    }

    fn make_id3v23_mp3_with_cover(
        title: &str,
        artist: &str,
        album: &str,
        track: i32,
        year: i32,
        cover: &[u8],
    ) -> Vec<u8> {
        let mut frames = Vec::new();
        fn write_text_frame(frames: &mut Vec<u8>, id: &str, text: &str) {
            let mut data = Vec::with_capacity(text.len() + 1);
            data.push(0);
            data.extend_from_slice(text.as_bytes());
            frames.extend_from_slice(id.as_bytes());
            frames.extend_from_slice(&(data.len() as u32).to_be_bytes());
            frames.extend_from_slice(&[0, 0]);
            frames.extend_from_slice(&data);
        }
        fn write_apic_frame(frames: &mut Vec<u8>, cover: &[u8]) {
            if cover.is_empty() {
                return;
            }
            let mut data = Vec::new();
            data.push(0);
            data.extend_from_slice(b"image/jpeg");
            data.push(0);
            data.push(3);
            data.push(0);
            data.extend_from_slice(cover);
            frames.extend_from_slice(b"APIC");
            frames.extend_from_slice(&(data.len() as u32).to_be_bytes());
            frames.extend_from_slice(&[0, 0]);
            frames.extend_from_slice(&data);
        }
        write_text_frame(&mut frames, "TIT2", title);
        write_text_frame(&mut frames, "TPE1", artist);
        write_text_frame(&mut frames, "TALB", album);
        write_text_frame(&mut frames, "TRCK", &track.to_string());
        write_text_frame(&mut frames, "TYER", &year.to_string());
        write_apic_frame(&mut frames, cover);
        let tag_size = frames.len();
        let mut out = Vec::with_capacity(10 + tag_size);
        out.extend_from_slice(b"ID3");
        out.extend_from_slice(&[3, 0, 0]);
        out.extend_from_slice(&[
            ((tag_size >> 21) & 0x7f) as u8,
            ((tag_size >> 14) & 0x7f) as u8,
            ((tag_size >> 7) & 0x7f) as u8,
            (tag_size & 0x7f) as u8,
        ]);
        out.extend_from_slice(&frames);
        out
    }

    fn tiny_jpeg() -> Vec<u8> {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([255, 0, 0]),
        ));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }
}
