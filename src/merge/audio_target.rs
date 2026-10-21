#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTarget {
    Copy,
    Aac,
    Mp3,
    Opus,
    Flac,
    Vorbis,
}
