use std::time::Duration;

use colored::Colorize;
use semver::Version;

use crate::error::RegistryError;
use crate::github::{self, IiiGithubError};
use crate::registry::{self, BinarySpec};
use crate::state::AppState;
use crate::{download, platform};

/// Information about an available update.
#[derive(Debug)]
pub struct UpdateInfo {
    pub binary_name: String,
    pub current_version: Version,
    pub latest_version: Version,
}

/// Check for updates for all installed binaries.
/// Returns a list of available updates.
pub async fn check_for_updates(
    client: &reqwest::Client,
    state: &AppState,
) -> Vec<UpdateInfo> {
    let mut updates = Vec::new();

    for (name, binary_state) in &state.binaries {
        // Find the spec for this binary
        let spec = match registry::all_binaries()
            .into_iter()
            .find(|s| s.name == name)
        {
            Some(s) => s,
            None => continue,
        };

        // Fetch latest release
        let release = match github::fetch_latest_release(client, spec).await {
            Ok(r) => r,
            Err(_) => continue, // Silently skip on error
        };

        // Parse version
        let latest = match github::parse_release_version(&release.tag_name) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if latest > binary_state.version {
            updates.push(UpdateInfo {
                binary_name: name.clone(),
                current_version: binary_state.version.clone(),
                latest_version: latest,
            });
        }
    }

    updates
}

/// Print update notifications to stderr (informational, not prompting).
pub fn print_update_notifications(updates: &[UpdateInfo]) {
    if updates.is_empty() {
        return;
    }

    eprintln!();
    for update in updates {
        eprintln!(
            "  {} Update available: {} {} → {} (run `iii-cli update {}`)",
            "info:".yellow(),
            update.binary_name,
            update.current_version.to_string().dimmed(),
            update.latest_version.to_string().green(),
            // Use the CLI command name, not the binary name
            cli_command_for_binary(&update.binary_name).unwrap_or(&update.binary_name),
        );
    }
    eprintln!();
}

/// Get the CLI command name for a binary name.
fn cli_command_for_binary(binary_name: &str) -> Option<&str> {
    for spec in registry::REGISTRY {
        if spec.name == binary_name {
            return spec.commands.first().map(|c| c.cli_command);
        }
    }
    None
}

/// Run the background update check with a bounded timeout.
/// Compatible with the process-replacement lifecycle.
///
/// Returns update notifications if the check completes within the timeout,
/// or None if it times out (will retry on next invocation).
pub async fn run_background_check(
    state: &AppState,
    timeout_ms: u64,
) -> Option<(Vec<UpdateInfo>, bool)> {
    if !state.is_update_check_due() {
        return None;
    }

    let client = match github::build_client() {
        Ok(c) => c,
        Err(_) => return None,
    };

    let check = async {
        let updates = check_for_updates(&client, state).await;
        (updates, true) // true = check completed, should update timestamp
    };

    match tokio::time::timeout(Duration::from_millis(timeout_ms), check).await {
        Ok(result) => Some(result),
        Err(_) => None, // Timed out, will retry next run
    }
}

/// Update a specific binary to the latest version.
pub async fn update_binary(
    client: &reqwest::Client,
    spec: &BinarySpec,
    state: &mut AppState,
) -> Result<UpdateResult, UpdateError> {
    // Check platform support
    platform::check_platform_support(spec)?;

    eprintln!("  Checking for updates to {}...", spec.name);

    // Fetch latest release
    let release = github::fetch_latest_release(client, spec).await?;
    let latest_version = github::parse_release_version(&release.tag_name)
        .map_err(|e| UpdateError::VersionParse(e.to_string()))?;

    // Check if already up to date
    if let Some(installed) = state.installed_version(spec.name) {
        if *installed >= latest_version {
            return Ok(UpdateResult::AlreadyUpToDate {
                binary: spec.name.to_string(),
                version: installed.clone(),
            });
        }
    }

    // Find asset for current platform
    let asset_name = platform::asset_name(spec.name);
    let asset = github::find_asset(&release, &asset_name).ok_or_else(|| {
        UpdateError::Github(IiiGithubError::Network(
            crate::error::NetworkError::AssetNotFound {
                binary: spec.name.to_string(),
                platform: platform::current_target().to_string(),
            },
        ))
    })?;

    // Find checksum asset in release (separate asset, not appended URL)
    let checksum_url = if spec.has_checksum {
        let checksum_name = platform::checksum_asset_name(spec.name);
        github::find_asset(&release, &checksum_name)
            .map(|a| a.browser_download_url.clone())
    } else {
        None
    };

    eprintln!(
        "  Updating {} to v{}...",
        spec.name,
        latest_version
    );

    // Download and install
    let target_path = platform::binary_path(spec.name);
    download::download_and_install(
        client,
        spec,
        asset,
        checksum_url.as_deref(),
        &target_path,
    )
    .await?;

    // Update state
    state.record_install(spec.name, latest_version.clone(), asset_name);

    Ok(UpdateResult::Updated {
        binary: spec.name.to_string(),
        from: state
            .installed_version(spec.name)
            .cloned(),
        to: latest_version,
    })
}

/// Update all installed binaries.
pub async fn update_all(
    client: &reqwest::Client,
    state: &mut AppState,
) -> Vec<Result<UpdateResult, UpdateError>> {
    let specs: Vec<&BinarySpec> = registry::all_binaries()
        .into_iter()
        .filter(|spec| {
            // Only update binaries that are installed or whose platform is supported
            state.binaries.contains_key(spec.name)
                || platform::check_platform_support(spec).is_ok()
        })
        .collect();

    let mut results = Vec::new();
    for spec in specs {
        results.push(update_binary(client, spec, state).await);
    }
    results
}

/// Result of an update operation.
#[derive(Debug)]
pub enum UpdateResult {
    Updated {
        binary: String,
        from: Option<Version>,
        to: Version,
    },
    AlreadyUpToDate {
        binary: String,
        version: Version,
    },
}

/// Errors during update.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error(transparent)]
    Registry(#[from] RegistryError),

    #[error(transparent)]
    Github(#[from] IiiGithubError),

    #[error("Failed to parse version: {0}")]
    VersionParse(String),

    #[error(transparent)]
    Download(#[from] download::DownloadAndInstallError),
}

/// Print the result of an update operation.
pub fn print_update_result(result: &Result<UpdateResult, UpdateError>) {
    match result {
        Ok(UpdateResult::Updated { binary, from, to }) => {
            if let Some(from) = from {
                eprintln!(
                    "  {} {} updated: {} → {}",
                    "✓".green(),
                    binary,
                    from.to_string().dimmed(),
                    to.to_string().green(),
                );
            } else {
                eprintln!(
                    "  {} {} installed: v{}",
                    "✓".green(),
                    binary,
                    to.to_string().green(),
                );
            }
        }
        Ok(UpdateResult::AlreadyUpToDate { binary, version }) => {
            eprintln!(
                "  {} {} is already up to date (v{})",
                "✓".green(),
                binary,
                version,
            );
        }
        Err(e) => {
            eprintln!("  {} {}", "error:".red(), e);
        }
    }
}
