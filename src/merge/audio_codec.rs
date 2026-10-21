use crate::merge::container::Container;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCodec {
    Aac,
    Mp3,
    Opus,
    Flac,
    Vorbis,
    Other(String),
}

pub fn parse_audio_codec(name: &str) -> AudioCodec {
    match name.to_ascii_lowercase().as_str() {
        "aac" => AudioCodec::Aac,
        "mp3" => AudioCodec::Mp3,
        "opus" => AudioCodec::Opus,
        "flac" => AudioCodec::Flac,
        "vorbis" => AudioCodec::Vorbis,
        _ => AudioCodec::Other(name.to_string()),
    }
}

/// Container whose native audio codec matches `codec`, if any.
pub fn native_audio_container(codec: &AudioCodec) -> Option<Container> {
    match codec {
        AudioCodec::Aac => Some(Container::M4a),
        AudioCodec::Mp3 => Some(Container::Mp3),
        AudioCodec::Opus | AudioCodec::Vorbis => Some(Container::Ogg),
        AudioCodec::Flac => Some(Container::Flac),
        AudioCodec::Other(_) => None,
    }
}
