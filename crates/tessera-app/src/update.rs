use std::borrow::Cow;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use isahc::ReadResponseExt;
use isahc::config::{Configurable, RedirectPolicy};
use isahc::http::Request;
use semver::Version;
use serde::Deserialize;
use zip::ZipArchive;

pub enum UpdateError {
    Zip(zip::result::ZipError),
    Io(std::io::Error),
    Http(isahc::Error),
    Request(isahc::http::Error),
    /// The download was canceled by the user.
    Cancelled,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Zip(msg) => write!(f, "[Update] Failed to unzip asset {msg}"),
            UpdateError::Io(msg) => write!(f, "[Update] I/O Error {msg}"),
            UpdateError::Http(msg) => write!(f, "[Update] Download failed: {msg}"),
            UpdateError::Request(msg) => write!(f, "[Update] Invalid request: {msg}"),
            UpdateError::Cancelled => write!(f, "[Update] Download canceled"),
        }
    }
}

impl From<zip::result::ZipError> for UpdateError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}

impl From<std::io::Error> for UpdateError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<isahc::Error> for UpdateError {
    fn from(value: isahc::Error) -> Self {
        Self::Http(value)
    }
}

impl From<isahc::http::Error> for UpdateError {
    fn from(value: isahc::http::Error) -> Self {
        Self::Request(value)
    }
}

/// User agent sent with update requests, required by the github api.
const USER_AGENT: &str = concat!("tessera-updater/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    url: String,
    browser_download_url: String,
}

/// What kind of update is being offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    App,
}

/// A GitHub repository whose releases an app updates from.
#[derive(Debug, Clone, Copy)]
pub struct UpdateRepository {
    pub owner: &'static str,
    pub repository: &'static str,
    pub archive: &'static str,
}

/// Information about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub kind: UpdateKind,
    pub remote_version: String,
    pub local_version: String,
    pub asset_url: String,
}

/// Outcome of a successful update.
#[derive(Debug, Clone, Copy)]
pub enum UpdateOutcome {
    AppInstalled,
}

/// Restart the current program with the same arguments.
/// - On Unix: replaces the current process image (exec) and never returns if successful.
/// - On Windows/others: spawns a new instance and exits the current process.
fn restart_self() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    // Best-effort flush
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(Command::new(exe).args(args).exec())
    }

    #[cfg(not(unix))]
    {
        Command::new(exe).args(args).spawn()?;
        std::process::exit(0);
    }
}

/// Fetches the latest release information from a GitHub repository.
fn fetch_latest_release(
    owner: &str,
    repo: &str,
    proxy_cfg: &GitHubProxyConfig,
) -> Option<GitHubRelease> {
    let api_url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let api_url = get_proxied_api_url(api_url.as_str(), proxy_cfg);

    Request::get(api_url.as_ref())
        .header("user-agent", USER_AGENT)
        .header("accept", "application/vnd.github+json")
        .redirect_policy(RedirectPolicy::Follow)
        .body(())
        .map_err(|e| eprintln!("[Update] Failed to build release request: {e}"))
        .and_then(|request| {
            isahc::send(request)
                .map_err(|e| eprintln!("[Update] Failed to fetch latest release: {e}"))
        })
        .and_then(|mut response| {
            response
                .text()
                .map_err(|e| eprintln!("[Update] Failed to read release response: {e}"))
        })
        .and_then(|response_string| {
            serde_json::from_str(&response_string)
                .map_err(|e| eprintln!("[Update] Failed to parse release JSON: {e}"))
        })
        .ok()
}

/// Compares two version strings and returns true if the remote version is newer.
fn is_newer_version(remote_version: &str, local_version: &str) -> bool {
    let (latest_ver, current_ver) = match (
        Version::parse(remote_version),
        Version::parse(local_version),
    ) {
        (Ok(l), Ok(c)) => (l, c),
        _ => {
            eprintln!(
                "[Update] Failed to parse version strings: latest={remote_version:?} current={local_version:?}"
            );
            return false;
        }
    };

    latest_ver > current_ver
}

/// Finds an asset by name in the release assets.
fn find_asset_by_name(assets: &[GitHubAsset], name: &str) -> Option<GitHubAsset> {
    assets.iter().find(|asset| asset.name == name).cloned()
}

#[derive(Debug, Clone)]
pub struct GitHubProxyConfig {
    enabled: bool,
    api_proxy_url: String,
    release_proxy_url: String,
}

impl GitHubProxyConfig {
    pub fn new() -> Self {
        Self {
            enabled: false,
            api_proxy_url: String::new(),
            release_proxy_url: String::new(),
        }
    }
}

impl Default for GitHubProxyConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn read_github_proxy_config() -> GitHubProxyConfig {
    const GH_PROXY_CFG_PATH: &str = "gh_proxy.cfg";

    let mut config = GitHubProxyConfig::new();

    let proxy_cfg = File::open(GH_PROXY_CFG_PATH);
    if let Ok(cfg) = proxy_cfg {
        let reader = BufReader::new(cfg);
        let lines = reader
            .lines()
            .take(2)
            .collect::<std::io::Result<Vec<String>>>();

        if let Ok(lines) = lines
            && lines.len() == 2
        {
            config.enabled = true;
            config.api_proxy_url = lines[0].clone();
            config.release_proxy_url = lines[1].clone();
        }
    }

    config
}

