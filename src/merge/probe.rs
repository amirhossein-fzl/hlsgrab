use std::{ffi::OsString, path::Path};

use crate::{
    merge::{
        FFPROBE_BIN, HLS_INPUT_FLAGS, errors::MergeError, raw_stream::RawStream,
        runner::ProcessRunner,
    },
    utils::os_strings::push_strs,
};

pub fn parse_ffprobe_streams(stdout: &str) -> Vec<RawStream> {
    let mut streams = Vec::new();
    let mut current: Option<RawStream> = None;

    for raw in stdout.lines() {
        let line = raw.trim();
        if line == "[STREAM]" {
            current = Some(RawStream::default());
        } else if line == "[/STREAM]" {
            if let Some(stream) = current.take() {
                streams.push(stream);
            }
        } else if let (Some(stream), Some((key, value))) = (current.as_mut(), line.split_once('='))
        {
            let value = value.trim();
            match key.trim() {
                "codec_type" => stream.codec_type = value.to_string(),
                "codec_name" => {
                    if !matches!(value, "" | "unknown" | "N/A") {
                        stream.codec_name = Some(value.to_string());
                    }
                }
                "width" => stream.width = value.parse().ok(),
                "height" => stream.height = value.parse().ok(),
                "sample_rate" => stream.sample_rate = value.parse().ok(),
                "channels" => stream.channels = value.parse().ok(),
                "bit_rate" => stream.bit_rate = value.parse().ok(),
                _ => {}
            }
        }
    }

    streams
}

pub async fn run_ffprobe<R: ProcessRunner>(
    runner: &R,
    target: &Path,
    is_playlist: bool,
) -> Result<Vec<RawStream>, MergeError> {
    let mut args: Vec<OsString> = Vec::new();
    push_strs(&mut args, &["-v", "error"]);
    if is_playlist {
        push_strs(&mut args, HLS_INPUT_FLAGS);
    }
    push_strs(
        &mut args,
        &[
            "-show_entries",
            "stream=codec_type,codec_name,width,height,sample_rate,channels,bit_rate",
            "-of",
            "default=noprint_wrappers=0",
            "-i",
        ],
    );
    args.push(target.as_os_str().to_owned());

    let output = runner.run(FFPROBE_BIN, &args).await?;
    if output.code != 0 {
        return Err(MergeError::FfprobeFailed {
            code: output.code,
            stderr: output.stderr,
        });
    }
    Ok(parse_ffprobe_streams(&output.stdout))
}
