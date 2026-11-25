//! Reviewed upstream build catalog, independent of the persisted-data parser.
//! An unknown AppImage is never selected merely because its version looks newer.
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedBuild {
    pub version: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub wire_format: String,
    pub vault: String,
    pub release_key: String,
    pub source_tag: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportMatrix {
    pub matrix_version: u32,
    pub schema_profile: String,
    pub schema_sources: std::collections::BTreeMap<String, String>,
    pub builds: Vec<VerifiedBuild>,
}

#[derive(Debug, Clone)]
pub struct SelectedBuild {
    pub build: VerifiedBuild,
    pub source: PathBuf,
    pub newer_unverified: Option<String>,
}

pub fn matrix() -> &'static SupportMatrix {
    static MATRIX: OnceLock<SupportMatrix> = OnceLock::new();
    MATRIX.get_or_init(|| {
        let value: SupportMatrix =
            serde_json::from_str(include_str!("../../../support/learnharness-builds.json"))
                .expect("Reviewed LearnHarness build catalog must parse");
        validate_matrix(&value).expect("Reviewed LearnHarness build catalog must be valid");
        value
    })
}

fn version_parts(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.');
    let result = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(result)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_matrix(value: &SupportMatrix) -> Result<()> {
    ensure!(value.matrix_version == 1, "Unknown support-matrix version");
    ensure!(
        value.schema_profile == "chat-v2-graph-v1-source-organization-v1-strict-text",
        "Unknown schema profile"
    );
    ensure!(
        value.schema_sources.len() == 4 && value.schema_sources.values().all(|v| valid_hash(v)),
        "Invalid upstream schema evidence"
    );
    ensure!(!value.builds.is_empty(), "Support matrix is empty");
    let mut versions = BTreeSet::new();
    let mut digests = BTreeSet::new();
    for build in &value.builds {
        ensure!(
            version_parts(&build.version).is_some(),
            "Invalid supported version"
        );
        ensure!(
            build.filename == format!("LearnHarness-{}-x86_64.AppImage", build.version),
            "Invalid supported asset name"
        );
        ensure!(
            build.source_tag == format!("v{}", build.version),
            "Invalid upstream tag"
        );
        ensure!(
            build.size > 100_000_000 && build.size < 500_000_000 && valid_hash(&build.sha256),
            "Invalid executable identity"
        );
        ensure!(
            matches!(build.release_key.as_str(), "061" | "070"),
            "Unknown managed release key"
        );
        ensure!(
            matches!(build.vault.as_str(), "vault" | "vault-070"),
            "Unknown managed vault"
        );
        ensure!(
            matches!(
                build.wire_format.as_str(),
                "learnharness-0.6.1-disposable-v1" | "learnharness-0.7.0-disposable-v1"
            ),
            "Unknown managed wire format"
        );
        ensure!(
            (build.release_key == "061"
                && build.vault == "vault"
                && build.wire_format == "learnharness-0.6.1-disposable-v1")
                || (build.release_key == "070"
                    && build.vault == "vault-070"
                    && build.wire_format == "learnharness-0.7.0-disposable-v1"),
            "Managed release identity mismatch"
        );
        ensure!(
            versions.insert(build.version.clone()) && digests.insert(build.sha256.clone()),
            "Duplicate verified build identity"
        );
    }
    Ok(())
}

fn installed_version(name: &str) -> Option<&str> {
    let value = name
        .strip_prefix("LearnHarness-")?
        .strip_suffix("-x86_64.AppImage")?;
    version_parts(value).map(|_| value)
}

fn cache_path(data: &Path, build: &VerifiedBuild) -> PathBuf {
    data.join("io.github.tenkejj.learnharness-sync/managed/pinned-images")
        .join(format!("{}.AppImage", build.sha256))
}

pub fn verify_image(path: &Path, build: &VerifiedBuild) -> Result<()> {
    let mut source = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("Cannot open pinned LearnHarness image: {}", path.display()))?;
    let metadata = source.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.len() == build.size
            && metadata.permissions().mode() & 0o111 != 0,
        "LearnHarness image size, type or permissions differ from the verified release"
    );
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 64 * 1024];
    let mut count = 0u64;
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count.checked_add(n as u64).context("Image size overflow")?;
        ensure!(
            count <= build.size,
            "LearnHarness image grew while being checked"
        );
        digest.update(&buffer[..n]);
    }
    let actual: String = digest
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    ensure!(
        count == build.size && actual == build.sha256,
        "LearnHarness executable digest does not match the reviewed build"
    );
    Ok(())
}

