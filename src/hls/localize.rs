// File: src/hls/localize.rs
//
// Rewrites a parsed `MediaPlaylist` back into m3u8 text suitable for local
// playback, after segments / init maps / keys have been downloaded to disk
// next to the playlist file.
//
// What gets rewritten vs. left alone:
//   - Segment URLs            -> local relative filenames
//   - EXT-X-MAP URI           -> local relative filename
//   - EXT-X-KEY URI           -> local relative filename (skipped for
//                                METHOD=NONE, since there's no URI at all)
//   - Byte ranges             -> left as-is (still valid against the local file)
//   - Durations / sequence /
//     discontinuity / PDT /
//     target duration / etc. -> left as-is, not a remote reference
//
// Naming is deterministic and based on the segment's `sequence` number, so
// re-running localization on the same playlist is stable and idempotent.

use crate::hls::types::{EncryptionKey, MediaPlaylist, Segment};
use crate::utils::number::{count_digits, pad_with_zeros};
use crate::utils::url::filename_from_url;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::exit;

pub struct RenderedLocalizedPlaylist {
    pub content: String,
    pub keys: HashMap<String, String>,
    pub maps: HashMap<String, String>,
    // (name, url)
    pub segments: Vec<(String, String)>,
}

/// Local filenames chosen for one occurrence of EXT-X-KEY's URI.
/// Keyed by the original remote key URI so the same key is only
/// "downloaded"/named once even if it repeats across many segments
/// (e.g. the common case of one key for the whole playlist).
fn local_key_filename(segments_count: usize, index: usize) -> String {
    // format!("key_{:03}.bin", index)
    format!("key_{}.bin", pad_with_zeros(count_digits(segments_count as i64) as usize, index as u64))
}

fn local_map_filename(segments_count: usize, index: usize) -> String {
    format!("init_{}.mp4", pad_with_zeros(count_digits(segments_count as i64) as usize, index as u64))
}

/// Renders a `MediaPlaylist` as m3u8 text with all remote URIs (segments,
/// EXT-X-MAP, EXT-X-KEY) rewritten to local relative filenames, as if every
/// referenced resource had been downloaded into the same directory as the
/// playlist file itself.
///
/// Returns the rendered playlist text. Does not touch the filesystem and
/// does not download anything — callers are expected to have already
/// fetched each resource and saved it under the same filename this function
/// produces (see `local_*_filename` helpers above), so the two stay in
/// sync.
pub fn render_localized_playlist(playlist: &MediaPlaylist) -> RenderedLocalizedPlaylist {
    let mut out = String::new();

    out.push_str("#EXTM3U\n");
    if let Some(version) = playlist.version {
        let _ = writeln!(out, "#EXT-X-VERSION:{}", version);
    }
    if let Some(target_duration) = playlist.target_duration {
        let _ = writeln!(out, "#EXT-X-TARGETDURATION:{}", target_duration);
    }
    let _ = writeln!(out, "#EXT-X-MEDIA-SEQUENCE:{}", playlist.media_sequence);
    if let Some(playlist_type) = &playlist.playlist_type {
        let _ = writeln!(out, "#EXT-X-PLAYLIST-TYPE:{}", playlist_type);
    }
    if playlist.iframes_only {
        out.push_str("#EXT-X-I-FRAMES-ONLY\n");
    }

    // Track the most recently emitted EXT-X-KEY / EXT-X-MAP so we only
    // re-emit the tag when it actually changes between segments, exactly
    // like a normal m3u8 writer would (and the original parser expects).
    let mut last_key: Option<&EncryptionKey> = None;
    let mut last_key_is_none = false; // tracks an explicit METHOD=NONE reset
    let mut last_map_url: Option<&str> = None;

    // Stable per-URI local names so repeated keys/maps map to the same
    // local file instead of being renamed every time they recur.
    let mut key_names: HashMap<String, String> = HashMap::new();
    let mut map_names: HashMap<String, String> = HashMap::new();
    let segments_count = &playlist.segments.len();

    let mut keys: HashMap<String, String> = HashMap::new();
    let mut maps: HashMap<String, String> = HashMap::new();
    let mut segs: Vec<(String, String)> = vec![];

    for segment in &playlist.segments {
        write_key_tag_if_changed(&mut out, segment, &mut last_key, &mut last_key_is_none, &mut key_names, *segments_count, &mut keys);
        write_map_tag_if_changed(&mut out, segment, &mut last_map_url, &mut map_names, *segments_count, &mut maps);

        if segment.discontinuity {
            out.push_str("#EXT-X-DISCONTINUITY\n");
        }

        if let Some(pdt) = &segment.program_date_time {
            let _ = writeln!(out, "#EXT-X-PROGRAM-DATE-TIME:{}", pdt);
        }

        if let Some(byte_range) = &segment.byte_range {
            let _ = writeln!(
                out,
                "#EXT-X-BYTERANGE:{}@{}",
                byte_range.length, byte_range.offset
            );
        }

        match &segment.title {
            Some(title) => {
                let _ = writeln!(out, "#EXTINF:{},{}", format_duration(segment.duration), title);
            }
            None => {
                let _ = writeln!(out, "#EXTINF:{},", format_duration(segment.duration));
            }
        }

        let up = reqwest::Url::parse(&segment.url).unwrap();
        let local_name = filename_from_url(&up).unwrap();
        out.push_str(&local_name);
        out.push('\n');

        segs.push((local_name, segment.url.to_owned()));
    }

    if playlist.is_complete {
        out.push_str("#EXT-X-ENDLIST\n");
    }

    RenderedLocalizedPlaylist {
        content: out,
        keys: keys,
        maps: maps,
        segments: segs,
    }
}

