//! `cox self update [--version v]` (T12.2): downloads the release archive
//! for this platform from GitHub, verifies its `.sha256` checksum, and
//! replaces the running binary. Refuses to install without a matching
//! checksum; refuses Windows (rename-over-running needs a dance this does
//! not do).

use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// `listepo/cox` releases carry `cox-<target>.tar.xz` built by cargo-dist.
const REPO: &str = "listepo/cox";

/// This platform's cargo-dist target triple, if releases build it.
fn target() -> anyhow::Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        (os, arch) => anyhow::bail!("no cox release for {os}/{arch}"),
    }
}

fn asset_base(tag: &str, target: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/cox-{target}.tar.xz")
}

/// Latest release tag via the GitHub API (public, no auth).
async fn latest_tag(client: &reqwest::Client) -> anyhow::Result<String> {
    let tag: serde_json::Value = client
        .get(format!("https://api.github.com/{REPO}/releases/latest"))
        .header("User-Agent", "cox-self-update")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    tag.get("tag_name")
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("latest release has no tag_name"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Downloads `url` fully.
async fn fetch(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    Ok(client
        .get(url)
        .header("User-Agent", "cox-self-update")
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec())
}

/// Updates to `version` (a tag like `v0.1.0`) or the latest release.
pub async fn run(version: Option<String>) -> anyhow::Result<()> {
    if cfg!(windows) {
        anyhow::bail!("cox self update is not supported on Windows yet");
    }
    let target = target()?;
    let current = env!("CARGO_PKG_VERSION");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let tag = match version {
        Some(v) => v,
        None => latest_tag(&client).await?,
    };
    if tag.trim_start_matches('v') == current {
        println!("already at latest ({current})");
        return Ok(());
    }
    let base = asset_base(&tag, target);
    println!("downloading {base}");
    let archive = fetch(&client, &base).await?;
    let checksum = fetch(&client, &format!("{base}.sha256")).await?;
    let checksum = String::from_utf8_lossy(&checksum);
    let want = checksum
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("checksum file is empty"))?;
    let got = sha256_hex(&archive);
    if got != want {
        anyhow::bail!("checksum mismatch for {base}: expected {want}, got {got}");
    }
    // cargo-dist archives hold the binary at the top level; unpack it.
    let exe = std::env::current_exe()?;
    let dir: PathBuf = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no binary dir"))?
        .into();
    let staged = dir.join("cox.update");
    unpack_cox(&archive, &staged)?;
    self_replace(&exe, &staged)?;
    println!("updated to {tag}");
    Ok(())
}

/// Extracts the `cox` binary from the `.tar.xz` archive to `dest` with the
/// system `tar` (BSD and GNU both read `.tar.xz`; no new C-linked
/// dependency for one extraction per update).
fn unpack_cox(archive: &[u8], dest: &PathBuf) -> anyhow::Result<()> {
    let dir: PathBuf = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no binary dir"))?
        .into();
    let tmp = dir.join(".cox-update-tmp");
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir(&tmp)?;
    let cleanup = || {
        let _ = std::fs::remove_dir_all(&tmp);
    };
    let archive_path = tmp.join("cox.tar.xz");
    if let Err(e) = (|| -> anyhow::Result<()> {
        std::fs::write(&archive_path, archive)?;
        let status = std::process::Command::new("tar")
            .args(["-xJf"])
            .arg(&archive_path)
            .args(["-C"])
            .arg(&tmp)
            .arg("cox")
            .status()
            .map_err(|e| anyhow::anyhow!("tar not found: {e}"))?;
        if !status.success() {
            anyhow::bail!("tar failed: {status}");
        }
        std::fs::rename(tmp.join("cox"), dest)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    })() {
        cleanup();
        return Err(e);
    }
    cleanup();
    Ok(())
}

/// Atomically swaps the staged binary over the running one (Unix rename).
fn self_replace(exe: &PathBuf, staged: &PathBuf) -> anyhow::Result<()> {
    std::fs::rename(staged, exe)?;
    Ok(())
}
