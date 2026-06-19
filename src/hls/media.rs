use crate::hls::attrs::AttributeList;
use crate::hls::types::{ByteRange, EncryptionKey, InitializationSegment, MediaPlaylist, Segment};
use crate::hls::url::resolve;

/// Parses the text of a media playlist.
///
/// `base_url` should be the URL (or filesystem path) the playlist was
/// fetched from, if known, used to resolve relative segment/key/map
/// URIs. Pass `None` to keep URIs exactly as written in the file.
pub fn parse_media_playlist(content: &str, base_url: Option<&str>) -> MediaPlaylist {
    let mut playlist = MediaPlaylist::default();

    let lines: Vec<&str> = content.lines().map(str::trim).collect();
    let mut i = 0usize;

    let mut sequence = 0u64;
    let mut media_sequence_set = false;

    // State carried forward across segments until replaced.
    let mut current_key: Option<EncryptionKey> = None;
    let mut current_map: Option<InitializationSegment> = None;
    // End-byte (offset + length) of the most recent segment's byte range,
    // used to default the offset of the next BYTERANGE that omits one.
    let mut last_byte_range_end: u64 = 0;

    // Per-segment pending state, reset after each URI line is consumed.
    let mut pending_duration: Option<f64> = None;
    let mut pending_title: Option<String> = None;
    let mut pending_byte_range: Option<ByteRange> = None;
    let mut pending_discontinuity = false;
    let mut pending_pdt: Option<String> = None;

    while i < lines.len() {
        let line = lines[i];
        i += 1;

        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            playlist.target_duration = rest.trim().parse().ok();
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            if let Ok(n) = rest.trim().parse::<u64>() {
                sequence = n;
                playlist.media_sequence = n;
                media_sequence_set = true;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-VERSION:") {
            playlist.version = rest.trim().parse().ok();
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-PLAYLIST-TYPE:") {
            playlist.playlist_type = Some(rest.trim().to_string());
            continue;
        }

        if line == "#EXT-X-ENDLIST" {
            playlist.is_complete = true;
            continue;
        }

        if line == "#EXT-X-I-FRAMES-ONLY" {
            playlist.iframes_only = true;
            continue;
        }

        if line == "#EXT-X-DISCONTINUITY" {
            pending_discontinuity = true;
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            pending_pdt = Some(rest.trim().to_string());
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (dur_str, title) = match rest.split_once(',') {
                Some((d, t)) => (
                    d,
                    if t.is_empty() {
                        None
                    } else {
                        Some(t.to_string())
                    },
                ),
                None => (rest, None),
            };
            pending_duration = dur_str.trim().parse().ok();
            pending_title = title;
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            pending_byte_range = parse_byte_range(rest.trim(), last_byte_range_end);
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            let attrs = AttributeList::parse(rest);
            current_key = key_from_attrs(&attrs, base_url);
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attrs = AttributeList::parse(rest);
            current_map = map_from_attrs(&attrs, base_url, last_byte_range_end);
            continue;
        }

        // A non-comment line is a segment URI, terminating the current
        // EXTINF/BYTERANGE/etc. group.
        if !line.starts_with('#') {
            let duration = pending_duration.take().unwrap_or(0.0);

            if let Some(br) = pending_byte_range {
                last_byte_range_end = br.offset + br.length;
            }

            playlist.segments.push(Segment {
                url: resolve(base_url, line),
                duration,
                title: pending_title.take(),
                byte_range: pending_byte_range.take(),
                key: current_key.clone(),
                map: current_map.clone(),
                discontinuity: pending_discontinuity,
                program_date_time: pending_pdt.take(),
                sequence,
            });

            sequence += 1;
            pending_discontinuity = false;
            continue;
        }

        // Any other tag not needed for segment extraction
        // (EXT-X-DISCONTINUITY-SEQUENCE, EXT-X-START, EXT-X-DEFINE,
        // EXT-X-GAP, comments, vendor extensions, etc.) — skip.
    }

    if !media_sequence_set {
        playlist.media_sequence = 0;
    }

    playlist
}

/// Parses `#EXT-X-BYTERANGE:<n>[@<o>]`. If the offset is omitted, it
/// defaults to `previous_end` (the end of the prior byte-range segment),
/// per RFC 8216 §4.3.2.2.
fn parse_byte_range(raw: &str, previous_end: u64) -> Option<ByteRange> {
    let (len_str, offset_str) = match raw.split_once('@') {
        Some((l, o)) => (l, Some(o)),
        None => (raw, None),
    };
    let length: u64 = len_str.trim().parse().ok()?;
    let offset: u64 = match offset_str {
        Some(o) => o.trim().parse().ok()?,
        None => previous_end,
    };
    Some(ByteRange { length, offset })
}

fn key_from_attrs(attrs: &AttributeList, base_url: Option<&str>) -> Option<EncryptionKey> {
    let method = attrs.get("METHOD")?.to_string();
    if method == "NONE" {
        // An explicit "no encryption from here on" marker; normalize to
        // None so callers don't need to special-case the string "NONE".
        return None;
    }
    Some(EncryptionKey {
        method,
        uri: attrs.get("URI").map(|u| resolve(base_url, u)),
        iv: attrs.get("IV").map(str::to_string),
        key_format: attrs.get("KEYFORMAT").map(str::to_string),
        key_format_versions: attrs.get("KEYFORMATVERSIONS").map(str::to_string),
    })
}

