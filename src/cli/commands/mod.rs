use clap::Subcommand;

use crate::cli::commands::{download::DownloadArgs};

pub mod download;

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Stream and download m3u8 media
    #[clap(alias = "dl")]
    Download(DownloadArgs),
}
