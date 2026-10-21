use crate::merge::{
    audio_codec::{AudioCodec, native_audio_container},
    audio_target::AudioTarget,
    container::Container,
    stream_info::StreamInfo,
    video_codec::VideoCodec,
    video_target::VideoTarget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPreset {
    pub label: &'static str,
    pub container: Container,
    /// `None` for audio-only outputs.
    pub video: Option<VideoTarget>,
    pub audio: Option<AudioTarget>,
    /// `true` for "extract audio" presets that drop the video track.
    pub strips_video: bool,
    pub is_default: bool,
}

pub const fn preset(
    label: &'static str,
    container: Container,
    video: Option<VideoTarget>,
    audio: Option<AudioTarget>,
    strips_video: bool,
    is_default: bool,
) -> OutputPreset {
    OutputPreset {
        label,
        container,
        video,
        audio,
        strips_video,
        is_default,
    }
}

pub fn presets_for(info: &StreamInfo) -> Vec<OutputPreset> {
    use AudioTarget as A;
    use Container as C;
    use VideoTarget as V;

    let mut candidates: Vec<OutputPreset> = Vec::new();

    if info.has_video {
        candidates.push(preset(
            "MP4 (H.264 + AAC)",
            C::Mp4,
            Some(V::H264),
            Some(A::Aac),
            false,
            true,
        ));
        candidates.push(preset(
            "MKV (H.264 + AAC)",
            C::Mkv,
            Some(V::H264),
            Some(A::Aac),
            false,
            false,
        ));
        candidates.push(preset(
            "WebM (VP9 + Opus)",
            C::Webm,
            Some(V::Vp9),
            Some(A::Opus),
            false,
            false,
        ));

        let mp4_copy_ok = info.video_codec == Some(VideoCodec::H264)
            && (!info.has_audio || info.audio_codec == Some(AudioCodec::Aac));
        if mp4_copy_ok {
            candidates.push(preset(
                "MP4 (copy, no re-encode)",
                C::Mp4,
                Some(V::Copy),
                Some(A::Copy),
                false,
                false,
            ));
        }
        candidates.push(preset(
            "MKV (copy, no re-encode)",
            C::Mkv,
            Some(V::Copy),
            Some(A::Copy),
            false,
            false,
        ));

        // Audio extraction only makes sense when there is audio to extract.
        if info.has_audio {
            candidates.push(preset(
                "Audio only \u{2014} M4A (AAC)",
                C::M4a,
                None,
                Some(A::Aac),
                true,
                false,
            ));
            candidates.push(preset(
                "Audio only \u{2014} MP3",
                C::Mp3,
                None,
                Some(A::Mp3),
                true,
                false,
            ));
            if let Some(container) = info.audio_codec.as_ref().and_then(native_audio_container) {
                candidates.push(preset(
                    "Audio only \u{2014} Copy",
                    container,
                    None,
                    Some(A::Copy),
                    true,
                    false,
                ));
            }
        }
    } else if info.has_audio {
        candidates.push(preset("M4A (AAC)", C::M4a, None, Some(A::Aac), false, true));
        candidates.push(preset("MP3", C::Mp3, None, Some(A::Mp3), false, false));
        candidates.push(preset(
            "Opus (OGG)",
            C::Ogg,
            None,
            Some(A::Opus),
            false,
            false,
        ));
        candidates.push(preset("FLAC", C::Flac, None, Some(A::Flac), false, false));
        if let Some(container) = info.audio_codec.as_ref().and_then(native_audio_container) {
            candidates.push(preset(
                "Copy audio (no re-encode)",
                container,
                None,
                Some(A::Copy),
                false,
                false,
            ));
        }
    }

    let mut unique: Vec<OutputPreset> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}
