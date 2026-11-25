//! Export-only wire contract and quarantine storage for verified LearnHarness
//! 0.6.1 managed profiles. This deliberately has no path into the active
//! LearnHarness profile or the synthetic reconciliation ledger.
use anyhow::{ensure, Context, Result};
use harness_sync_core::{
    learnharness_061, learnharness_070, sha256, Payload, MAX_FILE, MAX_FILES, MAX_HISTORY,
    MAX_TOTAL,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRecord {
    pub path: String,
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportManifest {
    pub version: u32,
    pub format: String,
    pub revision: String,
    /// The managed-profile adapter currently has no durable parent ledger.
    /// Empty ancestry is explicit; it must not be inferred from unrelated
    /// synthetic revisions.
    pub parents: Vec<String>,
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuarantinedRevision {
    pub revision: String,
    pub files: usize,
    pub bytes: u64,
}

fn quarantine_directory(root: &Path, format: &str) -> Result<std::path::PathBuf> {
    let name = match format {
        learnharness_061::FORMAT => "learnharness-0.6.1",
        learnharness_070::FORMAT => "learnharness-0.7.0",
        _ => anyhow::bail!("Unsupported LearnHarness schema"),
    };
    Ok(root.join("quarantine").join(name))
}

pub fn bundle_value(manifest: &ExportManifest, payload: &Payload) -> Result<serde_json::Value> {
    use base64::Engine as _;
    verify(manifest, payload)?;
    let records: Vec<_> = payload
        .iter()
        .map(|(path, bytes)| {
            serde_json::json!({
                "path": path,
                "data": base64::engine::general_purpose::STANDARD.encode(bytes),
            })
        })
        .collect();
    Ok(serde_json::json!({"manifest":manifest,"records":records}))
}

pub fn parse_bundle(value: &serde_json::Value) -> Result<(ExportManifest, Payload)> {
    use base64::Engine as _;
    let object = value
        .as_object()
        .context("Managed export bundle is not an object")?;
    ensure!(
        object.len() == 2,
        "Unsupported managed export bundle fields"
    );
    let manifest: ExportManifest = serde_json::from_value(
        object
            .get("manifest")
            .context("Managed export manifest missing")?
            .clone(),
    )?;
    manifest.validate()?;
    let rows = object
        .get("records")
        .and_then(serde_json::Value::as_array)
        .context("Managed export records missing")?;
    ensure!(
        rows.len() == manifest.files.len(),
        "Managed export inventory is incomplete"
    );
    let mut payload = Payload::new();
    for row in rows {
        let record = row.as_object().context("Malformed managed export record")?;
        ensure!(
            record.len() == 2,
            "Unsupported managed export record fields"
        );
        let path = record
            .get("path")
            .and_then(serde_json::Value::as_str)
            .context("Managed export path missing")?
            .to_owned();
        let data = record
            .get("data")
            .and_then(serde_json::Value::as_str)
            .context("Managed export bytes missing")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .context("Malformed managed export encoding")?;
        ensure!(
            payload.insert(path, bytes).is_none(),
            "Duplicate managed export path"
        );
    }
    verify(&manifest, &payload)?;
    Ok((manifest, payload))
}

pub fn load(root: &Path, revision: &str) -> Result<(ExportManifest, Payload)> {
    ensure!(valid_hash(revision), "Invalid quarantined revision id");
    let legacy = root.join("quarantine/learnharness-0.6.1").join(revision);
    let newer = root.join("quarantine/learnharness-0.7.0").join(revision);
    ensure!(
        !(legacy.try_exists()? && newer.try_exists()?),
        "Ambiguous quarantined revision across LearnHarness versions"
    );
    let target = if newer.try_exists()? { newer } else { legacy };
    crate::storage::safe_path(&target)?;
    let stored: ExportManifest = serde_json::from_slice(&crate::storage::read(
        &target.join("manifest.json"),
        crate::storage::MAX_METADATA,
    )?)?;
    ensure!(
        stored.revision == revision,
        "Quarantined manifest does not match its address"
    );
    ensure!(
        target.starts_with(quarantine_directory(root, &stored.format)?),
        "Quarantined LearnHarness version does not match its manifest"
    );
    let expected_directories: BTreeSet<_> = stored
        .files
        .iter()
        .filter_map(|f| f.path.split_once('/').map(|(d, _)| d.to_owned()))
        .collect();
    let actual: BTreeSet<_> = fs::read_dir(&target)?
        .map(|e| e.map(|v| v.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    let mut inventory = expected_directories.clone();
    inventory.insert("manifest.json".to_owned());
    ensure!(
        actual == inventory,
        "Quarantined revision has an unexpected directory inventory"
    );
    let mut payload = Payload::new();
    for record in &stored.files {
        let bytes = crate::storage::read(&target.join(&record.path), MAX_FILE)?;
        ensure!(
            payload.insert(record.path.clone(), bytes).is_none(),
            "Duplicate quarantined record"
        );
    }
    verify(&stored, &payload)?;
    for dir in expected_directories {
        let expected: BTreeSet<_> = stored
            .files
            .iter()
            .filter_map(|f| f.path.strip_prefix(&format!("{dir}/")).map(str::to_owned))
            .collect();
        let actual: BTreeSet<_> = fs::read_dir(target.join(&dir))?
            .map(|e| e.map(|v| v.file_name().to_string_lossy().into_owned()))
            .collect::<std::io::Result<_>>()?;
        ensure!(
            actual == expected,
            "Quarantined revision contains an unexpected file"
        );
    }
    Ok((stored, payload))
}

pub fn list(root: &Path) -> Result<Vec<QuarantinedRevision>> {
    let mut items = Vec::new();
    for format in [learnharness_061::FORMAT, learnharness_070::FORMAT] {
        let quarantine = quarantine_directory(root, format)?;
        if !quarantine.exists() {
            continue;
        }
        for entry in fs::read_dir(&quarantine)? {
            let entry = entry?;
            let meta = fs::symlink_metadata(entry.path())?;
            ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "Unsafe quarantine entry"
            );
            let revision = entry.file_name().to_string_lossy().into_owned();
            let (manifest, _) = load(root, &revision)?;
            items.push(QuarantinedRevision {
                revision,
                files: manifest.files.len(),
                bytes: manifest.files.iter().map(|f| f.size).sum(),
            });
        }
    }
    items.sort_by(|a, b| a.revision.cmp(&b.revision));
    Ok(items)
}

impl ExportManifest {
    pub fn from_payload(payload: &Payload) -> Result<Self> {
        Self::from_payload_for_format(payload, learnharness_061::FORMAT)
    }

    pub fn from_payload_for_format(payload: &Payload, format: &str) -> Result<Self> {
        validate_payload_for_format(format, payload)?;
        ensure!(
            !payload.is_empty(),
            "Empty managed LearnHarness profile cannot be exported"
        );
        let directories: BTreeSet<String> = payload
            .keys()
            .filter_map(|path| path.split_once('/').map(|(dir, _)| dir.to_owned()))
            .collect();
        let objects: Vec<_> = payload
            .iter()
            .map(|(path, bytes)| (path, sha256(bytes)))
            .collect();
        let revision = if format == learnharness_061::FORMAT {
            // Preserve addresses for every existing 0.6.1 quarantine and backup.
            sha256(&serde_json::to_vec(&(&directories, &objects))?)
        } else {
            // The version is part of the new address, even for identical bytes.
            sha256(&serde_json::to_vec(&(format, &directories, &objects))?)
        };
        let files = payload
            .iter()
            .map(|(path, bytes)| FileRecord {
                path: path.clone(),
                hash: sha256(bytes),
                size: bytes.len() as u64,
            })
            .collect();
        Ok(Self {
            version: 1,
            format: format.into(),
            revision,
            parents: Vec::new(),
            files,
        })
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "Unsupported LearnHarness export manifest version"
        );
        ensure!(
            self.format == learnharness_061::FORMAT || self.format == learnharness_070::FORMAT,
            "Unsupported LearnHarness schema"
        );
        ensure!(
            self.parents.is_empty(),
            "Unsupported managed profile ancestry"
        );
        ensure!(
            !self.files.is_empty() && self.files.len() <= MAX_FILES,
            "Invalid export inventory size"
        );
        ensure!(
            valid_hash(&self.revision),
            "Invalid LearnHarness revision id"
        );
        let mut prior: Option<&str> = None;
        let mut total = 0u64;
        for file in &self.files {
            ensure!(
                supported_path(&self.format, &file.path),
                "Unsupported LearnHarness record path"
            );
            ensure!(valid_hash(&file.hash), "Invalid record hash");
            ensure!(
                file.size <= MAX_FILE as u64,
                "LearnHarness record exceeds size limit"
            );
            ensure!(
                prior.is_none_or(|p| p < file.path.as_str()),
                "Unordered or duplicate export paths"
            );
            prior = Some(&file.path);
            total = total
                .checked_add(file.size)
                .context("Export inventory size overflow")?;
            ensure!(
                total <= MAX_TOTAL as u64,
                "LearnHarness export exceeds total size limit"
            );
        }
        Ok(())
    }
}

pub fn validate_payload(payload: &Payload) -> Result<()> {
    validate_payload_for_format(learnharness_061::FORMAT, payload)
}

pub fn validate_payload_for_format(format: &str, payload: &Payload) -> Result<()> {
    ensure!(payload.len() <= MAX_FILES, "Too many LearnHarness records");
    let total = payload.values().try_fold(0usize, |n, bytes| {
        n.checked_add(bytes.len()).context("Profile size overflow")
    })?;
    ensure!(
        total <= MAX_TOTAL,
        "LearnHarness profile exceeds total size limit"
    );
    ensure!(
        payload.keys().all(|p| supported_path(format, p)),
        "Unsupported LearnHarness record path"
    );
    match format {
        learnharness_061::FORMAT => learnharness_061::validate(payload),
        learnharness_070::FORMAT => learnharness_070::validate(payload),
        _ => Err("Unsupported LearnHarness schema".into()),
    }
    .map_err(anyhow::Error::msg)
}

fn supported_path(format: &str, path: &str) -> bool {
    match format {
        learnharness_061::FORMAT => learnharness_061::supported_path(path),
        learnharness_070::FORMAT => learnharness_070::supported_path(path),
        _ => false,
    }
}

pub fn verify(manifest: &ExportManifest, payload: &Payload) -> Result<()> {
    manifest.validate()?;
    validate_payload_for_format(&manifest.format, payload)?;
    ensure!(
        manifest.files.len() == payload.len(),
        "Export inventory does not match content"
    );
    for record in &manifest.files {
        let bytes = payload
            .get(&record.path)
            .context("Export is missing a required record")?;
        ensure!(
            bytes.len() as u64 == record.size && sha256(bytes) == record.hash,
            "Export record integrity failure"
        );
    }
    ensure!(
        ExportManifest::from_payload_for_format(payload, &manifest.format)?.revision
            == manifest.revision,
        "LearnHarness revision digest mismatch"
    );
    Ok(())
}

/// Persist verified data beneath the native engine's private data root. A
/// repeated delivery is idempotent only when the existing exact tree verifies.
pub fn stage(root: &Path, manifest: &ExportManifest, payload: &Payload) -> Result<bool> {
    verify(manifest, payload)?;
    let quarantine = quarantine_directory(root, &manifest.format)?;
    crate::storage::private_dir(&quarantine)?;
    let target = quarantine.join(&manifest.revision);
    if target.try_exists()? {
        let target_meta = fs::symlink_metadata(&target)?;
        ensure!(
            target_meta.is_dir() && !target_meta.file_type().is_symlink(),
            "Unsafe existing quarantine revision"
        );
        let stored_manifest: ExportManifest = serde_json::from_slice(&crate::storage::read(
            &target.join("manifest.json"),
            crate::storage::MAX_METADATA,
        )?)?;
        ensure!(
            stored_manifest == *manifest,
            "Existing quarantine receipt differs from incoming manifest"
        );
        let mut existing = Payload::new();
        for file in &manifest.files {
            let bytes = crate::storage::read(&target.join(&file.path), MAX_FILE)?;
            existing.insert(file.path.clone(), bytes);
        }
        verify(manifest, &existing)?;
        return Ok(false);
    }
    let (stored_bytes, stored_files) = quarantine_usage(&quarantine)?;
    let incoming_bytes = payload.values().try_fold(0u64, |n, b| {
        n.checked_add(b.len() as u64)
            .context("Quarantine size overflow")
    })?;
    ensure!(
        stored_bytes.saturating_add(incoming_bytes) <= 1024 * 1024 * 1024,
        "LearnHarness quarantine storage quota reached; existing staged history was retained"
    );
    ensure!(
        stored_files.saturating_add(payload.len()).saturating_add(1) <= 100_000,
        "LearnHarness quarantine file-count quota reached"
    );
    let temp = quarantine.join(format!(".incoming-{}", crate::storage::nonce()?));
    crate::storage::private_dir(&temp)?;
    let result = (|| {
        let manifest_bytes = serde_json::to_vec(manifest)?;
        ensure!(
            manifest_bytes.len() <= crate::storage::MAX_METADATA,
            "LearnHarness manifest exceeds limit"
        );
        crate::storage::atomic(&temp.join("manifest.json"), &manifest_bytes)?;
        for (path, bytes) in payload {
            let (directory, filename) = path.split_once('/').context("Invalid export path")?;
            let dir = temp.join(directory);
            crate::storage::private_dir(&dir)?;
            crate::storage::atomic(&dir.join(filename), bytes)?;
        }
        let mut readback = Payload::new();
        for file in &manifest.files {
            readback.insert(
                file.path.clone(),
                crate::storage::read(&temp.join(&file.path), MAX_FILE)?,
            );
        }
        verify(manifest, &readback)?;
        crate::storage::sync_dir(&temp)?;
        fs::rename(&temp, &target)?;
        crate::storage::sync_dir(&quarantine)?;
        Ok::<_, anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temp);
    }
    result?;
    Ok(true)
}

fn quarantine_usage(root: &Path) -> Result<(u64, usize)> {
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "Symlink in LearnHarness quarantine"
            );
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                ensure!(
                    metadata.is_file(),
                    "Unexpected entry in LearnHarness quarantine"
                );
                bytes = bytes
                    .checked_add(metadata.len())
                    .context("Quarantine size overflow")?;
                files += 1;
            }
            ensure!(
                pending.len() <= MAX_HISTORY,
                "Quarantine directory count limit reached"
            );
        }
    }
    Ok((bytes, files))
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn payload() -> Payload {
        let chat = serde_json::json!({
            "version":2,"id":"chat_abc123_1234abcd","title":"Disposable export",
            "createdAt":"2026-11-24T10:00:00Z","updatedAt":"2026-11-24T10:00:00Z",
            "messages":[{"role":"user","id":"msg_abc123_1234abcd","content":"hello ✓","timestamp":1}]
        });
        [(
            "chats/chat_abc123_1234abcd.json".to_owned(),
            serde_json::to_vec(&chat).unwrap(),
        )]
        .into_iter()
        .collect()
    }

    #[test]
    fn validated_manifest_stages_and_duplicate_delivery_is_idempotent() {
        let payload = payload();
        let manifest = ExportManifest::from_payload(&payload).unwrap();
        let temp = TempDir::new().unwrap();
        assert!(stage(temp.path(), &manifest, &payload).unwrap());
        assert!(!stage(temp.path(), &manifest, &payload).unwrap());
        let file = temp
            .path()
            .join("quarantine/learnharness-0.6.1")
            .join(&manifest.revision)
            .join("chats/chat_abc123_1234abcd.json");
        assert_eq!(
            fs::read(file).unwrap(),
            payload.values().next().unwrap().as_slice()
        );
    }

    #[test]
    fn malformed_or_tampered_content_is_never_staged() {
        let payload = payload();
        let manifest = ExportManifest::from_payload(&payload).unwrap();
        let mut oversized = manifest.clone();
        oversized.files[0].size = MAX_FILE as u64 + 1;
        assert!(oversized.validate().is_err());
        let mut damaged = payload.clone();
        damaged
            .get_mut("chats/chat_abc123_1234abcd.json")
            .unwrap()
            .push(b' ');
        let temp = TempDir::new().unwrap();
        assert!(stage(temp.path(), &manifest, &damaged).is_err());
        assert!(!temp
            .path()
            .join("quarantine/learnharness-0.6.1")
            .join(manifest.revision)
            .exists());
    }
}
