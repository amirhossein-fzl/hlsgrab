use clap::Parser;

use crate::cli::commands::Commands;

#[derive(Parser, Debug)]
#[clap(disable_help_subcommand = true)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,
}