pub fn detect(home: &Path, data: &Path) -> Result<SelectedBuild> {
    ensure!(
        home.is_absolute() && data.is_absolute(),
        "Absolute Linux profile locations are required"
    );
    let applications = home.join("Applications");
    let installed = match fs::read_dir(&applications) {
        Ok(rows) => Some(rows),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut seen = Vec::new();
    if let Some(installed) = installed {
        for entry in installed {
            let name = entry?.file_name();
            if let Some(version) = installed_version(&name.to_string_lossy()) {
                seen.push(version.to_owned());
            }
        }
    }
    seen.sort_by_key(|v| version_parts(v).unwrap());
    let newest = seen.last().cloned();
    let mut verified = Vec::new();
    for build in &matrix().builds {
        let path = applications.join(&build.filename);
        if path.try_exists()? || path.is_symlink() {
            verify_image(&path, build)?;
            verified.push((build.clone(), path, true));
        }
        let cached = cache_path(data, build);
        if cached.try_exists()? || cached.is_symlink() {
            verify_image(&cached, build)?;
            verified.push((build.clone(), cached, false));
        }
    }
    verified.sort_by_key(|(b, _, installed)| (version_parts(&b.version).unwrap(), *installed));
    let unverified = newest.filter(|v| !matrix().builds.iter().any(|b| b.version == *v));
    if let Some((build, source, _)) = verified.pop() {
        let newer_unverified =
            unverified.filter(|v| version_parts(v) > version_parts(&build.version));
        return Ok(SelectedBuild {
            build,
            source,
            newer_unverified,
        });
    }
    if let Some(version) = unverified {
        bail!("LearnHarness {version} is not verified. Keep a pinned 0.7.0 or 0.6.1 AppImage for managed synchronization.");
    }
    bail!("Install a verified LearnHarness 0.7.0 or 0.6.1 AppImage in ~/Applications.")
}

pub fn detect_from_env() -> Result<SelectedBuild> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is unavailable")?);
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    detect(&home, &data)
}

/// Keep the already verified AppImage in this user's private application data.
/// It is checked again on every later use and never copied into the UID vault.
pub fn retain_verified_image(selected: &SelectedBuild, data: &Path) -> Result<PathBuf> {
    let target = cache_path(data, &selected.build);
    let directory = target.parent().context("Pinned image directory missing")?;
    crate::storage::private_dir(directory)?;
    if target.try_exists()? || target.is_symlink() {
        verify_image(&target, &selected.build)?;
        return Ok(target);
    }
    let temporary = directory.join(format!(".image-{}", crate::storage::nonce()?));
    let result = (|| {
        let mut input = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&selected.source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&temporary)?;
        std::io::copy(&mut input, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        verify_image(&temporary, &selected.build)?;
        fs::rename(&temporary, &target)?;
        crate::storage::sync_dir(directory)?;
        verify_image(&target, &selected.build)?;
        Ok::<_, anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn catalog_is_reviewed_and_versions_are_distinct_from_schema() {
        let builds = &matrix().builds;
        assert_eq!(
            builds
                .iter()
                .map(|b| b.version.as_str())
                .collect::<Vec<_>>(),
            ["0.6.1", "0.7.0"]
        );
        assert_ne!(builds[0].sha256, builds[1].sha256);
        assert_ne!(builds[0].wire_format, builds[1].wire_format);
        assert_eq!(
            matrix().schema_profile,
            "chat-v2-graph-v1-source-organization-v1-strict-text"
        );
    }

    #[test]
    fn unknown_upgrade_is_detected_without_becoming_trusted() {
        let temp = TempDir::new().unwrap();
        let apps = temp.path().join("home/Applications");
        fs::create_dir_all(&apps).unwrap();
        File::create(apps.join("LearnHarness-0.8.0-x86_64.AppImage")).unwrap();
        let error = detect(&temp.path().join("home"), &temp.path().join("data")).unwrap_err();
        assert!(error.to_string().contains("0.8.0 is not verified"));
        File::create(apps.join("LearnHarness-0.7.0-x86_64.AppImage")).unwrap();
        let error = detect(&temp.path().join("home"), &temp.path().join("data")).unwrap_err();
        assert!(error.to_string().contains("size, type or permissions"));
    }
}
