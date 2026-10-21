#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mkv,
    Webm,
    M4a,
    Mp3,
    Ogg,
    Flac,
}

impl Container {
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mkv => "mkv",
            Container::Webm => "webm",
            Container::M4a => "m4a",
            Container::Mp3 => "mp3",
            Container::Ogg => "ogg",
            Container::Flac => "flac",
        }
    }

    pub fn from_str(container: &str) -> Option<Self> {
        match container {
            "mp4" => Some(Container::Mp4),
            "mkv" => Some(Container::Mkv),
            "webm" => Some(Container::Webm),
            "m4a" => Some(Container::M4a),
            "mp3" => Some(Container::Mp3),
            "ogg" => Some(Container::Ogg),
            "flac" => Some(Container::Flac),
            _ => None,
        }
    }

    pub fn is_audio_only(self) -> bool {
        matches!(
            self,
            Container::M4a | Container::Mp3 | Container::Ogg | Container::Flac
        )
    }
}
