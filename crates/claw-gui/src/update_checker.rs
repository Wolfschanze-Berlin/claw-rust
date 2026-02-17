//! Background update checker that queries GitHub Releases API.
//!
//! Checks the latest release of the claw-rust repository and compares it
//! against the current build version. Results are sent to the UI via an mpsc
//! channel so the egui render loop is never blocked.
//!
//! Also provides [`UpdateDownloader`] for downloading and launching platform
//! installers with progress reporting.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// How long to cache a check result before allowing another automatic check.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Timeout for the HTTP request to GitHub API.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for downloading an installer asset.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// GitHub API endpoint for the latest release.
const RELEASES_URL: &str =
    "https://api.github.com/repos/Wolfschanze-Berlin/claw-rust/releases/latest";

/// Result of a version check against the GitHub releases API.
#[derive(Debug, Clone)]
pub struct UpdateCheckResult {
    /// The version currently running (from `CARGO_PKG_VERSION`).
    pub current: semver::Version,
    /// The latest version published on GitHub.
    pub latest: semver::Version,
    /// Whether the latest version is newer than the current one.
    pub update_available: bool,
}

/// Manages update check state and scheduling.
pub struct UpdateChecker {
    /// Timestamp of the last completed check (successful or failed).
    last_check_time: Option<Instant>,
    /// Sender half — used to dispatch results from spawned tasks.
    result_tx: mpsc::UnboundedSender<Result<UpdateCheckResult, String>>,
    /// Receiver half — drained each frame by the UI.
    result_rx: mpsc::UnboundedReceiver<Result<UpdateCheckResult, String>>,
    /// egui context for requesting repaints after a result arrives.
    repaint: eframe::egui::Context,
}

impl UpdateChecker {
    /// Create a new checker bound to the given egui repaint context.
    pub fn new(repaint: eframe::egui::Context) -> Self {
        let (result_tx, result_rx) = mpsc::unbounded_channel();
        Self {
            last_check_time: None,
            result_tx,
            result_rx,
            repaint,
        }
    }

    /// Returns `true` if enough time has elapsed since the last check (or if
    /// no check has ever been performed).
    pub fn should_check(&self) -> bool {
        match self.last_check_time {
            None => true,
            Some(t) => t.elapsed() >= CHECK_INTERVAL,
        }
    }

