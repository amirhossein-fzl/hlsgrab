use std::path::PathBuf;

/// All errors that can occur while configuring or running a download.
#[derive(Debug, thiserror::Error)]
pub enum DownloaderError {
    #[error("Invalid url!")]
    InvalidUrl,

    #[error("invalid proxy string '{0}': {1}")]
    InvalidProxy(String, String),

    #[error("invalid header name '{0}'")]
    InvalidHeaderName(String),

    #[error("invalid header value for '{0}'")]
    InvalidHeaderValue(String),

    #[error("failed to build http client: {0}")]
    ClientBuild(#[source] reqwest::Error),

    #[error("request failed: {0}")]
    Request(#[source] reqwest::Error),

    #[error("server returned status {0}")]
    BadStatus(reqwest::StatusCode),

    #[error("io error while writing to '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
