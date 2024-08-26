use clap::Parser;
use log::{error, info, warn, LevelFilter};
use reqwest;
#[cfg(feature = "pipes")]
use screenpipe_core::run_js;
use screenpipe_server::Cli;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use url::Url;

#[cfg(feature = "pipes")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.pipe.is_empty() {
        error!("No pipe specified. Use --pipe to specify the pipe.");
        eprintln!("No pipe specified. Use --pipe to specify the pipe.");
        std::process::exit(1);
    }

    let mut builder = env_logger::Builder::new();
    builder
        .filter(None, LevelFilter::Info)
        .format_timestamp_secs()
        .init();

    warn!("Warning: only 1 pipe is supported right now. This will change in the future.");

    let pipe_input = &cli.pipe[0];
    info!("Attempting to process pipe input: {}", pipe_input);

    let path_to_main_module = match Url::parse(pipe_input) {
        Ok(_) => {
            info!("Input appears to be a URL. Attempting to download...");
            match download_pipe(pipe_input).await {
                Ok(path) => path,
                Err(e) => {
                    error!("Failed to download pipe: {}", e);
                    return Err(e);
                }
            }
        }
        Err(_) => {
            info!("Input appears to be a local path. Attempting to canonicalize...");
            match Path::new(pipe_input).canonicalize() {
                Ok(path) => path,
                Err(e) => {
                    error!("Failed to canonicalize path: {}", e);
                    return Err(e.into());
                }
            }
        }
    };

    info!("Path to main module: {:?}", path_to_main_module);

    match run_js(&path_to_main_module.to_string_lossy()).await {
        Ok(_) => info!("JS execution completed successfully"),
        Err(error) => {
            error!("Error during JS execution: {}", error);
            return Err(error.into());
        }
    }

    Ok(())
}

fn get_raw_github_url(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    info!("Attempting to get raw GitHub URL for: {}", url);
    let parsed_url = Url::parse(url)?;
    if parsed_url.host_str() == Some("github.com") {
        let path_segments: Vec<&str> = parsed_url.path_segments().unwrap().collect();
        if path_segments.len() >= 3 {
            let (owner, repo, _, branch) = (
                path_segments[0],
                path_segments[1],
                path_segments[2],
                path_segments[3],
            );
            let raw_path = path_segments[4..].join("/");
            let raw_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                owner, repo, branch, raw_path
            );
            info!("Converted to raw GitHub URL: {}", raw_url);
            return Ok(raw_url);
        }
    }
    info!("URL is not a GitHub URL, returning as-is");
    Ok(url.to_string())
}

async fn download_pipe(url: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    info!("Downloading pipe from URL: {}", url);

    let raw_url = get_raw_github_url(url)?;
    let parsed_url = Url::parse(&raw_url)?;
    if parsed_url.host_str() != Some("raw.githubusercontent.com") {
        error!("Only public GitHub URLs or raw.githubusercontent.com URLs are supported");
        return Err(
            "Only public GitHub URLs or raw.githubusercontent.com URLs are supported".into(),
        );
    }

    info!("Downloading from raw URL: {}", raw_url);
    std::io::stdout().flush()?;

    let response = match reqwest::get(&raw_url).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to send GET request: {}", e);
            return Err(e.into());
        }
    };

    let content = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            error!("Failed to get response text: {}", e);
            return Err(e.into());
        }
    };

    info!("Downloaded content length: {} bytes", content.len());

    let mut temp_file = match NamedTempFile::new() {
        Ok(file) => file,
        Err(e) => {
            error!("Failed to create temporary file: {}", e);
            return Err(e.into());
        }
    };

    if let Err(e) = temp_file.write_all(content.as_bytes()) {
        error!("Failed to write content to temporary file: {}", e);
        return Err(e.into());
    }

    // Extract the file extension from the URL
    let extension = Path::new(url)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("js"); // Default to .js if no extension is found

    info!("File extension: {}", extension);

    // Create a new temporary file with the correct extension
    let temp_path = temp_file.into_temp_path();
    let final_path = temp_path.with_extension(extension);
    if let Err(e) = std::fs::rename(&temp_path, &final_path) {
        error!("Failed to rename temporary file: {}", e);
        return Err(e.into());
    }

    info!("Pipe downloaded successfully to: {:?}", final_path);

    Ok(final_path)
}

#[cfg(not(feature = "pipes"))]
fn main() {
    eprintln!("Pipes support is not enabled. Compile with --features pipes to enable it.");
    std::process::exit(1);
}