fn write_key_tag_if_changed<'a>(
    out: &mut String,
    segment: &'a Segment,
    last_key: &mut Option<&'a EncryptionKey>,
    last_key_is_none: &mut bool,
    key_names: &mut HashMap<String, String>,
    segments_count: usize,
    keys: &mut HashMap<String, String>,
) {
    match &segment.key {
        None => {
            // No active key for this segment. Only emit METHOD=NONE if we
            // previously had a key active and haven't already reset it,
            // so we don't spam METHOD=NONE before the first key appears.
            if last_key.is_some() && !*last_key_is_none {
                out.push_str("#EXT-X-KEY:METHOD=NONE\n");
                *last_key_is_none = true;
                *last_key = None;
            }
        }
        Some(key) => {
            let changed = match last_key {
                Some(prev) => *prev != key,
                None => true,
            };
            if changed {
                let mut attrs = format!("METHOD={}", key.method);
                if let Some(uri) = &key.uri {
                    let local_name = key_names.clone()
                        .entry(uri.clone())
                        .or_insert_with(|| local_key_filename(segments_count ,key_names.len()))
                        .clone();
                    let _ = write!(attrs, ",URI=\"{}\"", local_name);
                    let _ = &keys.insert(local_name, uri.clone());
                }
                if let Some(iv) = &key.iv {
                    let _ = write!(attrs, ",IV={}", iv);
                }
                if let Some(kf) = &key.key_format {
                    let _ = write!(attrs, ",KEYFORMAT=\"{}\"", kf);
                }
                if let Some(kfv) = &key.key_format_versions {
                    let _ = write!(attrs, ",KEYFORMATVERSIONS=\"{}\"", kfv);
                }
                let _ = writeln!(out, "#EXT-X-KEY:{}", attrs);
                *last_key = Some(key);
                *last_key_is_none = false;
            }
        }
    }
}

fn write_map_tag_if_changed<'a>(
    out: &mut String,
    segment: &'a Segment,
    last_map_url: &mut Option<&'a str>,
    map_names: &mut HashMap<String, String>,
    segments_count: usize,
    maps: &mut HashMap<String, String>,
) {
    if let Some(map) = &segment.map {
        let changed = match last_map_url {
            Some(prev) => *prev != map.url.as_str(),
            None => true,
        };
        if changed {
            let local_name = map_names.clone()
                .entry(map.url.clone())
                .or_insert_with(|| local_map_filename(segments_count, map_names.len()))
                .clone();
            let mut attrs = format!("URI=\"{}\"", local_name);

            if let Some(br) = &map.byte_range {
                let _ = write!(attrs, ",BYTERANGE=\"{}@{}\"", br.length, br.offset);
            }
            let _ = writeln!(out, "#EXT-X-MAP:{}", attrs);
            *last_map_url = Some(map.url.as_str());

            let _ = &maps.insert(local_name, map.url.clone());
        }
    }
}

/// Formats a duration the way m3u8 expects: as compact a decimal as
/// possible without losing precision, matching common encoder output
/// (e.g. `9.009` rather than `9.00900000000001`).
fn format_duration(duration: f64) -> String {
    let rounded = (duration * 1000.0).round() / 1000.0;
    if (rounded.fract()).abs() < f64::EPSILON {
        format!("{:.0}", rounded)
    } else {
        let mut s = format!("{:.3}", rounded);
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::media::parse_media_playlist;

    #[test]
    fn rewrites_segment_urls_to_local_filenames() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:9.009,
https://cdn.example.com/video/seg0.ts
#EXTINF:9.009,
https://cdn.example.com/video/seg1.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("seg_00000000.ts"));
        assert!(rendered.content.contains("seg_00000001.ts"));
        assert!(!rendered.content.contains("cdn.example.com"));
        assert!(rendered.content.trim_end().ends_with("#EXT-X-ENDLIST"));
    }

    #[test]
    fn rewrites_key_uri_and_preserves_other_key_attrs() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-KEY:METHOD=AES-128,URI=\"https://cdn.example.com/key.bin\",IV=0x00000000000000000000000000000001
#EXTINF:9.009,
https://cdn.example.com/video/seg0.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("#EXT-X-KEY:METHOD=AES-128"));
        assert!(rendered.content.contains("URI=\"key_000.bin\""));
        assert!(rendered.content.contains("IV=0x00000000000000000000000000000001"));
        assert!(!rendered.content.contains("cdn.example.com"));
    }

    #[test]
    fn does_not_emit_key_tag_when_no_key_present() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:9.009,
