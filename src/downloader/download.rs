use std::fs;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use reqwest::Url;
use reqwest::header::CONTENT_LENGTH;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::downloader::client::DownloaderClient;
use crate::downloader::error::DownloaderError;
use crate::downloader::progress::{ProgressUpdate, SpeedTracker};

/// Result of a successful download.
pub enum DownloadOutput {
    File { path: PathBuf, bytes_written: u64 },
}

pub async fn download<F>(
    client: &DownloaderClient,
    url: &str,
    output_dir: &Path,
    filename: &str,
    mut on_progress: F,
) -> Result<DownloadOutput, DownloaderError>
where
    F: FnMut(ProgressUpdate),
{
    let response = client
        .client
        .get(url)
        .send()
        .await
        .map_err(DownloaderError::Request)?;

    if !response.status().is_success() {
        return Err(DownloaderError::BadStatus(response.status()));
    }

    let total_bytes = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let output_file_path = output_dir.join(filename);

    let mut file_handle = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(output_file_path.clone())
        .await
        .map_err(|e| DownloaderError::Io {
            path: output_file_path.to_path_buf(),
            source: e,
        })?;

    let mut bytes_so_far: u64 = 0;
    let mut speed_tracker = SpeedTracker::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(DownloaderError::Request)?;

        let f = &mut file_handle;
        f.write_all(&chunk).await.map_err(|e| DownloaderError::Io {
            path: output_file_path.to_path_buf(),
            source: e,
        })?;

        bytes_so_far += chunk.len() as u64;

        let speed = speed_tracker.sample(bytes_so_far);
        let percentage = total_bytes.map(|total| {
            if total == 0 {
                100.0
            } else {
                (bytes_so_far as f64 / total as f64) * 100.0
            }
        });

        on_progress(ProgressUpdate {
            bytes_downloaded: bytes_so_far,
            total_bytes,
            speed_bytes_per_sec: speed,
            percentage,
        });
    }

    file_handle.flush().await.map_err(|e| DownloaderError::Io {
        path: output_file_path.to_path_buf(),
        source: e,
    })?;

    Ok(DownloadOutput::File {
        path: output_file_path.to_path_buf(),
        bytes_written: bytes_so_far,
    })
}

pub async fn download_to_file<F>(
    client: &DownloaderClient,
    url: &str,
    directory: &Path,
    on_progress: F,
) -> Result<u64, DownloaderError>
where
    F: FnMut(ProgressUpdate),
{
    if !directory.exists() {
        let _ = fs::create_dir_all(directory);
    }

    let parsed_url = Url::parse(&url).expect("Error");
    let filename = Path::new(parsed_url.path()).file_name().unwrap().to_str().unwrap();

    match download(client, url, directory.as_ref(), filename, on_progress).await? {
        DownloadOutput::File { bytes_written, .. } => Ok(bytes_written),
    }
}
