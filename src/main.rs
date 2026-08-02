use crate::cli::app::Application;

pub mod cli;
pub mod downloader;
pub mod hls;
pub mod utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = Application::launch().await;

    Ok(())
}