seg0.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, None);
        let rendered = render_localized_playlist(&playlist);

        assert!(!rendered.content.contains("EXT-X-KEY"));
    }

    #[test]
    fn reuses_same_local_filename_for_repeated_key_across_segments() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-KEY:METHOD=AES-128,URI=\"https://cdn.example.com/key.bin\"
#EXTINF:9.009,
seg0.ts
#EXTINF:9.009,
seg1.ts
#EXTINF:9.009,
seg2.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        // The key tag (and therefore the local filename it assigns) should
        // only be emitted once, immediately before the first segment,
        // since it never changes across segments 0..2.
        let occurrences = rendered.content.matches("#EXT-X-KEY").count();
        assert_eq!(occurrences, 1);
        assert!(rendered.content.contains("key_000.bin"));
    }

    #[test]
    fn emits_method_none_when_key_rotation_turns_encryption_off() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-KEY:METHOD=AES-128,URI=\"https://cdn.example.com/key.bin\"
#EXTINF:9.009,
seg0.ts
#EXT-X-KEY:METHOD=NONE
#EXTINF:9.009,
seg1.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        // seg0 should be preceded by the AES-128 key tag; seg1 should be
        // preceded by an explicit METHOD=NONE reset since the source
        // playlist explicitly turned encryption off mid-stream.
        let key_pos = rendered.content.find("#EXT-X-KEY:METHOD=AES-128").expect("expected key tag");
        let none_pos = rendered.content.find("#EXT-X-KEY:METHOD=NONE").expect("expected none tag");
        let seg0_pos = rendered.content.find("seg_00000000.ts").expect("expected seg0");
        let seg1_pos = rendered.content.find("seg_00000001.ts").expect("expected seg1");

        assert!(key_pos < seg0_pos);
        assert!(seg0_pos < none_pos);
        assert!(none_pos < seg1_pos);
    }

    #[test]
    fn handles_key_rotation_with_two_distinct_keys() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-KEY:METHOD=AES-128,URI=\"https://cdn.example.com/key1.bin\"
#EXTINF:9.009,
seg0.ts
#EXT-X-KEY:METHOD=AES-128,URI=\"https://cdn.example.com/key2.bin\"
#EXTINF:9.009,
seg1.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("key_000.bin"));
        assert!(rendered.content.contains("key_001.bin"));
        let occurrences = rendered.content.matches("#EXT-X-KEY:METHOD=AES-128").count();
        assert_eq!(occurrences, 2);
    }

    #[test]
    fn rewrites_map_uri_and_preserves_byte_range() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-MAP:URI=\"https://cdn.example.com/init.mp4\",BYTERANGE=\"512@0\"
#EXTINF:9.009,
seg0.mp4
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("#EXT-X-MAP:URI=\"init_000.mp4\",BYTERANGE=\"512@0\""));
        assert!(!rendered.content.contains("cdn.example.com"));
    }

    #[test]
    fn preserves_segment_byte_ranges_unchanged() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-BYTERANGE:1000@0
#EXTINF:9.009,
seg0.ts
#EXT-X-BYTERANGE:2000@1000
#EXTINF:9.009,
seg0.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, None);
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("#EXT-X-BYTERANGE:1000@0"));
        assert!(rendered.content.contains("#EXT-X-BYTERANGE:2000@1000"));
    }

    #[test]
    fn preserves_discontinuity_and_program_date_time() {
        let input = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-PROGRAM-DATE-TIME:2024-01-01T00:00:00.000Z
#EXTINF:9.009,
seg0.ts
#EXT-X-DISCONTINUITY
#EXTINF:9.009,
seg1.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, None);
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("#EXT-X-PROGRAM-DATE-TIME:2024-01-01T00:00:00.000Z"));
        assert!(rendered.content.contains("#EXT-X-DISCONTINUITY"));
    }

    #[test]
    fn preserves_playlist_level_metadata() {
        let input = "\
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:6
#EXT-X-MEDIA-SEQUENCE:3
#EXT-X-PLAYLIST-TYPE:VOD
#EXTINF:6.0,
seg3.ts
#EXT-X-ENDLIST
";
        let playlist = parse_media_playlist(input, None);
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("#EXT-X-VERSION:7"));
        assert!(rendered.content.contains("#EXT-X-TARGETDURATION:6"));
        assert!(rendered.content.contains("#EXT-X-MEDIA-SEQUENCE:3"));
        assert!(rendered.content.contains("#EXT-X-PLAYLIST-TYPE:VOD"));
        assert!(rendered.content.contains("#EXT-X-ENDLIST"));
    }
}