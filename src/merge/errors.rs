use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid output filename: {0}")]
    InvalidFilename(String),
    #[error("playlist file not found: {}", .0.display())]
    PlaylistNotFound(PathBuf),
    #[error("failed to read playlist: {0}")]
    PlaylistRead(#[from] io::Error),
    #[error("failed to spawn {binary}: {source}")]
    Spawn {
        binary: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("ffprobe exited with code {code}: {stderr}")]
    FfprobeFailed { code: i32, stderr: String },
    #[error("could not determine stream info from playlist: {reason}")]
    ProbeFailed { reason: String },
    #[error("ffmpeg exited with code {code}: {stderr}")]
    FfmpegFailed { code: i32, stderr: String },
    #[error("invalid preset: {0}")]
    InvalidPreset(String),
    #[error("failed to create output directory: {0}")]
    OutputDir(#[source] io::Error),
}
