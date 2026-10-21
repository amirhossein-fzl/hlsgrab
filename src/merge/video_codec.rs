#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
    Av1,
    Other(String),
}

pub fn parse_video_codec(name: &str) -> VideoCodec {
    match name.to_ascii_lowercase().as_str() {
        "h264" => VideoCodec::H264,
        "hevc" | "h265" => VideoCodec::H265,
        "vp9" => VideoCodec::Vp9,
        "av1" => VideoCodec::Av1,
        _ => VideoCodec::Other(name.to_string()),
    }
}
