use std::{collections::HashMap, fs, io::Write, path::Path, process::exit, time::Duration};

use clap::Args;
use cliclack::{ProgressBar, multi_progress, outro, progress_bar, select};
use reqwest::Url;

use crate::{
    downloader::{
        client::DownloaderClient,
        download::{download, download_to_file},
        progress::ProgressUpdate,
    },
    hls::{
        detect::detect_m3u8_kind,
        localize::render_localized_playlist,
        master::parse_master_playlist,
        media::parse_media_playlist,
        types::{
            M3u8Kind, MasterPlaylist, MediaPlaylist, MediaRendition, MediaType, VariantStream,
        },
    },
    utils::{http::parse_header, url::filename_from_url},
};

#[derive(Debug, Args, Clone)]
pub struct DownloadArgs {
    /// Url of m3u8.
    #[arg()]
    url: String,

    /// Continue download playlist (skip segments/files already on disk).
    #[arg(long, short, action)]
    continue_download: bool,

    /// Output directory.
    #[arg(long, short, value_name = "PATH", default_value = "")]
    output_directory: String,

    /// Custom http headers.
    #[arg(long)]
    header: Vec<String>,

    /// Cookies (In header string format).
    #[arg(long, default_value = "")]
    cookies: String,

    /// Number of concurrent download of segments.
    #[arg(long, default_value_t = 1)]
    max_concurrent_download: i32,

    /// Set timeout in seconds.
    #[arg(long, short, default_value_t = 30)]
    timeout: u64,

    /// Proxy url. [http|socks5://][USER:PASSWORD@]HOST[:PORT]
    #[arg(long, short, value_name = "URL", default_value = "")]
    proxy: String,
}

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("{}", message.as_ref());
    exit(1);
}

trait OrFail<T> {
    fn or_fail(self, context: &str) -> T;
}

impl<T, E: std::fmt::Display> OrFail<T> for Result<T, E> {
    fn or_fail(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(err) => fail(format!("{context}: {err}")),
        }
    }
}

impl<T> OrFail<T> for Option<T> {
    fn or_fail(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => fail(context),
        }
    }
}

fn build_client(args: &DownloadArgs) -> DownloaderClient {
    let mut builder = DownloaderClient::builder().timeout(Duration::from_secs(args.timeout));

    if !args.proxy.is_empty() {
        builder = builder.proxy(&args.proxy).or_fail("Invalid proxy url");
    }

    if !args.cookies.is_empty() {
        builder = builder
            .cookie_str(&args.cookies)
            .or_fail("Invalid cookie header string");
    }

    if !args.header.is_empty() {
        let headers: Vec<(String, String)> = args
            .header
            .iter()
            .map(|header| parse_header(header.as_str()).or_fail("Invalid --header value"))
            .collect();

        builder = builder.headers(headers).or_fail("Invalid header string");
    }

    builder.build().or_fail("Error building http client")
}

pub async fn handle_download_action(args: &DownloadArgs) {
    let url = Url::parse(&args.url).or_fail("Invalid url");

    let output_directory = Path::new(&args.output_directory).join("media");
    if !output_directory.exists() {
        fs::create_dir_all(&output_directory).or_fail("Could not create output directory")
    }

    let client = build_client(args);

    let root_playlist = fetch_playlist_text(&client, url.as_ref()).await;
    let kind = detect_m3u8_kind(&root_playlist).or_fail("Error in m3u8 detection");

    match kind {
        M3u8Kind::MasterPlaylist => {
            let master = parse_master_playlist(&root_playlist, Some(&args.url));
            handle_master_playlist(&client, &master, &output_directory, args).await;
        }
        M3u8Kind::MediaPlaylist => {
            download_media_playlist(&client, &args.url, &output_directory, "video", args).await;
        }
        M3u8Kind::Unknown => fail("This m3u8 is unknown."),
    }

    outro("Done!").ok();
}

async fn fetch_playlist_text(client: &DownloaderClient, url: &str) -> String {
    let downloaded = download_to_file(client, url, &std::env::temp_dir(), |_| {})
        .await
        .or_fail("Download error");

    fs::read_to_string(&downloaded.path).or_fail("Error reading downloaded playlist")
}

async fn handle_master_playlist(
    client: &DownloaderClient,
    master: &MasterPlaylist,
    output_directory: &Path,
    args: &DownloadArgs,
) {
    let variants = master.variants_by_quality_desc();

    if variants.is_empty() {
        fail("This master playlist has no playable variants.");
    }

    let items: Vec<(&String, String, String)> = variants
        .iter()
        .map(|v| (&v.url, describe_variant(v), String::new()))
        .collect();

    let chosen_url = select("Select quality")
        .filter_mode()
        .items(&items)
        .interact()
        .or_fail("Error in select variant");

    let chosen_variant: &VariantStream = variants
        .iter()
        .copied()
        .find(|v| v.url == *chosen_url)
        .or_fail("Selected variant disappeared unexpectedly");

    // Download the video media playlist.
    download_media_playlist(client, &chosen_variant.url, output_directory, "video", args).await;

    // If this variant references an audio group, resolve and download it too.
    let Some(audio_group_id) = &chosen_variant.audio_group else {
        return;
    };

    let audio_renditions: Vec<&MediaRendition> = master
        .renditions
        .iter()
        .filter(|r| r.media_type == MediaType::Audio && r.group_id == *audio_group_id)
        .collect();

    let chosen_audio = match audio_renditions.len() {
        0 => return,
        1 => audio_renditions[0],
        _ => {
            let audio_items: Vec<(&String, String, String)> = audio_renditions
                .iter()
                .filter_map(|r| {
                    r.url
                        .as_ref()
                        .map(|u| (u, describe_rendition(r), String::new()))
                })
                .collect();

            if audio_items.is_empty() {
                return;
            }

            let chosen_audio_url = select("Select audio track")
                .filter_mode()
                .items(&audio_items)
                .interact()
                .or_fail("Error in select audio track");

            audio_renditions
                .into_iter()
                .find(|r| r.url.as_deref() == Some(chosen_audio_url.as_str()))
                .or_fail("Selected audio track disappeared unexpectedly")
        }
    };

    let Some(audio_url) = &chosen_audio.url else {
        return;
    };

    let audio_output_directory = output_directory.join("audio");
    if !audio_output_directory.exists() {
        fs::create_dir_all(&audio_output_directory)
            .or_fail("Could not create audio output directory")
    }

    download_media_playlist(client, audio_url, &audio_output_directory, "audio", args).await;
}