fn map_from_attrs(
    attrs: &AttributeList,
    base_url: Option<&str>,
    last_byte_range_end: u64,
) -> Option<InitializationSegment> {
    let uri = attrs.get("URI")?;
    let byte_range = attrs
        .get("BYTERANGE")
        .and_then(|raw| parse_byte_range(raw, last_byte_range_end));
    Some(InitializationSegment {
        url: resolve(base_url, uri),
        byte_range,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_vod_segments() {
        let content = "\
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-PLAYLIST-TYPE:VOD
#EXTINF:9.009,
segment0.ts
#EXTINF:9.009,
segment1.ts
#EXTINF:3.003,
segment2.ts
#EXT-X-ENDLIST
";
        let playlist =
            parse_media_playlist(content, Some("https://cdn.example.com/720p/index.m3u8"));

        assert_eq!(playlist.segments.len(), 3);
        assert!(playlist.is_complete);
        assert_eq!(playlist.playlist_type.as_deref(), Some("VOD"));
        assert_eq!(playlist.target_duration, Some(10));

        assert_eq!(
            playlist.segments[0].url,
            "https://cdn.example.com/720p/segment0.ts"
        );
        assert_eq!(playlist.segments[0].duration, 9.009);
        assert_eq!(playlist.segments[0].sequence, 0);
        assert_eq!(playlist.segments[2].sequence, 2);

        let total = playlist.total_duration();
        assert!((total - 21.021).abs() < 1e-9);
    }

    #[test]
    fn carries_forward_encryption_key_across_segments() {
        let content = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\",IV=0x0123456789abcdef0123456789abcdef
#EXTINF:10.0,
seg0.ts
#EXTINF:10.0,
seg1.ts
#EXT-X-KEY:METHOD=NONE
#EXTINF:10.0,
seg2.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(content, Some("https://cdn.example.com/index.m3u8"));
        assert_eq!(playlist.segments.len(), 3);

        let k0 = playlist.segments[0].key.as_ref().expect("seg0 encrypted");
        assert_eq!(k0.method, "AES-128");
        assert_eq!(k0.uri.as_deref(), Some("https://cdn.example.com/key.bin"));

        let k1 = playlist.segments[1]
            .key
            .as_ref()
            .expect("seg1 still encrypted");
        assert_eq!(k1.method, "AES-128");

        assert!(
            playlist.segments[2].key.is_none(),
            "METHOD=NONE clears the key"
        );
    }

    #[test]
    fn handles_fmp4_with_map_and_byteranges() {
        let content = "\
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:6
#EXT-X-MAP:URI=\"init.mp4\"
#EXTINF:6.0,
#EXT-X-BYTERANGE:609774@0
fileSequence0.mp4
#EXTINF:6.0,
#EXT-X-BYTERANGE:646090@609774
fileSequence1.mp4
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(content, None);
        assert_eq!(playlist.segments.len(), 2);

        let m0 = playlist.segments[0].map.as_ref().unwrap();
        assert_eq!(m0.url, "init.mp4");

        let br0 = playlist.segments[0].byte_range.unwrap();
        assert_eq!(br0.length, 609774);
        assert_eq!(br0.offset, 0);

        let br1 = playlist.segments[1].byte_range.unwrap();
        assert_eq!(br1.length, 646090);
        assert_eq!(br1.offset, 609774);
    }

    #[test]
    fn byterange_offset_defaults_to_previous_end_when_omitted() {
        // Per spec, omitting @offset means "right after the previous
        // BYTERANGE segment ended".
        let content = "\
#EXTM3U
#EXT-X-TARGETDURATION:6
#EXTINF:6.0,
#EXT-X-BYTERANGE:1000@0
seg.mp4
#EXTINF:6.0,
#EXT-X-BYTERANGE:500
seg.mp4
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(content, None);
        let br1 = playlist.segments[1].byte_range.unwrap();
        assert_eq!(br1.offset, 1000);
        assert_eq!(br1.length, 500);
    }

    #[test]
    fn marks_discontinuity_on_the_correct_segment_only() {
        let content = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXTINF:10.0,
seg0.ts
#EXT-X-DISCONTINUITY
#EXTINF:10.0,
seg1.ts
#EXTINF:10.0,
seg2.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(content, None);
        assert!(!playlist.segments[0].discontinuity);
        assert!(playlist.segments[1].discontinuity);
        assert!(!playlist.segments[2].discontinuity);
    }

    #[test]
    fn live_playlist_without_endlist_is_not_complete() {
        let content = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:55
#EXTINF:10.0,
seg55.ts
#EXTINF:10.0,
seg56.ts
";
        let playlist = parse_media_playlist(content, None);
        assert!(!playlist.is_complete);
        assert_eq!(playlist.media_sequence, 55);
        assert_eq!(playlist.segments[0].sequence, 55);
        assert_eq!(playlist.segments[1].sequence, 56);
    }

    #[test]
    fn extinf_with_title_is_captured() {
        let content = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10.0,Some title here\nseg.ts\n";
        let playlist = parse_media_playlist(content, None);
        assert_eq!(
            playlist.segments[0].title.as_deref(),
            Some("Some title here")
        );
    }

    #[test]
    fn program_date_time_is_captured_per_segment() {
        let content = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-PROGRAM-DATE-TIME:2024-01-02T03:04:05.000Z
#EXTINF:10.0,
seg0.ts
#EXTINF:10.0,
seg1.ts
";
        let playlist = parse_media_playlist(content, None);
        assert_eq!(
            playlist.segments[0].program_date_time.as_deref(),
            Some("2024-01-02T03:04:05.000Z")
        );
        assert_eq!(playlist.segments[1].program_date_time, None);
    }
}
