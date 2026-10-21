pub mod audio_codec;
pub mod audio_target;
pub mod container;
pub mod errors;
pub mod ffmpeg;
pub mod merge_request;
pub mod output_preset;
pub mod probe;
pub mod raw_stream;
pub mod runner;
pub mod stream_info;
pub mod video_codec;
pub mod video_target;

use std::path::PathBuf;

use crate::hls::types::MediaPlaylist;
use crate::merge::errors::MergeError;
use crate::merge::runner::ProcessRunner;

pub const FFMPEG_BIN: &str = "ffmpeg";
pub const FFPROBE_BIN: &str = "ffprobe";

const HLS_INPUT_FLAGS: &[&str] = &[
    "-allowed_extensions",
    "ALL",
    "-protocol_whitelist",
    "file,crypto,data",
];

const AAC_MIN_BITRATE_KBPS: u64 = 192;
const AAC_MAX_BITRATE_KBPS: u64 = 512;

/// Decides which local files to probe when ffprobe cannot read the playlist.
fn fallback_targets(playlist: &MediaPlaylist) -> Result<Vec<PathBuf>, MergeError> {
    let first_segment = &playlist.segments.first();

    if first_segment.is_none() {
        return Err(MergeError::ProbeFailed {
            reason: "playlist contains no segments".to_string(),
        });
    }

    let first_segment = first_segment.unwrap();

    if let Some(key) = &first_segment.key {
        return Err(MergeError::ProbeFailed {
            reason: format!(
                "playlist is encrypted ({}); raw segments cannot be probed",
                key.method
            ),
        });
    }

    let first_segment_path = PathBuf::from(first_segment.url.to_owned());
    let mut targets = vec![];

    if !first_segment.map.is_none() {
        targets.push(PathBuf::from(
            first_segment.map.as_ref().unwrap().url.to_owned(),
        ));
    }

    targets.push(first_segment_path);

    Ok(targets)
}

#[cfg(test)]
mod tests {
    use crate::cli::fail::OrFail;
    use crate::hls::media::parse_media_playlist;
    use crate::merge::audio_codec::{AudioCodec, parse_audio_codec};
    use crate::merge::audio_target::AudioTarget;
    use crate::merge::container::Container;
    use crate::merge::ffmpeg::{build_ffmpeg_args, validate_preset};
    use crate::merge::merge_request::MergeRequest;
use crate::merge::output_preset::{OutputPreset, preset, presets_for};
    use crate::merge::probe::{parse_ffprobe_streams, run_ffprobe};
    use crate::merge::runner::ProcessOutput;
    use crate::merge::stream_info::StreamInfo;
    use crate::merge::video_codec::{VideoCodec, parse_video_codec};
    use crate::merge::video_target::VideoTarget;

    use super::*;
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::Mutex;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    const FMP4_PLAYLIST: &str = "#EXTM3U\n\
#EXT-X-VERSION:7\n\
#EXT-X-TARGETDURATION:10\n\
#EXT-X-MEDIA-SEQUENCE:0\n\
#EXT-X-PLAYLIST-TYPE:VOD\n\
#EXT-X-MAP:URI=\"init_000.mp4\"\n\
#EXTINF:10.008,\n\
seg_00000000.m4s\n\
#EXTINF:10.008,\n\
seg_00000001.m4s\n\
#EXTINF:10.008,\n\
seg_00000002.m4s\n\
#EXT-X-ENDLIST\n";

    const TS_AES_PLAYLIST: &str = "#EXTM3U\r\n\
#EXT-X-VERSION:3\r\n\
#EXT-X-TARGETDURATION:8\r\n\
#EXT-X-MEDIA-SEQUENCE:0\r\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"key_000.bin\",IV=0x6ffcdb62469a93651f8690e100183f5e\r\n\
#EXTINF:8,\r\n\
1080p_00000.ts\r\n\
#EXTINF:8,\r\n\
1080p_00001.ts\r\n\
#EXTINF:8,\r\n\
1080p_00002.ts\r\n\
#EXT-X-ENDLIST\r\n";

    fn base() -> PathBuf {
        std::env::temp_dir().join("hls_playlist_dir")
    }

    pub fn video_audio_info() -> StreamInfo {
        StreamInfo {
            has_video: true,
            has_audio: true,
            video_codec: Some(VideoCodec::H264),
            audio_codec: Some(AudioCodec::Aac),
            ..Default::default()
        }
    }

