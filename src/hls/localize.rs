use crate::hls::types::{EncryptionKey, MediaPlaylist, Segment};
use crate::utils::number::{count_digits, format_duration, pad_with_zeros};
use std::collections::HashMap;
use std::fmt::Write as _;

const MIN_RESOURCE_INDEX_WIDTH: u32 = 3;
const SEGMENT_INDEX_WIDTH: usize = 8;
const DEFAULT_SEGMENT_EXTENSION: &str = "ts";

pub struct RenderedLocalizedPlaylist {
    pub content: String,
    pub keys: HashMap<String, String>,
    pub maps: HashMap<String, String>,
    // (name, url)
    pub segments: Vec<(String, String)>,
}

pub fn render_localized_playlist(playlist: &MediaPlaylist) -> RenderedLocalizedPlaylist {
    let mut out = String::new();
    write_header(&mut out, playlist);

    let resource_width = resource_index_width(playlist.segments.len());
    let mut keys = LocalNamePool::new("key", "bin", resource_width);
    let mut maps = LocalNamePool::new("init", "mp4", resource_width);

    // Track the most recently emitted EXT-X-KEY / EXT-X-MAP so we only
    // re-emit the tag when it actually changes between segments.
    let mut last_key: Option<&EncryptionKey> = None;
    let mut key_reset_emitted = false; // tracks an explicit METHOD=NONE reset
    let mut last_map_url: Option<&str> = None;

    let mut segments = Vec::with_capacity(playlist.segments.len());

    for segment in &playlist.segments {
        write_key_tag_if_changed(&mut out, segment, &mut last_key, &mut key_reset_emitted, &mut keys);
        write_map_tag_if_changed(&mut out, segment, &mut last_map_url, &mut maps);
        write_segment_tags(&mut out, segment);

        let local_name = local_segment_filename(segment);
        out.push_str(&local_name);
        out.push('\n');
        segments.push((local_name, segment.url.clone()));
    }

    if playlist.is_complete {
        out.push_str("#EXT-X-ENDLIST\n");
    }

    RenderedLocalizedPlaylist {
        content: out,
        keys: keys.into_reverse_map(),
        maps: maps.into_reverse_map(),
        segments,
    }
}

fn resource_index_width(segments_count: usize) -> usize {
    count_digits(segments_count as i64).max(MIN_RESOURCE_INDEX_WIDTH) as usize
}

fn write_header(out: &mut String, playlist: &MediaPlaylist) {
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
}

fn write_segment_tags(out: &mut String, segment: &Segment) {
    if segment.discontinuity {
        out.push_str("#EXT-X-DISCONTINUITY\n");
    }
    if let Some(pdt) = &segment.program_date_time {
        let _ = writeln!(out, "#EXT-X-PROGRAM-DATE-TIME:{}", pdt);
    }
    if let Some(byte_range) = &segment.byte_range {
        let _ = writeln!(out, "#EXT-X-BYTERANGE:{}@{}", byte_range.length, byte_range.offset);
    }
    match &segment.title {
        Some(title) => {
            let _ = writeln!(out, "#EXTINF:{},{}", format_duration(segment.duration), title);
        }
        None => {
            let _ = writeln!(out, "#EXTINF:{},", format_duration(segment.duration));
        }
    }
}

fn local_segment_filename(segment: &Segment) -> String {
    let index = pad_with_zeros(SEGMENT_INDEX_WIDTH, segment.sequence);
    format!("seg_{}.{}", index, segment_extension(&segment.url))
}

fn segment_extension(url_str: &str) -> String {
    if let Ok(url) = reqwest::Url::parse(url_str) {
        if let Some(ext) = url
            .path_segments()
            .and_then(|mut segs| segs.next_back())
            .and_then(extension_of)
        {
            return ext;
        }
    } else {
        let path_only = url_str.split(['?', '#']).next().unwrap_or(url_str);
        if let Some(ext) = extension_of(path_only) {
            return ext;
        }
    }
    DEFAULT_SEGMENT_EXTENSION.to_string()
}

fn extension_of(segment: &str) -> Option<String> {
    let basename = segment.rsplit('/').next().unwrap_or(segment);
    basename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_string())
        .filter(|ext| !ext.is_empty())
}

struct LocalNamePool {
    names: HashMap<String, String>, // original URL -> local name
    prefix: &'static str,
    extension: &'static str,
    width: usize,
}

