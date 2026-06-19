pub mod cli;
pub mod downloader;
pub mod hls;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
