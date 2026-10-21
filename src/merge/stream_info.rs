use crate::merge::{
    audio_codec::{AudioCodec, parse_audio_codec},
    raw_stream::RawStream,
    video_codec::{VideoCodec, parse_video_codec},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamInfo {
    pub has_video: bool,
    pub has_audio: bool,
    pub video_codec: Option<VideoCodec>,
    pub audio_codec: Option<AudioCodec>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    /// Source audio bitrate in bits per second, when known.
    pub audio_bitrate_bps: Option<u64>,
}

impl StreamInfo {
    pub fn absorb(&mut self, streams: &[RawStream]) {
        for stream in streams {
            let Some(name) = &stream.codec_name else {
                continue;
            };
            match stream.codec_type.as_str() {
                "video" if !self.has_video => {
                    self.has_video = true;
                    self.video_codec = Some(parse_video_codec(name));
                    self.width = stream.width.and_then(|v| u32::try_from(v).ok());
                    self.height = stream.height.and_then(|v| u32::try_from(v).ok());
                }
                "audio" if !self.has_audio => {
                    self.has_audio = true;
                    self.audio_codec = Some(parse_audio_codec(name));
                    self.sample_rate = stream.sample_rate.and_then(|v| u32::try_from(v).ok());
                    self.channels = stream.channels.and_then(|v| u8::try_from(v).ok());
                    self.audio_bitrate_bps = stream.bit_rate;
                }
                _ => {}
            }
        }
    }

    pub fn is_usable(&self) -> bool {
        self.has_video || self.has_audio
    }
}
