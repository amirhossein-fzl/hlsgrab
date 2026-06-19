use crate::hls::attrs::AttributeList;
use crate::hls::types::{MasterPlaylist, MediaRendition, MediaType, Resolution, VariantStream};
use crate::hls::url::resolve;

/// Parses the text of a master playlist.
///
/// `base_url` should be the URL (or filesystem path, using `/`
/// separators) the playlist was fetched from, if known. It's used to
/// resolve the relative URIs that HLS playlists almost always use for
/// their variants/renditions. Pass `None` to keep URIs exactly as
/// written in the file (useful if you already know the base directory
/// and will join paths yourself).
pub fn parse_master_playlist(content: &str, base_url: Option<&str>) -> MasterPlaylist {
    let mut playlist = MasterPlaylist::default();

    let lines: Vec<&str> = content.lines().map(str::trim).collect();
    let mut i = 0usize;

    let mut pending_stream_attrs: Option<(AttributeList, bool)> = None;

    while i < lines.len() {
        let line = lines[i];

        if line.is_empty() {
            i += 1;
            continue;
        }

        if line == "#EXT-X-INDEPENDENT-SEGMENTS" {
            playlist.independent_segments = true;
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending_stream_attrs = Some((AttributeList::parse(rest), false));
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-I-FRAME-STREAM-INF:") {
            let attrs = AttributeList::parse(rest);
            if let Some(uri) = attrs.get("URI") {
                playlist
                    .variants
                    .push(variant_from_attrs(&attrs, uri, base_url, true));
            }
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = AttributeList::parse(rest);
            if let Some(rendition) = rendition_from_attrs(&attrs, base_url) {
                playlist.renditions.push(rendition);
            }
            i += 1;
            continue;
        }

        if !line.starts_with('#') {
            if let Some((attrs, _is_iframe)) = pending_stream_attrs.take() {
                playlist
                    .variants
                    .push(variant_from_attrs(&attrs, line, base_url, false));
            }
            i += 1;
            continue;
        }

        i += 1;
    }

    playlist
}

fn variant_from_attrs(
    attrs: &AttributeList,
    uri: &str,
    base_url: Option<&str>,
    is_iframe_stream: bool,
) -> VariantStream {
    let resolution = attrs
        .get_resolution("RESOLUTION")
        .map(|(width, height)| Resolution { width, height });

    VariantStream {
        url: resolve(base_url, uri),
        bandwidth: attrs.get_u64("BANDWIDTH"),
        average_bandwidth: attrs.get_u64("AVERAGE-BANDWIDTH"),
        resolution,
        codecs: attrs.get("CODECS").map(str::to_string),
        frame_rate: attrs.get_f64("FRAME-RATE"),
        video_range: attrs.get("VIDEO-RANGE").map(str::to_string),
        audio_group: attrs.get("AUDIO").map(str::to_string),
        subtitles_group: attrs.get("SUBTITLES").map(str::to_string),
        video_group: attrs.get("VIDEO").map(str::to_string),
        is_iframe_stream,
    }
}