    fn labels(presets: &[OutputPreset]) -> Vec<&'static str> {
        presets.iter().map(|p| p.label).collect()
    }

    fn request() -> MergeRequest {
        MergeRequest::builder()
            .playlist_path("playlist.m3u8")
            .output_dir("out")
            .output_filename("movie")
            .build()
            .unwrap()
    }

    fn arg_strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn contains_seq(args: &[String], seq: &[&str]) -> bool {
        args.windows(seq.len())
            .any(|w| w.iter().zip(seq).all(|(a, b)| a == b))
    }

    fn block_on<F: Future>(fut: F) -> F::Output {
        fn raw() -> RawWaker {
            fn clone(_: *const ()) -> RawWaker {
                raw()
            }
            fn noop(_: *const ()) {}
            static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        let waker = unsafe { Waker::from_raw(raw()) };
        let mut cx = Context::from_waker(&waker);
        let mut fut = Box::pin(fut);
        loop {
            if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
                return value;
            }
            std::thread::yield_now();
        }
    }

    struct StubRunner {
        code: i32,
        stdout: String,
        stderr: String,
        calls: Mutex<Vec<Vec<OsString>>>,
    }

    impl ProcessRunner for StubRunner {
        fn run<'a>(
            &'a self,
            _binary: &'static str,
            args: &'a [OsString],
        ) -> impl Future<Output = Result<ProcessOutput, MergeError>> + Send + 'a {
            async move {
                self.calls.lock().unwrap().push(args.to_vec());
                Ok(ProcessOutput {
                    code: self.code,
                    stdout: self.stdout.clone(),
                    stderr: self.stderr.clone(),
                })
            }
        }
    }

    #[test]
    fn fallback_targets_for_fmp4_are_map_then_first_segment() {
        let parsed = parse_media_playlist(
            FMP4_PLAYLIST,
            Some(
                base()
                    .as_path()
                    .to_str()
                    .or_fail("Can't convert path to str."),
            ),
        );
        let targets = fallback_targets(&parsed).unwrap();
        assert_eq!(
            targets,
            // TODO: Check base url in media playlist.
            // vec![base().join("init_000.mp4"), base().join("seg_00000000.m4s")]
            vec![
                PathBuf::from("init_000.mp4"),
                PathBuf::from("seg_00000000.m4s")
            ]
        );
    }

    #[test]
    fn fallback_refuses_encrypted_playlists() {
        let parsed = parse_media_playlist(
            TS_AES_PLAYLIST,
            Some(
                base()
                    .as_path()
                    .to_str()
                    .or_fail("Can't convert path to str."),
            ),
        );

        assert!(matches!(
            fallback_targets(&parsed),
            Err(MergeError::ProbeFailed { .. })
        ));
    }

    #[test]
    fn fallback_fails_on_empty_playlist() {
        let parsed = parse_media_playlist(
            "#EXTM3U\n",
            Some(
                base()
                    .as_path()
                    .to_str()
                    .or_fail("Can't convert path to str."),
            ),
        );

        assert!(matches!(
            fallback_targets(&parsed),
            Err(MergeError::ProbeFailed { .. })
        ));
    }

    // -- ffprobe parsing ----------------------------------------------------

    #[test]
    fn parses_ffprobe_output_into_stream_info() {
        let out = "[STREAM]\ncodec_name=hevc\ncodec_type=video\nwidth=1920\nheight=1080\nbit_rate=N/A\n[/STREAM]\n\
[STREAM]\ncodec_name=aac\ncodec_type=audio\nsample_rate=48000\nchannels=2\nbit_rate=128000\n[/STREAM]\n";
        let mut info = StreamInfo::default();
        info.absorb(&parse_ffprobe_streams(out));

        assert!(info.has_video && info.has_audio);
        assert_eq!(info.video_codec, Some(VideoCodec::H265));
        assert_eq!(info.audio_codec, Some(AudioCodec::Aac));
        assert_eq!((info.width, info.height), (Some(1920), Some(1080)));
        assert_eq!(info.sample_rate, Some(48000));
        assert_eq!(info.channels, Some(2));
        assert_eq!(info.audio_bitrate_bps, Some(128000));
    }

    #[test]
    fn unknown_codecs_are_not_usable() {
        let out = "[STREAM]\ncodec_name=unknown\ncodec_type=video\n[/STREAM]\n";
        let mut info = StreamInfo::default();
        info.absorb(&parse_ffprobe_streams(out));
        assert!(!info.is_usable());
    }

    #[test]
    fn unrecognized_codec_maps_to_other() {
        assert_eq!(
            parse_video_codec("mpeg2video"),
            VideoCodec::Other("mpeg2video".into())
        );
        assert_eq!(parse_audio_codec("ac3"), AudioCodec::Other("ac3".into()));
    }

    #[test]
    fn ffprobe_failure_reports_exit_code_and_stderr() {
        let runner = StubRunner {
            code: 1,
            stdout: String::new(),
            stderr: "boom".into(),
            calls: Mutex::new(Vec::new()),
        };
        let result = block_on(run_ffprobe(&runner, Path::new("x.m3u8"), true));
        match result {
            Err(MergeError::FfprobeFailed { code, stderr }) => {
                assert_eq!(code, 1);
                assert_eq!(stderr, "boom");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn hls_flags_only_used_for_playlist_probe() {
        let runner = StubRunner {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
            calls: Mutex::new(Vec::new()),
        };
        block_on(run_ffprobe(&runner, Path::new("x.m3u8"), true)).unwrap();
        block_on(run_ffprobe(&runner, Path::new("seg.ts"), false)).unwrap();

        let calls = runner.calls.lock().unwrap();
        assert!(arg_strings(&calls[0]).contains(&"-allowed_extensions".to_string()));
        assert!(!arg_strings(&calls[1]).contains(&"-allowed_extensions".to_string()));
    }

    // -- builder ------------------------------------------------------------

    #[test]
    fn builder_requires_all_fields() {
        assert!(matches!(
            MergeRequest::builder().build(),
            Err(MergeError::MissingField("playlist_path"))
        ));
        assert!(matches!(
            MergeRequest::builder().playlist_path("a.m3u8").build(),
            Err(MergeError::MissingField("output_dir"))
        ));
        assert!(matches!(
            MergeRequest::builder()
                .playlist_path("a.m3u8")
                .output_dir("out")
                .build(),
            Err(MergeError::MissingField("output_filename"))
        ));
    }

    #[test]
    fn builder_rejects_path_separators_in_filename() {
        let result = MergeRequest::builder()
            .playlist_path("a.m3u8")
            .output_dir("out")
            .output_filename("../evil")
            .build();
        assert!(matches!(result, Err(MergeError::InvalidFilename(_))));
    }

    // -- presets ------------------------------------------------------------

    #[test]
    fn video_with_h264_aac_offers_all_presets_in_order() {
        let presets = request().available_presets(&video_audio_info());
        assert_eq!(
            labels(&presets),
            vec![
                "MP4 (H.264 + AAC)",
                "MKV (H.264 + AAC)",
                "WebM (VP9 + Opus)",
                "MP4 (copy, no re-encode)",
                "MKV (copy, no re-encode)",
                "Audio only \u{2014} M4A (AAC)",
                "Audio only \u{2014} MP3",
                "Audio only \u{2014} Copy",
            ]
        );
        let defaults: Vec<_> = presets.iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].label, "MP4 (H.264 + AAC)");
        assert!(presets.iter().filter(|p| p.strips_video).count() == 3);
    }

    #[test]
    fn mp4_copy_requires_h264_and_aac_or_no_audio() {
        let mut info = video_audio_info();
        info.video_codec = Some(VideoCodec::H265);
        let presets = request().available_presets(&info);
        assert!(!labels(&presets).contains(&"MP4 (copy, no re-encode)"));
        assert!(labels(&presets).contains(&"MKV (copy, no re-encode)"));

        let mut info = video_audio_info();
        info.audio_codec = Some(AudioCodec::Opus);
        assert!(!labels(&request().available_presets(&info)).contains(&"MP4 (copy, no re-encode)"));

        let video_only = StreamInfo {
            has_video: true,
            video_codec: Some(VideoCodec::H264),
            ..Default::default()
        };
        let presets = request().available_presets(&video_only);
        assert!(labels(&presets).contains(&"MP4 (copy, no re-encode)"));
        assert!(presets.iter().all(|p| !p.strips_video));
    }

    #[test]
    fn audio_only_presets_and_default() {
        let info = StreamInfo {
            has_audio: true,
            audio_codec: Some(AudioCodec::Aac),
            ..Default::default()
        };
        let presets = request().available_presets(&info);
        assert_eq!(
            labels(&presets),
            vec![
                "M4A (AAC)",
                "MP3",
                "Opus (OGG)",
                "FLAC",
                "Copy audio (no re-encode)"
            ]
        );
        assert!(presets[0].is_default);
        assert_eq!(presets.iter().filter(|p| p.is_default).count(), 1);
        assert!(presets.iter().all(|p| p.video.is_none() && !p.strips_video));
        assert_eq!(presets[4].container, Container::M4a);
    }

    #[test]
    fn audio_copy_preset_follows_source_codec() {
        let mut info = StreamInfo {
            has_audio: true,
            audio_codec: Some(AudioCodec::Opus),
            ..Default::default()
        };
        let presets = request().available_presets(&info);
        assert_eq!(presets.last().unwrap().container, Container::Ogg);

        info.audio_codec = Some(AudioCodec::Other("ac3".into()));
        let presets = request().available_presets(&info);
        assert!(!labels(&presets).contains(&"Copy audio (no re-encode)"));
    }

    #[test]
    fn no_streams_means_no_presets() {
        assert!(
            request()
                .available_presets(&StreamInfo::default())
                .is_empty()
        );
    }

    // -- ffmpeg arguments ---------------------------------------------------

    #[test]
    fn mp4_h264_aac_args() {
        let presets = presets_for(&video_audio_info());
        let args = build_ffmpeg_args(
            Path::new("in.m3u8"),
            Path::new("out.mp4"),
            &presets[0],
            &video_audio_info(),
        )
        .unwrap();
        let args = arg_strings(&args);

        assert!(contains_seq(&args, &["-i", "in.m3u8"]));
        assert!(contains_seq(&args, &["-map", "0:v:0?", "-c:v", "libx264"]));
        assert!(contains_seq(
            &args,
            &["-map", "0:a:0?", "-c:a", "aac", "-b:a", "192k"]
        ));
        assert!(contains_seq(&args, &["-movflags", "+faststart"]));
        assert_eq!(&args[args.len() - 2..], ["-y", "out.mp4"]);
        let input_pos = args.iter().position(|a| a == "-i").unwrap();
        let flags_pos = args
            .iter()
            .position(|a| a == "-allowed_extensions")
            .unwrap();
        assert!(flags_pos < input_pos);
    }

    #[test]
    fn strip_video_preset_uses_vn_and_no_video_flags() {
        let info = video_audio_info();
        let presets = presets_for(&info);
        let extract = presets
            .iter()
            .find(|p| p.label == "Audio only \u{2014} MP3")
            .unwrap();
        let args = arg_strings(
            &build_ffmpeg_args(Path::new("in.m3u8"), Path::new("o.mp3"), extract, &info).unwrap(),
        );

        assert!(args.contains(&"-vn".to_string()));
        assert!(!args.contains(&"-c:v".to_string()));
        assert!(contains_seq(
            &args,
            &["-map", "0:a:0?", "-c:a", "libmp3lame", "-q:a", "2"]
        ));
        assert!(!args.contains(&"-movflags".to_string()));
    }

    #[test]
    fn copy_preset_args() {
        let info = video_audio_info();
        let presets = presets_for(&info);
        let copy = presets
            .iter()
            .find(|p| p.label == "MKV (copy, no re-encode)")
            .unwrap();
        let args = arg_strings(
            &build_ffmpeg_args(Path::new("in.m3u8"), Path::new("o.mkv"), copy, &info).unwrap(),
        );
        assert!(contains_seq(&args, &["-c:v", "copy"]));
        assert!(contains_seq(&args, &["-c:a", "copy"]));
    }

    #[test]
    fn audio_only_preset_on_video_source_is_invalid() {
        let info = video_audio_info();
        let audio_preset = preset(
            "M4A (AAC)",
            Container::M4a,
            None,
            Some(AudioTarget::Aac),
            false,
            true,
        );
        let result = build_ffmpeg_args(
            Path::new("in.m3u8"),
            Path::new("o.m4a"),
            &audio_preset,
            &info,
        );
        assert!(matches!(result, Err(MergeError::InvalidPreset(_))));
    }

    #[test]
    fn strip_video_with_video_target_is_invalid() {
        let info = video_audio_info();
        let bad = preset(
            "bad",
            Container::M4a,
            Some(VideoTarget::H264),
            Some(AudioTarget::Aac),
            true,
            false,
        );
        assert!(matches!(
            validate_preset(&bad, &info),
            Err(MergeError::InvalidPreset(_))
        ));
    }

    #[test]
    fn audio_only_preset_on_audio_source_is_valid() {
        let info = StreamInfo {
            has_audio: true,
            audio_codec: Some(AudioCodec::Aac),
            ..Default::default()
        };
        let presets = presets_for(&info);
        let args = arg_strings(
            &build_ffmpeg_args(Path::new("in.m3u8"), Path::new("o.m4a"), &presets[0], &info)
                .unwrap(),
        );
        assert!(contains_seq(&args, &["-movflags", "+faststart"]));
        assert!(args.contains(&"-vn".to_string()));
    }

    #[test]
    fn container_extensions() {
        assert_eq!(Container::Mp4.extension(), "mp4");
        assert_eq!(Container::Ogg.extension(), "ogg");
        assert_eq!(Container::M4a.extension(), "m4a");
    }
}