fn describe_variant(v: &VariantStream) -> String {
    let mut parts = Vec::new();

    if let Some(res) = v.resolution {
        parts.push(format!("{res}"));
    }

    if let Some(codecs) = &v.codecs {
        parts.push(codecs.clone());
    }

    if let Some(bw) = v.bandwidth {
        parts.push(format!("{} kbps", bw / 1000));
    }

    if parts.is_empty() {
        let unknown_variant = "Unknown variant".to_string();

        if let Ok(url) = Url::parse(&v.url) {
            return filename_from_url(&url).unwrap_or_else(|| unknown_variant.to_owned());
        }

        unknown_variant
    } else {
        parts.join(", ")
    }
}

fn describe_rendition(r: &MediaRendition) -> String {
    let mut label = r.name.clone();

    if let Some(lang) = &r.language {
        label.push_str(&format!(" ({lang})"));
    }

    if r.is_default {
        label.push_str(" [default]");
    }

    label
}

pub async fn download_media_playlist(
    client: &DownloaderClient,
    url: &str,
    output_directory: &Path,
    label: &str,
    args: &DownloadArgs,
) {
    let playlist_text = fetch_playlist_text(client, url).await;

    let kind = detect_m3u8_kind(&playlist_text).or_fail("Error in m3u8 detection");
    if kind != M3u8Kind::MediaPlaylist {
        fail(format!(
            "Expected a media playlist for the {label} track but got something else."
        ));
    }

    let media_playlist: MediaPlaylist = parse_media_playlist(&playlist_text, Some(url));
    let localized_playlist = render_localized_playlist(&media_playlist);

    let mut playlist_file = fs::File::create(output_directory.join("playlist.m3u8"))
        .or_fail("Error creating playlist file");
    playlist_file
        .write_all(localized_playlist.content.as_bytes())
        .or_fail("Error writing playlist file");

    if localized_playlist.segments.is_empty() {
        eprintln!("Warning: {label} playlist has no segments, skipping.");
        return;
    }

    download_resources(client, localized_playlist.keys, output_directory).await;
    download_resources(client, localized_playlist.maps, output_directory).await;

    let total_segments = localized_playlist.segments.len();

    let multi = multi_progress(format!("Downloading {label}"));
    let segments_progress = multi.add(progress_bar(total_segments as u64));

    segments_progress.start(format!("0/{total_segments} segments"));

    for (index, (segment_filename, url)) in localized_playlist.segments.iter().enumerate() {
        if args.continue_download && output_directory.join(segment_filename).exists() {
            segments_progress.inc(1);
            continue;
        }

        let file_progress = multi.add(progress_bar(0).with_download_template());
        file_progress.start(segment_filename.clone());

        download_segment(
            client,
            url,
            output_directory,
            total_segments,
            index,
            &file_progress,
            &segments_progress,
        )
        .await
    }

    segments_progress.stop(format!("{label}: segments downloaded"));
    multi.stop();
}

async fn download_resources(
    client: &DownloaderClient,
    files: HashMap<String, String>,
    output_directory: &Path,
) {
    for (filename, url) in files {
        if !output_directory.join(&filename).exists() {
            download(client, &url, output_directory, &filename, |_| {})
                .await
                .or_fail(&format!("Can't download {filename}."));
        }
    }
}

fn progress_action(file_progress: &ProgressBar, progress: ProgressUpdate, last_reported: &mut u64) {
    if let Some(total) = progress.total_bytes {
        file_progress.set_length(total);
    }

    let delta = progress.bytes_downloaded.saturating_sub(*last_reported);
    if delta > 0 {
        file_progress.inc(delta);
        *last_reported = progress.bytes_downloaded;
    }
}

async fn download_segment(
    client: &DownloaderClient,
    url: &str,
    output_directory: &Path,
    total_segments: usize,
    index: usize,
    file_progress: &ProgressBar,
    segments_progress: &ProgressBar,
) {
    let mut last_reported: u64 = 0;
    loop {
        let download_result =
            download_to_file(client, url, output_directory, |progress: ProgressUpdate| {
                progress_action(file_progress, progress, &mut last_reported);
            })
            .await;

        match download_result {
            Ok(_) => {
                file_progress.stop("");
                break;
            }
            Err(_) => {
                file_progress.stop("");
                last_reported = 0;
            }
        }
    }

    segments_progress.inc(1);
    segments_progress.start(format!("{}/{total_segments} segments", index + 1));
}
