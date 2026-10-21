#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RawStream {
    pub codec_type: String,
    pub codec_name: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub sample_rate: Option<u64>,
    pub channels: Option<u64>,
    pub bit_rate: Option<u64>,
}