fn get_proxied_asset_url<'a>(
    asset: &'a GitHubAsset,
    proxy_cfg: &GitHubProxyConfig,
) -> Cow<'a, str> {
    if proxy_cfg.enabled {
        let url = format!(
            "https://{}/{}",
            proxy_cfg.release_proxy_url, asset.browser_download_url
        );
        println!("[Update] Proxied asset url: {url}");

        Cow::Owned(url)
    } else {
        println!("[Update] Asset url: {}", asset.url);

        Cow::Borrowed(asset.url.as_str())
    }
}

fn get_proxied_api_url<'a>(api_url: &'a str, proxy_cfg: &GitHubProxyConfig) -> Cow<'a, str> {
    if proxy_cfg.enabled {
        let url = format!("https://{}/{}", proxy_cfg.api_proxy_url, api_url);
        println!("[Update] Proxied API url: {url}");

        Cow::Owned(url)
    } else {
        println!("[Update] API url: {api_url}");

        Cow::Borrowed(api_url)
    }
}

/// Checks the releases of `repository` for a newer app version.
/// Returns `Some(UpdateInfo)` if an update is available and the asset was found.
pub fn check_app_update(
    proxy_cfg: &GitHubProxyConfig,
    repository: UpdateRepository,
    local_version: &str,
) -> Option<UpdateInfo> {
    let release = fetch_latest_release(repository.owner, repository.repository, proxy_cfg)?;

    if !is_newer_version(&release.tag_name, local_version) {
        return None;
    }

    let asset = match find_asset_by_name(&release.assets, repository.archive) {
        Some(asset) => asset,
        None => {
            println!(
                "[Update] Could not find '{}' in release",
                repository.archive
            );
            return None;
        }
    };

    Some(UpdateInfo {
        kind: UpdateKind::App,
        remote_version: release.tag_name,
        local_version: local_version.to_string(),
        asset_url: get_proxied_asset_url(&asset, proxy_cfg).into_owned(),
    })
}

/// Progress of an update download.
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    /// Downloaded percentage, 0 to 100.
    pub percent: u32,
    /// Current download speed in bytes per second.
    pub bytes_per_sec: u64,
}

/// Downloads the given url, reporting progress and honoring the cancel flag.
pub(crate) fn download_bytes<F>(
    url: &str,
    is_cancelled: impl Fn() -> bool,
    mut on_progress: F,
) -> Result<Vec<u8>, UpdateError>
where
    F: FnMut(DownloadProgress),
{
    let request = Request::get(url)
        .header("user-agent", USER_AGENT)
        .header("accept", "application/octet-stream")
        .redirect_policy(RedirectPolicy::Follow)
        .body(())?;

    let response = isahc::send(request)?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    let mut body = response.into_body();
    let mut data = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut buffer = [0u8; 64 * 1024];

    let mut last_tick = Instant::now();
    let mut last_bytes = 0u64;

    loop {
        if is_cancelled() {
            return Err(UpdateError::Cancelled);
        }

        let read = body.read(&mut buffer)?;

        if read == 0 {
            break;
        }

        data.extend_from_slice(&buffer[..read]);

        let elapsed = last_tick.elapsed();

        if elapsed >= Duration::from_millis(250) {
            let received = data.len() as u64;
            let bytes_per_sec = ((received - last_bytes) as f64 / elapsed.as_secs_f64()) as u64;

            last_bytes = received;
            last_tick = Instant::now();

            let percent = total
                .map(|total| (received.saturating_mul(100) / total.max(1)) as u32)
                .unwrap_or(0);

            on_progress(DownloadProgress {
                percent: percent.min(100),
                bytes_per_sec,
            });
        }
    }

    on_progress(DownloadProgress {
        percent: 100,
        bytes_per_sec: 0,
    });

    Ok(data)
}

/// Downloads and installs the given update, reporting progress via the callback.
/// The cancel flag aborts the download and returns [`UpdateError::Cancelled`].
/// On a successful app update, the process is replaced and this function does not return.
pub fn perform_update<F>(
    info: UpdateInfo,
    cancel: Arc<AtomicBool>,
    on_progress: F,
) -> Result<UpdateOutcome, UpdateError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    let bytes = download_bytes(
        &info.asset_url,
        || cancel.load(Ordering::Relaxed),
        on_progress,
    )?;

    match info.kind {
        UpdateKind::App => {
            install_app(bytes)?;
            Ok(UpdateOutcome::AppInstalled)
        }
    }
}

/// Extracts the new executable from the downloaded zip, replaces self, and restarts.
fn install_app(bytes: Vec<u8>) -> Result<(), UpdateError> {
    let executable = std::env::current_exe()?;
    let expected = executable.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "current executable has no filename",
        )
    })?;
    let expected = expected.to_string_lossy();
    let exe_data = executable_from_archive(bytes, &expected)?;

    let temp_path = std::env::temp_dir().join(format!("tessera-update-{}.exe", std::process::id()));
    let mut temporary = File::options()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    let written = temporary.write_all(&exe_data);
    drop(temporary);

    let installed = written.and_then(|()| self_replace::self_replace(&temp_path));
    let _ = std::fs::remove_file(&temp_path);
    installed?;
    restart_self()?;

    Ok(())
}

fn executable_from_archive(bytes: Vec<u8>, expected: &str) -> Result<Vec<u8>, UpdateError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let mut exe_data = None;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if !file.is_dir()
            && Path::new(file.name())
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(expected))
        {
            if exe_data.is_some() {
                return Err(UpdateError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "multiple matching executables in update archive",
                )));
            }
            let mut buffer = Vec::new();
            std::io::copy(&mut file, &mut buffer)?;
            exe_data = Some(buffer);
        }
    }

    exe_data.ok_or_else(|| {
        UpdateError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "executable not found inside zip archive",
        ))
    })
}