fn rendition_from_attrs(attrs: &AttributeList, base_url: Option<&str>) -> Option<MediaRendition> {
    let media_type = match attrs.get("TYPE")? {
        "AUDIO" => MediaType::Audio,
        "VIDEO" => MediaType::Video,
        "SUBTITLES" => MediaType::Subtitles,
        "CLOSED-CAPTIONS" => MediaType::ClosedCaptions,
        _ => return None,
    };

    let group_id = attrs.get("GROUP-ID")?.to_string();
    let name = attrs.get("NAME")?.to_string();

    Some(MediaRendition {
        media_type,
        group_id,
        name,
        language: attrs.get("LANGUAGE").map(str::to_string),
        url: attrs.get("URI").map(|u| resolve(base_url, u)),
        is_default: attrs.get("DEFAULT") == Some("YES"),
        autoselect: attrs.get("AUTOSELECT") == Some("YES"),
        forced: attrs.get("FORCED") == Some("YES"),
        channels: attrs.get("CHANNELS").map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
#EXTM3U
#EXT-X-VERSION:6
#EXT-X-INDEPENDENT-SEGMENTS
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",LANGUAGE=\"en\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"2\",URI=\"audio/en/index.m3u8\"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"English\",LANGUAGE=\"en\",URI=\"subs/en/index.m3u8\"
#EXT-X-STREAM-INF:BANDWIDTH=5000000,AVERAGE-BANDWIDTH=4500000,RESOLUTION=1920x1080,FRAME-RATE=29.970,CODECS=\"avc1.640028,mp4a.40.2\",AUDIO=\"aud\",SUBTITLES=\"subs\"
1080p/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS=\"avc1.64001f,mp4a.40.2\",AUDIO=\"aud\"
720p/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.64000d,mp4a.40.2\",AUDIO=\"aud\"
360p/index.m3u8
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=120000,RESOLUTION=1920x1080,CODECS=\"avc1.640028\",URI=\"1080p/iframes.m3u8\"
";

    #[test]
    fn parses_variants_with_resolution_and_url() {
        let playlist =
            parse_master_playlist(SAMPLE, Some("https://cdn.example.com/show/master.m3u8"));

        let regular: Vec<_> = playlist
            .variants
            .iter()
            .filter(|v| !v.is_iframe_stream)
            .collect();
        assert_eq!(regular.len(), 3);

        let v1080 = regular
            .iter()
            .find(|v| {
                v.resolution
                    == Some(Resolution {
                        width: 1920,
                        height: 1080,
                    })
            })
            .expect("1080p variant present");
        assert_eq!(v1080.bandwidth, Some(5_000_000));
        assert_eq!(v1080.url, "https://cdn.example.com/show/1080p/index.m3u8");
        assert_eq!(v1080.audio_group.as_deref(), Some("aud"));

        let v720 = regular
            .iter()
            .find(|v| {
                v.resolution
                    == Some(Resolution {
                        width: 1280,
                        height: 720,
                    })
            })
            .expect("720p variant present");
        assert_eq!(v720.url, "https://cdn.example.com/show/720p/index.m3u8");
    }

    #[test]
    fn parses_iframe_stream_separately() {
        let playlist = parse_master_playlist(SAMPLE, None);
        let iframe = playlist
            .variants
            .iter()
            .find(|v| v.is_iframe_stream)
            .expect("iframe stream present");
        assert_eq!(iframe.url, "1080p/iframes.m3u8");
        assert_eq!(iframe.bandwidth, Some(120_000));
    }

    #[test]
    fn parses_renditions() {
        let playlist =
            parse_master_playlist(SAMPLE, Some("https://cdn.example.com/show/master.m3u8"));
        assert_eq!(playlist.renditions.len(), 2);

        let audio = playlist
            .renditions
            .iter()
            .find(|r| r.media_type == MediaType::Audio)
            .unwrap();
        assert_eq!(audio.group_id, "aud");
        assert!(audio.is_default);
        assert_eq!(
            audio.url.as_deref(),
            Some("https://cdn.example.com/show/audio/en/index.m3u8")
        );
        assert_eq!(audio.channels.as_deref(), Some("2"));
    }

    #[test]
    fn variants_by_quality_desc_orders_best_first() {
        let playlist = parse_master_playlist(SAMPLE, None);
        let sorted = playlist.variants_by_quality_desc();
        assert_eq!(sorted[0].bandwidth, Some(5_000_000));
        assert_eq!(sorted.last().unwrap().bandwidth, Some(800_000));
        // iframe stream must be excluded from quality ranking
        assert!(sorted.iter().all(|v| !v.is_iframe_stream));
    }

    #[test]
    fn audio_only_master_without_resolution() {
        let content = "\
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2\"
audio-only/index.m3u8
";
        let playlist = parse_master_playlist(content, None);
        assert_eq!(playlist.variants.len(), 1);
        assert_eq!(playlist.variants[0].resolution, None);
        assert_eq!(playlist.variants[0].bandwidth, Some(128_000));
    }
}
