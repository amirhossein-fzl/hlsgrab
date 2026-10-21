use clap::Parser;

use crate::cli::{
    args::Args, commands::{Commands, download::handle_download_action, merge::handle_merge_action},
};

#[derive(Debug)]
pub struct Application {
    pub args: Args,
}

impl Application {
    pub async fn launch() -> Self {
        let args = Args::parse();

        match args.command {
            Some(Commands::Download(ref download_args)) => {
                handle_download_action(&download_args).await
            },
            Some(Commands::Merge(ref  merge_args)) => {
                handle_merge_action(&merge_args).await
            },
            None => {}
        }

        Self { args }
    }
}