    /// Spawn a background task that performs the update check.
    ///
    /// The result is sent through the internal mpsc channel and can be
    /// retrieved via [`drain_results`].
    pub fn trigger_check(&mut self) {
        let tx = self.result_tx.clone();
        let ctx = self.repaint.clone();
        tokio::spawn(async move {
            let result = check_for_update().await;
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    /// Drain any pending results from the channel.
    ///
    /// Call this once per frame from the `update()` method.  Returns the most
    /// recent result if one or more arrived since the last drain.
    pub fn drain_result(&mut self) -> Option<Result<UpdateCheckResult, String>> {
        let mut last = None;
        while let Ok(r) = self.result_rx.try_recv() {
            self.last_check_time = Some(Instant::now());
            last = Some(r);
        }
        last
    }
}

// ---------------------------------------------------------------------------
// Download progress types
// ---------------------------------------------------------------------------

/// Status of an ongoing or completed download.
#[derive(Debug, Clone)]
pub enum DownloadStatus {
    /// Currently downloading bytes.
    Downloading,
    /// Download finished successfully; the path points to the installer.
    Completed(PathBuf),
    /// Download or installation launch failed.
    Failed(String),
}

/// Progress snapshot sent from the download task to the UI each frame.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Bytes received so far.
    pub bytes_downloaded: u64,
    /// Total size in bytes if the server provided `Content-Length`.
    pub total_bytes: Option<u64>,
    /// Current download status.
    pub status: DownloadStatus,
}

// ---------------------------------------------------------------------------
// UpdateDownloader
// ---------------------------------------------------------------------------

/// Downloads a platform-specific installer from a GitHub release and launches
/// it.  Progress is reported via an mpsc channel so the egui render loop can
/// display a progress bar without blocking.
pub struct UpdateDownloader {
    /// Sender for progress updates from the background task.
    progress_tx: mpsc::UnboundedSender<DownloadProgress>,
    /// Receiver polled each frame by the UI.
    progress_rx: mpsc::UnboundedReceiver<DownloadProgress>,
    /// egui context for requesting repaints after progress updates.
    repaint: eframe::egui::Context,
}

impl UpdateDownloader {
    /// Create a new downloader bound to the given repaint context.
    pub fn new(repaint: eframe::egui::Context) -> Self {
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        Self {
            progress_tx,
            progress_rx,
            repaint,
        }
    }

    /// Start downloading the installer for the given version in the background.
    ///
    /// The task queries the GitHub Releases API, finds the correct asset for
    /// the current platform, downloads it to a temp directory, then launches
    /// the installer and exits the application.
    pub fn start_download(&self, version: String) {
        let tx = self.progress_tx.clone();
        let ctx = self.repaint.clone();
        tokio::spawn(async move {
            let result = download_and_install(version, tx.clone(), ctx.clone()).await;
            if let Err(msg) = result {
                error!(%msg, "update download failed");
                let _ = tx.send(DownloadProgress {
                    bytes_downloaded: 0,
                    total_bytes: None,
                    status: DownloadStatus::Failed(msg),
                });
                ctx.request_repaint();
            }
        });
    }

    /// Drain all pending progress updates from the channel.
    ///
    /// Returns the most recent update if any arrived since the last drain.
    pub fn drain_progress(&mut self) -> Option<DownloadProgress> {
        let mut last = None;
        while let Ok(p) = self.progress_rx.try_recv() {
            last = Some(p);
        }
        last
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Build a shared `reqwest::Client` with a user-agent header.
fn build_client(timeout: Duration) -> Result<reqwest::Client, String> {
    let current_str = env!("CARGO_PKG_VERSION");
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(format!("claw-gui/{current_str}"))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))
}

/// Perform a single update check against the GitHub releases API.
async fn check_for_update() -> Result<UpdateCheckResult, String> {
    let current_str = env!("CARGO_PKG_VERSION");
    let current = semver::Version::parse(current_str)
        .map_err(|e| format!("invalid current version '{current_str}': {e}"))?;

    let client = build_client(REQUEST_TIMEOUT)?;

    debug!("querying GitHub releases API");

    let resp = client
        .get(RELEASES_URL)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned status {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse response: {e}"))?;

    let tag = body["tag_name"]
        .as_str()
        .ok_or_else(|| "missing tag_name in response".to_string())?;

    let version_str = tag.strip_prefix('v').unwrap_or(tag);
    let latest = semver::Version::parse(version_str)
        .map_err(|e| format!("invalid release version '{version_str}': {e}"))?;

    let update_available = latest > current;

    debug!(
        %current,
        %latest,
        update_available,
        "update check complete"
    );

    Ok(UpdateCheckResult {
        current,
        latest,
        update_available,
    })
}

/// Find the platform-specific asset download URL from a GitHub release.
fn find_asset_url(assets: &[serde_json::Value]) -> Result<(String, String), String> {
    for asset in assets {
        let name = asset["name"].as_str().unwrap_or_default();
        let url = asset["browser_download_url"].as_str().unwrap_or_default();

        if url.is_empty() {
            continue;
        }

        let matches_platform = if cfg!(target_os = "windows") {
            let lower = name.to_lowercase();
            lower.ends_with(".exe") || lower.contains("setup")
        } else if cfg!(target_os = "macos") {
            name.to_lowercase().ends_with(".dmg")
        } else {
            // Linux: look for .AppImage or .deb or .tar.gz
            let lower = name.to_lowercase();
            lower.ends_with(".appimage")
                || lower.ends_with(".deb")
                || lower.ends_with(".tar.gz")
        };

        if matches_platform {
            return Ok((name.to_string(), url.to_string()));
        }
    }
    Err("no installer asset found for this platform".to_string())
}

/// Download the installer and launch it.
async fn download_and_install(
    version: String,
    tx: mpsc::UnboundedSender<DownloadProgress>,
    ctx: eframe::egui::Context,
) -> Result<(), String> {
    let client = build_client(DOWNLOAD_TIMEOUT)?;

    // Fetch release metadata for the target version
    let release_url = format!(
        "https://api.github.com/repos/Wolfschanze-Berlin/claw-rust/releases/tags/v{version}"
    );

    info!(%release_url, "fetching release metadata");

    let resp = client
        .get(&release_url)
        .send()
        .await
        .map_err(|e| format!("failed to fetch release metadata: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "GitHub API returned status {} for release v{version}",
            resp.status()
        ));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse release metadata: {e}"))?;

    let assets = body["assets"]
        .as_array()
        .ok_or_else(|| "missing assets in release response".to_string())?;

    let (asset_name, download_url) = find_asset_url(assets)?;

    info!(%asset_name, %download_url, "downloading installer asset");

    // Send initial progress
    let _ = tx.send(DownloadProgress {
        bytes_downloaded: 0,
        total_bytes: None,
        status: DownloadStatus::Downloading,
    });
    ctx.request_repaint();

    // Start streaming download
    let resp = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("download request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "download returned status {}",
            resp.status()
        ));
    }

    let total_bytes = resp.content_length();
    let dest_path = std::env::temp_dir().join(&asset_name);

    let mut file = tokio::fs::File::create(&dest_path)
        .await
        .map_err(|e| format!("failed to create temp file: {e}"))?;

    let mut bytes_downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download stream error: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("failed to write to temp file: {e}"))?;
        bytes_downloaded += chunk.len() as u64;

        let _ = tx.send(DownloadProgress {
            bytes_downloaded,
            total_bytes,
            status: DownloadStatus::Downloading,
        });
        ctx.request_repaint();
    }

    file.flush()
        .await
        .map_err(|e| format!("failed to flush temp file: {e}"))?;
    drop(file);

    info!(
        bytes = bytes_downloaded,
        path = %dest_path.display(),
        "download complete"
    );

    // Report completion
    let _ = tx.send(DownloadProgress {
        bytes_downloaded,
        total_bytes,
        status: DownloadStatus::Completed(dest_path.clone()),
    });
    ctx.request_repaint();

    // Launch the installer
    launch_installer(&dest_path)?;

    Ok(())
}

/// Launch the downloaded installer and exit the current application.
fn launch_installer(path: &std::path::Path) -> Result<(), String> {
    info!(path = %path.display(), "launching installer");

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("failed to launch installer: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to open DMG: {e}"))?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        info!(path = %path.display(), "installer downloaded — please run manually");
        return Ok(());
    }

    // Give the OS a moment to start the process, then exit so the installer
    // can replace binaries.
    info!("exiting to allow installer to proceed");
    std::process::exit(0);
}
