use std::{ffi::OsString, path::Path};

use crate::{
    merge::{
        AAC_MAX_BITRATE_KBPS, AAC_MIN_BITRATE_KBPS, HLS_INPUT_FLAGS, audio_target::AudioTarget,
        container::Container, errors::MergeError, output_preset::OutputPreset,
        stream_info::StreamInfo, video_target::VideoTarget,
    },
    utils::os_strings::push_strs,
};

pub fn validate_preset(preset: &OutputPreset, info: &StreamInfo) -> Result<(), MergeError> {
    if preset.strips_video && preset.video.is_some() {
        return Err(MergeError::InvalidPreset(format!(
            "'{}' strips video but also selects a video target",
            preset.label
        )));
    }
    if !preset.strips_video && preset.video.is_none() && info.has_video {
        return Err(MergeError::InvalidPreset(format!(
            "'{}' has no video target but the source has video; use an audio-extraction preset",
            preset.label
        )));
    }
    if preset.container.is_audio_only() && preset.video.is_some() {
        return Err(MergeError::InvalidPreset(format!(
            "'{}' selects a video target for an audio-only container",
            preset.label
        )));
    }
    if preset.strips_video && !info.has_audio {
        return Err(MergeError::InvalidPreset(format!(
            "'{}' extracts audio but the source has no audio track",
            preset.label
        )));
    }
    if preset.video.is_none() && preset.audio.is_none() {
        return Err(MergeError::InvalidPreset(format!(
            "'{}' selects no streams",
            preset.label
        )));
    }
    Ok(())
}

fn video_args(target: VideoTarget) -> &'static [&'static str] {
    match target {
        VideoTarget::Copy => &["-c:v", "copy"],
        VideoTarget::H264 => &[
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
        ],
        VideoTarget::H265 => &["-c:v", "libx265", "-preset", "medium", "-crf", "22"],
        VideoTarget::Vp9 => &["-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0"],
        VideoTarget::Av1 => &["-c:v", "libsvtav1", "-crf", "30", "-preset", "8"],
    }
}

fn audio_args(target: AudioTarget, info: &StreamInfo) -> Vec<String> {
    let strs = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match target {
        AudioTarget::Copy => strs(&["-c:a", "copy"]),
        AudioTarget::Aac => {
            let source_kbps = info.audio_bitrate_bps.map_or(0, |bps| bps / 1000);
            let kbps = source_kbps.clamp(AAC_MIN_BITRATE_KBPS, AAC_MAX_BITRATE_KBPS);
            vec![
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                format!("{kbps}k"),
            ]
        }
        AudioTarget::Mp3 => strs(&["-c:a", "libmp3lame", "-q:a", "2"]),
        AudioTarget::Opus => strs(&["-c:a", "libopus", "-b:a", "160k"]),
        AudioTarget::Flac => strs(&["-c:a", "flac"]),
        AudioTarget::Vorbis => strs(&["-c:a", "libvorbis", "-q:a", "6"]),
    }
}

/// Builds the full ffmpeg argument list. Pure function, no I/O.
pub fn build_ffmpeg_args(
    playlist: &Path,
    output: &Path,
    preset: &OutputPreset,
    info: &StreamInfo,
) -> Result<Vec<OsString>, MergeError> {
    validate_preset(preset, info)?;

    let mut args: Vec<OsString> = Vec::new();
    push_strs(
        &mut args,
        &["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error"],
    );
    push_strs(&mut args, HLS_INPUT_FLAGS);
    args.push(OsString::from("-i"));
    args.push(playlist.as_os_str().to_owned());

    match preset.video {
        Some(target) if !preset.strips_video => {
            push_strs(&mut args, &["-map", "0:v:0?"]);
            push_strs(&mut args, video_args(target));
        }
        _ => push_strs(&mut args, &["-vn"]),
    }

    match preset.audio {
        Some(target) => {
            push_strs(&mut args, &["-map", "0:a:0?"]);
            args.extend(audio_args(target, info).into_iter().map(OsString::from));
        }
        None => push_strs(&mut args, &["-an"]),
    }

    if matches!(preset.container, Container::Mp4 | Container::M4a) {
        push_strs(&mut args, &["-movflags", "+faststart"]);
    }

    push_strs(&mut args, &["-y"]);
    args.push(output.as_os_str().to_owned());
    Ok(args)
}

#[cfg(test)]
mod test {
    use crate::merge::{audio_target::AudioTarget, ffmpeg::audio_args, tests::video_audio_info};

    #[test]
    fn aac_bitrate_follows_higher_source_bitrate() {
        let mut info = video_audio_info();
        info.audio_bitrate_bps = Some(256_000);
        assert_eq!(audio_args(AudioTarget::Aac, &info)[3], "256k");
        info.audio_bitrate_bps = Some(64_000);
        assert_eq!(audio_args(AudioTarget::Aac, &info)[3], "192k");
    }
}
