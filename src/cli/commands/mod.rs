use clap::Subcommand;

use crate::cli::commands::{download::DownloadArgs, merge::MergeArgs};

pub mod download;
pub mod merge;

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Stream and download m3u8 media
    #[clap(alias = "dl")]
    Download(DownloadArgs),
    /// Merge downloaded m3u8 playlist in unit outout
    Merge(MergeArgs),
}
