//! Core data types shared by the master-playlist and media-playlist parsers.

use std::fmt;

/// What kind of m3u8 document a given file/text is.
///
/// Per RFC 8216, a single m3u8 document is *either* a "Multivariant
/// Playlist" (what most people mean by "playlist": a list of variant
/// streams/renditions pointing at other m3u8 files) *or* a "Media
/// Playlist" (a list of media segments belonging to one single rendition,
/// i.e. the thing you actually download).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum M3u8Kind {
    MasterPlaylist,
    MediaPlaylist,
    Unknown,
}

impl fmt::Display for M3u8Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            M3u8Kind::MasterPlaylist => write!(f, "master playlist"),
            M3u8Kind::MediaPlaylist => write!(f, "media playlist"),
            M3u8Kind::Unknown => write!(f, "unknown"),
        }
    }
}

/// Video resolution in pixels, as found in `EXT-X-STREAM-INF:RESOLUTION=WxH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantStream {
    pub url: String,
    pub bandwidth: Option<u64>,
    pub average_bandwidth: Option<u64>,
    pub resolution: Option<Resolution>,
    pub codecs: Option<String>,
    pub frame_rate: Option<f64>,
    pub video_range: Option<String>,
    pub audio_group: Option<String>,
    pub subtitles_group: Option<String>,
    pub video_group: Option<String>,
    pub is_iframe_stream: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaRendition {
    pub media_type: MediaType,
    pub group_id: String,
    pub name: String,
    pub language: Option<String>,
    pub url: Option<String>,
    pub is_default: bool,
    pub autoselect: bool,
    pub forced: bool,
    pub channels: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Audio,
    Video,
    Subtitles,
    ClosedCaptions,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MasterPlaylist {
    pub variants: Vec<VariantStream>,
    pub renditions: Vec<MediaRendition>,
    pub independent_segments: bool,
}

impl MasterPlaylist {
    pub fn variants_by_quality_desc(&self) -> Vec<&VariantStream> {
        let mut v: Vec<&VariantStream> = self
            .variants
            .iter()
            .filter(|s| !s.is_iframe_stream)
            .collect();
        v.sort_by(|a, b| b.bandwidth.unwrap_or(0).cmp(&a.bandwidth.unwrap_or(0)));
        v
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub length: u64,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncryptionKey {
    /// e.g. `"AES-128"`, `"SAMPLE-AES"`, or `"NONE"`.
    pub method: String,
    pub uri: Option<String>,
    pub iv: Option<String>,
    pub key_format: Option<String>,
    pub key_format_versions: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InitializationSegment {
    pub url: String,
    pub byte_range: Option<ByteRange>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub url: String,
    pub duration: f64,
    pub title: Option<String>,
    pub byte_range: Option<ByteRange>,
    pub key: Option<EncryptionKey>,
    pub map: Option<InitializationSegment>,
    pub discontinuity: bool,
    pub program_date_time: Option<String>,
    pub sequence: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaPlaylist {
    pub segments: Vec<Segment>,
    pub target_duration: Option<u64>,
    pub media_sequence: u64,
    pub playlist_type: Option<String>,
    pub is_complete: bool,
    pub iframes_only: bool,
    pub version: Option<u32>,
}

impl MediaPlaylist {
    pub fn total_duration(&self) -> f64 {
        self.segments.iter().map(|s| s.duration).sum()
    }
}
