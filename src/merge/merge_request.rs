use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{
    hls::media::parse_media_playlist,
    merge::{
        FFMPEG_BIN, ProcessRunner,
        errors::MergeError,
        fallback_targets,
        ffmpeg::build_ffmpeg_args,
        output_preset::{OutputPreset, presets_for},
        probe::run_ffprobe,
        runner::TokioRunner,
        stream_info::StreamInfo,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRequest {
    pub playlist_path: PathBuf,
    pub output_dir: PathBuf,
    /// File name without extension.
    pub output_filename: String,
}

#[derive(Debug, Clone, Default)]
pub struct MergeRequestBuilder {
    playlist_path: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    output_filename: Option<String>,
}

impl MergeRequest {
    pub fn builder() -> MergeRequestBuilder {
        MergeRequestBuilder::default()
    }

    /// Runs ffprobe and returns the detected stream layout.
    pub async fn probe(&self) -> Result<StreamInfo, MergeError> {
        self.probe_with(&TokioRunner).await
    }

    /// Returns the presets applicable to the given stream info.
    /// Pure function, no I/O.
    pub fn available_presets(&self, info: &StreamInfo) -> Vec<OutputPreset> {
        presets_for(info)
    }

    /// Probes the playlist, then runs ffmpeg to produce the output file.
    /// Returns the absolute path of the produced file.
    ///
    /// Hosts that already hold a `StreamInfo` should call
    /// [`MergeRequest::merge_with_info`] to avoid probing twice.
    pub async fn merge(&self, preset: &OutputPreset) -> Result<PathBuf, MergeError> {
        let info = self.probe().await?;
        self.merge_with_info(preset, &info).await
    }

    /// Same as [`MergeRequest::merge`] but uses an already-probed `StreamInfo`.
    pub async fn merge_with_info(
        &self,
        preset: &OutputPreset,
        info: &StreamInfo,
    ) -> Result<PathBuf, MergeError> {
        self.merge_with(&TokioRunner, preset, info).await
    }

    async fn probe_with<R: ProcessRunner>(&self, runner: &R) -> Result<StreamInfo, MergeError> {
        if !self.playlist_path.is_file() {
            return Err(MergeError::PlaylistNotFound(self.playlist_path.clone()));
        }

        // Primary path: let ffprobe parse the playlist itself.
        let streams = run_ffprobe(runner, &self.playlist_path, true).await?;
        let mut info = StreamInfo::default();
        info.absorb(&streams);
        if info.is_usable() {
            return Ok(info);
        }

        // Fallback: probe the init segment and/or the first media segment.
        let text = read_playlist(&self.playlist_path)?;
        let base_dir = self.playlist_path.parent().unwrap_or_else(|| Path::new(""));
        let parsed = parse_media_playlist(&text, base_dir.to_str());
        let targets = fallback_targets(&parsed)?;

        let mut last_error: Option<MergeError> = None;
        for target in targets {
            match run_ffprobe(runner, &target, false).await {
                Ok(streams) => info.absorb(&streams),
                Err(err @ MergeError::FfprobeFailed { .. }) => last_error = Some(err),
                Err(err) => return Err(err),
            }
        }

        if info.is_usable() {
            Ok(info)
        } else {
            let detail = last_error.map(|e| format!(" ({e})")).unwrap_or_default();
            Err(MergeError::ProbeFailed {
                reason: format!(
                    "no usable streams found in the playlist or its first segment{detail}"
                ),
            })
        }
    }

    async fn merge_with<R: ProcessRunner>(
        &self,
        runner: &R,
        preset: &OutputPreset,
        info: &StreamInfo,
    ) -> Result<PathBuf, MergeError> {
        if !self.playlist_path.is_file() {
            return Err(MergeError::PlaylistNotFound(self.playlist_path.clone()));
        }

        let file_name = format!("{}.{}", self.output_filename, preset.container.extension());
        let output_path =
            std::path::absolute(self.output_dir.join(file_name)).map_err(MergeError::OutputDir)?;

        let args = build_ffmpeg_args(&self.playlist_path, &output_path, preset, info)?;

        std::fs::create_dir_all(&self.output_dir).map_err(MergeError::OutputDir)?;

        let output = runner.run(FFMPEG_BIN, &args).await?;
        if output.code != 0 {
            // Do not leave a partial file behind.
            let _ = std::fs::remove_file(&output_path);
            return Err(MergeError::FfmpegFailed {
                code: output.code,
                stderr: output.stderr,
            });
        }

        Ok(output_path)
    }
}

impl MergeRequestBuilder {
    pub fn playlist_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.playlist_path = Some(path.into());
        self
    }

    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    pub fn output_filename(mut self, name: impl Into<String>) -> Self {
        self.output_filename = Some(name.into());
        self
    }

    pub fn build(self) -> Result<MergeRequest, MergeError> {
        let playlist_path = self
            .playlist_path
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or(MergeError::MissingField("playlist_path"))?;
        let output_dir = self
            .output_dir
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or(MergeError::MissingField("output_dir"))?;
        let output_filename = self
            .output_filename
            .filter(|n| !n.trim().is_empty())
            .ok_or(MergeError::MissingField("output_filename"))?;

        let bad_name = output_filename == "."
            || output_filename == ".."
            || output_filename.contains(['/', '\\', '\0']);
        if bad_name {
            return Err(MergeError::InvalidFilename(output_filename));
        }

        Ok(MergeRequest {
            playlist_path,
            output_dir,
            output_filename,
        })
    }
}

fn read_playlist(path: &Path) -> Result<String, MergeError> {
    std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            MergeError::PlaylistNotFound(path.to_path_buf())
        } else {
            MergeError::PlaylistRead(e)
        }
    })
}
