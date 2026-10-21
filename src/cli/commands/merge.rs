use std::{env, fs, path::Path};

use clap::Args;
use cliclack::select;

use crate::{cli::fail::OrFail, merge::{container::Container, merge_request::MergeRequest}};

#[derive(Debug, Args)]
pub struct MergeArgs {
    /// Path of downloaded playlist file.
    #[arg()]
    path: String,

    /// Output directory.
    #[arg(long, short, value_name = "PATH", default_value = "")]
    output_directory: String,

    /// Output file name
    #[arg(long)]
    filename: String,

    /// Output file format
    #[arg(short, long, default_value = "")]
    format: String,

    /// Merge with recommended format
    #[arg(long, action)]
    recommended_format: bool,
}

pub async fn handle_merge_action(args: &MergeArgs) {
    let output_directory: &Path = if args.output_directory.is_empty() {
        &env::current_dir().or_fail("Error in get work directory.")
    } else {
        Path::new(&args.output_directory)
    };

    if !output_directory.exists() {
        fs::create_dir_all(&output_directory).or_fail("Could not create output directory");
    }

    let merge_request = MergeRequest::builder()
        .output_dir(output_directory)
        .output_filename(&args.filename)
        .playlist_path(&args.path)
        .build()
        .or_fail("Error in build merge request.");

    let stream_info = &merge_request
        .probe()
        .await
        .or_fail("Error in probe playlist.");

    let available_presets = &merge_request.available_presets(stream_info);

    let preset = if args.recommended_format {
        available_presets.iter().find(|v| v.is_default).or_fail("Not available recommended format.")
    } else if !args.format.is_empty() {
        let format = &args.format.to_lowercase();
        let format = format.trim();
        let merge_container = Container::from_str(format).or_fail("The format provided not currently supported.");

        available_presets.iter().find(|v| { v.container == merge_container }).or_fail("The format provided for this media is invalid.")
    } else {
        let items: &Vec<(usize, &str, String)> = &available_presets
            .iter()
            .enumerate()
            .map(|(key, v)| (key, v.label, String::new()))
            .collect();

        let chosen_preset = select("Select preset")
            .filter_mode()
            .items(items)
            .interact()
            .or_fail("Error in select preset");

        &available_presets[chosen_preset]
    };

    println!("Starting merge playlist ...");

    let _ = &merge_request
        .merge(preset)
        .await
        .or_fail("Unexpected error in merge playlist.");

    println!("Playlist is merged successfully.");
}
