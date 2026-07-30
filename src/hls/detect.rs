use crate::hls::types::M3u8Kind;

const MASTER_INDICATOR_TAGS: &[&str] = &[
    "#EXT-X-STREAM-INF",
    "#EXT-X-I-FRAME-STREAM-INF",
    "#EXT-X-MEDIA:",
    "#EXT-X-SESSION-DATA",
    "#EXT-X-SESSION-KEY",
];

const MEDIA_INDICATOR_TAGS: &[&str] = &[
    "#EXTINF",
    "#EXT-X-TARGETDURATION",
    "#EXT-X-MEDIA-SEQUENCE",
    "#EXT-X-DISCONTINUITY-SEQUENCE",
    "#EXT-X-ENDLIST",
    "#EXT-X-PLAYLIST-TYPE",
    "#EXT-X-I-FRAMES-ONLY",
    "#EXT-X-BYTERANGE",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DetectError {
    #[error("The file is not m3u8 format.")]
    NotM3u8,
}

pub fn detect_m3u8_kind(content: &str) -> Result<M3u8Kind, DetectError> {
    let trimmed = strip_bom(content);

    if !has_extm3u_header(trimmed) {
        return Err(DetectError::NotM3u8);
    }

    for line in trimmed.lines() {
        let line = line.trim();
        if MASTER_INDICATOR_TAGS
            .iter()
            .any(|tag| line.starts_with(tag))
        {
            return Ok(M3u8Kind::MasterPlaylist);
        }
    }

    for line in trimmed.lines() {
        let line = line.trim();
        if MEDIA_INDICATOR_TAGS.iter().any(|tag| line.starts_with(tag)) {
            return Ok(M3u8Kind::MediaPlaylist);
        }
    }

    Ok(M3u8Kind::Unknown)
}

fn strip_bom(content: &str) -> &str {
    content.strip_prefix('\u{feff}').unwrap_or(content)
}

fn has_extm3u_header(content: &str) -> bool {
    content
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|first| first == "#EXTM3U")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_master_playlist() {
        let content = "\
#EXTM3U
#EXT-X-VERSION:6
#EXT-X-STREAM-INF:BANDWIDTH=1280000,RESOLUTION=1920x1080
1080p.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=640000,RESOLUTION=1280x720
720p.m3u8
";
        assert_eq!(detect_m3u8_kind(content), Ok(M3u8Kind::MasterPlaylist));
    }

    #[test]
    fn detects_media_playlist() {
        let content = "\
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:9.009,
segment0.ts
#EXTINF:9.009,
segment1.ts
#EXT-X-ENDLIST
";
        assert_eq!(detect_m3u8_kind(content), Ok(M3u8Kind::MediaPlaylist));
    }

    #[test]
    fn detects_media_playlist_with_byterange_only() {
        // Edge case: some packagers emit only BYTERANGE/EXTINF without
        // TARGETDURATION on truncated/live snapshots.
        let content = "#EXTM3U\n#EXTINF:6.0,\n#EXT-X-BYTERANGE:75232@0\nseg.mp4\n";
        assert_eq!(detect_m3u8_kind(content), Ok(M3u8Kind::MediaPlaylist));
    }

    #[test]
    fn rejects_non_m3u8_content() {
        let content = "not an m3u8 file at all";
        assert_eq!(detect_m3u8_kind(content), Err(DetectError::NotM3u8));
    }

    #[test]
    fn handles_bom_and_leading_blank_lines() {
        let content = "\u{feff}\n\n#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000\nlow.m3u8\n";
        assert_eq!(detect_m3u8_kind(content), Ok(M3u8Kind::MasterPlaylist));
    }

    #[test]
    fn unknown_for_header_only_playlist() {
        let content = "#EXTM3U\n";
        assert_eq!(detect_m3u8_kind(content), Ok(M3u8Kind::Unknown));
    }

    #[test]
    fn master_with_media_tag_and_no_stream_inf() {
        // EXT-X-MEDIA alone (audio-only master with alternate renditions,
        // no #EXT-X-STREAM-INF e.g. malformed) still signals master.
        let content = "\
#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",URI=\"aud/en.m3u8\"
";
        assert_eq!(detect_m3u8_kind(content), Ok(M3u8Kind::MasterPlaylist));
    }
}