impl LocalNamePool {
    fn new(prefix: &'static str, extension: &'static str, width: usize) -> Self {
        Self { names: HashMap::new(), prefix, extension, width }
    }

    fn resolve(&mut self, url: &str) -> String {
        if let Some(existing) = self.names.get(url) {
            return existing.clone();
        }
        let index = self.names.len() as u64;
        let name = format!("{}_{}.{}", self.prefix, pad_with_zeros(self.width, index), self.extension);
        self.names.insert(url.to_string(), name.clone());
        name
    }

    fn into_reverse_map(self) -> HashMap<String, String> {
        self.names.into_iter().map(|(url, name)| (name, url)).collect()
    }
}

fn write_key_tag_if_changed<'a>(
    out: &mut String,
    segment: &'a Segment,
    last_key: &mut Option<&'a EncryptionKey>,
    key_reset_emitted: &mut bool,
    key_names: &mut LocalNamePool,
) {
    match &segment.key {
        None => {
            // Only emit METHOD=NONE if a key was previously active and we
            // haven't already reset it, so we don't spam it before the
            // first key appears.
            if last_key.is_some() && !*key_reset_emitted {
                out.push_str("#EXT-X-KEY:METHOD=NONE\n");
                *key_reset_emitted = true;
                *last_key = None;
            }
        }
        Some(key) => {
            let changed = last_key.map_or(true, |prev| prev != key);
            if !changed {
                return;
            }
            let mut attrs = format!("METHOD={}", key.method);
            if let Some(uri) = &key.uri {
                let local_name = key_names.resolve(uri);
                let _ = write!(attrs, ",URI=\"{}\"", local_name);
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
            *key_reset_emitted = false;
        }
    }
}

fn write_map_tag_if_changed<'a>(
    out: &mut String,
    segment: &'a Segment,
    last_map_url: &mut Option<&'a str>,
    map_names: &mut LocalNamePool,
) {
    let Some(map) = &segment.map else { return };
    let changed = last_map_url.map_or(true, |prev| prev != map.url);
    if !changed {
        return;
    }
    let local_name = map_names.resolve(&map.url);
    let mut attrs = format!("URI=\"{}\"", local_name);
    if let Some(br) = &map.byte_range {
        let _ = write!(attrs, ",BYTERANGE=\"{}@{}\"", br.length, br.offset);
    }
    let _ = writeln!(out, "#EXT-X-MAP:{}", attrs);
    *last_map_url = Some(&map.url);
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
        let playlist =
            parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
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
        let playlist =
            parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("#EXT-X-KEY:METHOD=AES-128"));
        assert!(rendered.content.contains("URI=\"key_000.bin\""));
        assert!(
            rendered
                .content
                .contains("IV=0x00000000000000000000000000000001")
        );
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
        let playlist =
            parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
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
        let playlist =
            parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        // seg0 should be preceded by the AES-128 key tag; seg1 should be
        // preceded by an explicit METHOD=NONE reset since the source
        // playlist explicitly turned encryption off mid-stream.
        let key_pos = rendered
            .content
            .find("#EXT-X-KEY:METHOD=AES-128")
            .expect("expected key tag");
        let none_pos = rendered
            .content
            .find("#EXT-X-KEY:METHOD=NONE")
            .expect("expected none tag");
        let seg0_pos = rendered
            .content
            .find("seg_00000000.ts")
            .expect("expected seg0");
        let seg1_pos = rendered
            .content
            .find("seg_00000001.ts")
            .expect("expected seg1");

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
        let playlist =
            parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        assert!(rendered.content.contains("key_000.bin"));
        assert!(rendered.content.contains("key_001.bin"));
        let occurrences = rendered
            .content
            .matches("#EXT-X-KEY:METHOD=AES-128")
            .count();
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
        let playlist =
            parse_media_playlist(input, Some("https://cdn.example.com/video/index.m3u8"));
        let rendered = render_localized_playlist(&playlist);

        assert!(
            rendered
                .content
                .contains("#EXT-X-MAP:URI=\"init_000.mp4\",BYTERANGE=\"512@0\"")
        );
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

        assert!(
            rendered
                .content
                .contains("#EXT-X-PROGRAM-DATE-TIME:2024-01-01T00:00:00.000Z")
        );
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
