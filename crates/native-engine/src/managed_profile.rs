//! Strict 0.6.1 managed-profile snapshots and durable tree operations.
//!
//! This module has no `/lab`, peer labels, or fixture assumptions. Callers must
//! establish exclusive writer control before reading or replacing a profile.
use crate::storage::{atomic, private_dir, read, safe_path, sync_dir, MAX_METADATA};
use anyhow::{ensure, Context, Result};
use harness_sync_core::{learnharness_061, Payload, MAX_FILE, MAX_FILES, MAX_TOTAL};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::CString,
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::Path,
    process::Command,
};

#[derive(Default, PartialEq, Eq)]
pub struct Tree {
    pub directories: BTreeSet<String>,
    pub files: Payload,
}

impl Tree {
    pub fn read(root: &Path) -> Result<Self> {
        safe_path(root)?;
        ensure!(root.is_dir(), "Managed data directory is unavailable");
        let mut tree = Self::default();
        let mut total = 0usize;
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Invalid LearnHarness filename"))?;
            if name == "settings.json" && entry.file_type()?.is_file() {
                tree.files.insert(name, read(&entry.path(), MAX_METADATA)?);
                continue;
            }
            ensure!(
                entry.file_type()?.is_dir()
                    && matches!(name.as_str(), "chats" | "graph" | "jobs" | "plugins"),
                "Unknown managed profile inventory"
            );
            tree.directories.insert(name.clone());
            for file in fs::read_dir(entry.path())? {
                let file = file?;
                let filename = file
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Invalid LearnHarness record name"))?;
                let path = format!("{name}/{filename}");
                ensure!(
                    file.file_type()?.is_file() && learnharness_061::supported_path(&path),
                    "Unsupported durable LearnHarness record"
                );
                ensure!(
                    tree.files.len() < MAX_FILES,
                    "Too many LearnHarness records"
                );
                let bytes = read(&file.path(), MAX_FILE.min(MAX_TOTAL.saturating_sub(total)))?;
                total = total
                    .checked_add(bytes.len())
                    .context("Managed profile size overflow")?;
                ensure!(
                    total <= MAX_TOTAL,
                    "Managed LearnHarness profile exceeds size limit"
                );
                tree.files.insert(path, bytes);
            }
        }
        let mut records = tree.files.clone();
        records.remove("settings.json");
        if !records.is_empty() {
            learnharness_061::validate(&records).map_err(anyhow::Error::msg)?;
        }
        Ok(tree)
    }

    pub fn records(root: &Path) -> Result<Payload> {
        let mut tree = Self::read(root)?;
        tree.files.remove("settings.json");
        Ok(tree.files)
    }

    pub fn snapshot_summary(root: &Path) -> Result<serde_json::Value> {
        let tree = Self::read(root)?;
        let mut records = tree.files.clone();
        records.remove("settings.json");
        let bytes = records
            .values()
            .map(|value| value.len() as u64)
            .sum::<u64>();
        let manifest: Vec<_> = records
            .iter()
            .map(|(path, bytes)| {
                serde_json::json!({
                    "path": path,
                    "bytes": bytes.len(),
                    "sha256": harness_sync_core::sha256(bytes),
                })
            })
            .collect();
        let manifest_size = serde_json::to_vec(&manifest)?.len();
        ensure!(
            manifest_size <= 48 * 1024,
            "Managed profile manifest exceeds the local IPC response limit"
        );
        let digest = Self {
            directories: tree.directories,
            files: records,
        }
        .id()?;
        Ok(serde_json::json!({
            "format": harness_sync_core::learnharness_061::FORMAT,
            "digest": digest,
            "files": manifest.len(),
            "bytes": bytes,
            "manifest": manifest,
        }))
    }

    /// A consent-gated export response for the isolated helper pipe. The
    /// helper serializes only validated durable records (settings/credentials
    /// are excluded) and the regular-user caller rechecks every hash.
    pub fn export_bundle(root: &Path) -> Result<serde_json::Value> {
        use base64::Engine as _;
        let records = Self::records(root)?;
        let manifest = crate::learnharness_export::ExportManifest::from_payload(&records)?;
        let encoded: Vec<_> = records
            .iter()
            .map(|(path, bytes)| {
                serde_json::json!({
                    "path": path,
                    "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                })
            })
            .collect();
        let value = serde_json::json!({
            "manifest": manifest,
            "records": encoded,
        });
        ensure!(
            serde_json::to_vec(&value)?.len() <= 180 * 1024 * 1024,
            "Managed export exceeds IPC response limit"
        );
        Ok(value)
    }

    pub fn id(&self) -> Result<String> {
        let objects: Vec<_> = self
            .files
            .iter()
            .map(|(name, bytes)| (name, harness_sync_core::sha256(bytes)))
            .collect();
        Ok(harness_sync_core::sha256(&serde_json::to_vec(&(
            &self.directories,
            objects,
        ))?))
    }

    pub fn copy_to(&self, target: &Path) -> Result<()> {
        private_dir(target)?;
        for dir in &self.directories {
            private_dir(&target.join(dir))?;
        }
        for (key, bytes) in &self.files {
            atomic(&target.join(key), bytes)?;
        }
        sync_dir(target)?;
        sync_dir(target.parent().context("Missing managed profile parent")?)?;
        ensure!(
            Self::read(target)? == *self,
            "Managed profile copy failed verification"
        );
        Ok(())
    }

    pub fn from_records(records: Payload, settings: Option<Vec<u8>>) -> Result<Self> {
        learnharness_061::validate(&records).map_err(anyhow::Error::msg)?;
        let mut tree = Self {
            files: records,
            ..Self::default()
        };
        if let Some(bytes) = settings {
            ensure!(
                bytes.len() <= MAX_METADATA,
                "LearnHarness settings exceed size limit"
            );
            tree.files.insert("settings.json".into(), bytes);
        }
        for path in tree.files.keys() {
            if let Some((directory, _)) = path.split_once('/') {
                tree.directories.insert(directory.into());
            }
        }
        Ok(tree)
    }
}

/// Monotonic, fail-closed merge for the supported 0.6.1 record contract.
/// Existing sessions and graph identities are retained; a same-identity
/// semantic edit becomes a conflict instead of an overwrite.
pub fn merge_records(current: &Payload, incoming: &Payload) -> Result<Payload> {
    learnharness_061::validate(current).map_err(anyhow::Error::msg)?;
    learnharness_061::validate(incoming).map_err(anyhow::Error::msg)?;
    let mut merged = incoming.clone();
    for (path, old) in current {
        let Some(new) = incoming.get(path) else {
            anyhow::bail!(
                "Incoming revision omits local record {path}; both histories were preserved"
            );
        };
        if old == new {
            continue;
        }
        let bytes = if path.starts_with("chats/") {
            merge_chat(old, new)?
        } else if path == "graph/positions.json" {
            merge_positions(old, new)?
        } else if matches!(
            path.as_str(),
            "graph/nodes.jsonl" | "graph/chains.jsonl" | "graph/coverage.jsonl"
        ) {
            merge_graph_lines(path, old, new)?
        } else {
            anyhow::bail!("Concurrent edit to {path}; both histories were preserved");
        };
        merged.insert(path.clone(), bytes);
    }
    learnharness_061::validate(&merged).map_err(anyhow::Error::msg)?;
    Ok(merged)
}

fn merge_chat(old: &[u8], new: &[u8]) -> Result<Vec<u8>> {
    let mut left: serde_json::Value = serde_json::from_slice(old)?;
    let right: serde_json::Value = serde_json::from_slice(new)?;
    for key in ["version", "id", "title", "createdAt", "kind", "settings"] {
        if left.get(key) != right.get(key) {
            anyhow::bail!(
                "Concurrent metadata edit to a conversation; both histories were preserved"
            );
        }
    }
    for (field, identity) in [
        ("messages", "id"),
        ("turns", "id"),
        ("extractions", "startedAt"),
    ] {
        let local_rows = left
            .get(field)
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut rows = Vec::new();
        let additions = right
            .get(field)
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut by_id = std::collections::BTreeMap::new();
        for row in local_rows {
            let id = stable_row_id(field, identity, &row)?;
            ensure!(
                !by_id.contains_key(&id),
                "Duplicate local conversation metadata identity"
            );
            by_id.insert(id, row.clone());
            rows.push(row);
        }
        for row in additions {
            let id = stable_row_id(field, identity, &row)?;
            if let Some(previous) = by_id.get(&id) {
                ensure!(
                    previous == &row,
                    "Conflicting edit to a conversation record; both histories were preserved"
                );
            } else {
                by_id.insert(id, row.clone());
                rows.push(row);
            }
        }
        left[field] = serde_json::Value::Array(rows);
    }
    let left_updated = left
        .get("updatedAt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let right_updated = right
        .get("updatedAt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    left["updatedAt"] = serde_json::Value::String(left_updated.max(right_updated).to_owned());
    Ok(serde_json::to_vec(&left)?)
}

fn stable_row_id(field: &str, key: &str, row: &serde_json::Value) -> Result<String> {
    if field == "extractions" {
        return Ok(harness_sync_core::sha256(&serde_json::to_vec(row)?));
    }
    row.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .context("Conversation record has no stable identity")
}

fn merge_graph_lines(path: &str, old: &[u8], new: &[u8]) -> Result<Vec<u8>> {
    use std::collections::BTreeMap;
    let decode = |bytes: &[u8]| -> Result<BTreeMap<String, serde_json::Value>> {
        let text = std::str::from_utf8(bytes)?;
        ensure!(text.ends_with('\n'), "Incomplete graph history");
        let mut rows = BTreeMap::new();
        for line in text.lines() {
            let row: serde_json::Value = serde_json::from_str(line)?;
            let id = if path == "graph/coverage.jsonl" {
                format!(
                    "{}:{}",
                    row["chatId"].as_str().context("Coverage chat id missing")?,
                    row["fp"].as_str().context("Coverage fingerprint missing")?
                )
            } else {
                row["id"]
                    .as_str()
                    .context("Graph identity missing")?
                    .to_owned()
            };
            rows.insert(id, row);
        }
        Ok(rows)
    };
    let mut rows = decode(old)?;
    for (id, incoming) in decode(new)? {
        if let Some(local) = rows.get_mut(&id) {
            match path {
                "graph/nodes.jsonl" => merge_node(local, &incoming)?,
                "graph/chains.jsonl" => merge_chain(local, &incoming)?,
                "graph/coverage.jsonl" => {
                    let tokens = local["tokens"]
                        .as_u64()
                        .unwrap_or(0)
                        .max(incoming["tokens"].as_u64().unwrap_or(0));
                    let folded = local["foldedAt"]
                        .as_str()
                        .unwrap_or("")
                        .max(incoming["foldedAt"].as_str().unwrap_or(""))
                        .to_owned();
                    local["tokens"] = serde_json::json!(tokens);
                    local["foldedAt"] = serde_json::Value::String(folded);
                }
                _ => anyhow::bail!("Unsupported graph merge path"),
            }
        } else {
            rows.insert(id, incoming);
        }
    }
    let mut bytes = Vec::new();
    for row in rows.values() {
        serde_json::to_writer(&mut bytes, row)?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn union_array(
    left: &mut serde_json::Value,
    right: &serde_json::Value,
    field: &str,
    key: &str,
) -> Result<()> {
    use std::collections::BTreeMap;
    let mut rows = BTreeMap::new();
    for row in left
        .get(field)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .chain(
            right
                .get(field)
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        )
    {
        let id = if key == "@canonical" {
            harness_sync_core::sha256(&serde_json::to_vec(&row)?)
        } else if key == "@provenance" {
            format!(
                "{}:{}",
                row["chatId"].as_str().unwrap_or(""),
                row["messageId"].as_str().unwrap_or("")
            )
        } else {
            row.get(key)
                .and_then(serde_json::Value::as_str)
                .context("Graph merge item lacks identity")?
                .to_owned()
        };
        if let Some(previous) = rows.get(&id) {
            ensure!(
                previous == &row,
                "Conflicting graph history record; both histories were preserved"
            );
        } else {
            rows.insert(id, row);
        }
    }
    left[field] = serde_json::Value::Array(rows.into_values().collect());
    Ok(())
}

fn merge_node(left: &mut serde_json::Value, right: &serde_json::Value) -> Result<()> {
    for key in ["id", "name", "statement", "createdAt"] {
        ensure!(
            left.get(key) == right.get(key),
            "Conflicting proposition identity or content; both histories were preserved"
        );
    }
    union_array(left, right, "aliases", "@canonical")?;
    union_array(left, right, "provenance", "@provenance")
}

fn merge_chain(left: &mut serde_json::Value, right: &serde_json::Value) -> Result<()> {
    for key in ["id", "premises", "conclusion", "createdAt"] {
        ensure!(
            left.get(key) == right.get(key),
            "Conflicting graph relationship; both histories were preserved"
        );
    }
    union_array(left, right, "provenance", "@provenance")
}

fn merge_positions(old: &[u8], new: &[u8]) -> Result<Vec<u8>> {
    let mut left: serde_json::Value = serde_json::from_slice(old)?;
    let right: serde_json::Value = serde_json::from_slice(new)?;
    let lp = left
        .get_mut("positions")
        .and_then(serde_json::Value::as_object_mut)
        .context("Invalid local graph layout")?;
    let rp = right
        .get("positions")
        .and_then(serde_json::Value::as_object)
        .context("Invalid incoming graph layout")?;
    for (id, point) in rp {
        lp.entry(id.clone()).or_insert_with(|| point.clone());
    }
    let updated = left["updatedAt"]
        .as_str()
        .unwrap_or("")
        .max(right["updatedAt"].as_str().unwrap_or(""))
        .to_owned();
    left["updatedAt"] = serde_json::Value::String(updated);
    Ok(serde_json::to_vec(&left)?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyTransaction {
    version: u32,
    old_generation: String,
    new_generation: String,
    incoming_revision: String,
    merged_revision: String,
    backup: String,
    state: String,
}

/// Apply one verified export by constructing an immutable new Btrfs
/// generation, preserving the prior generation as a separate read-only
/// subvolume, then atomically swapping the `current` pointer. The durable
/// journal remains until the actual LearnHarness process has reopened and
/// cleanly persisted the expected merged records.
pub fn apply_incoming(
    vault: &Path,
    manifest: &crate::learnharness_export::ExportManifest,
    incoming: &Payload,
) -> Result<serde_json::Value> {
    crate::learnharness_export::verify(manifest, incoming)?;
    ensure!(
        fs::read(vault.join("active"))? == b"apply\n",
        "Managed writer exclusion marker is missing"
    );
    ensure!(
        !vault.join("apply.pending.json").try_exists()?,
        "Another managed apply requires readback or recovery"
    );
    let old_generation = read_generation(vault)?;
    let old_path = vault.join(&old_generation);
    ensure!(
        subvolume_readonly(&old_path)?,
        "Current managed generation is writable"
    );
    let old_tree = Tree::read(&old_path.join("config/LearnHarness/data"))?;
    let mut local_records = old_tree.files.clone();
    let settings = local_records.remove("settings.json");
    let merged = if local_records.is_empty() {
        incoming.clone()
    } else {
        merge_records(&local_records, incoming)?
    };
    let merged_revision =
        crate::learnharness_export::ExportManifest::from_payload(&merged)?.revision;
    ensure!(
        local_records != *incoming,
        "This LearnHarness revision is already present in the managed profile"
    );
    let next_tree = Tree::from_records(merged.clone(), settings)?;

    let backups = vault.join("backups");
    private_dir(&backups)?;
    let backup_name = format!("backup-{}-{}", manifest.revision, uuid()?);
    let backup_path = backups.join(&backup_name);
    btrfs_snapshot(&old_path, &backup_path, true)?;
    ensure!(
        subvolume_readonly(&backup_path)?,
        "Independent managed backup is not immutable"
    );
    ensure!(
        Tree::read(&backup_path.join("config/LearnHarness/data"))? == old_tree,
        "Independent backup does not match the prior managed profile"
    );
    sync_dir(&backups)?;

    let new_generation = format!("session.{}", uuid()?);
    let new_path = vault.join(&new_generation);
    btrfs_snapshot(&old_path, &new_path, false)?;
    fs::set_permissions(&new_path, fs::Permissions::from_mode(0o700))?;
    let data = new_path.join("config/LearnHarness/data");
    let staging = new_path.join(format!("config/LearnHarness/data.next.{}", uuid()?));
    next_tree.copy_to(&staging)?;
    exchange_directories(&staging, &data)?;
    fs::remove_dir_all(&staging)?;
    sync_dir(data.parent().context("Managed data parent missing")?)?;
    ensure!(
        Tree::read(&data)? == next_tree,
        "Staged managed profile readback failed"
    );
    btrfs_set_readonly(&new_path)?;
    ensure!(
        subvolume_readonly(&new_path)?,
        "New managed generation is not immutable"
    );

    let transaction = ApplyTransaction {
        version: 1,
        old_generation: old_generation.clone(),
        new_generation: new_generation.clone(),
        incoming_revision: manifest.revision.clone(),
        merged_revision: merged_revision.clone(),
        backup: backup_name.clone(),
        state: "awaiting_application_readback".into(),
    };
    crate::storage::atomic(
        &vault.join("apply.pending.json"),
        &serde_json::to_vec(&transaction)?,
    )?;
    sync_dir(vault)?;
    set_current(vault, &new_generation)?;
    Ok(serde_json::json!({
        "state":"awaiting_readback",
        "incoming_revision":manifest.revision,
        "merged_revision":merged_revision,
        "backup":backup_name,
        "applied":true,
        "application_verified":false,
    }))
}

pub fn confirm_readback(vault: &Path, incoming_revision: &str) -> Result<serde_json::Value> {
    let path = vault.join("apply.pending.json");
    if !path.try_exists()? {
        return Ok(serde_json::json!({"confirmed":false}));
    }
    let mut transaction = read_transaction(vault)?;
    ensure!(
        transaction.incoming_revision == incoming_revision,
        "Readback request does not match the pending revision"
    );
    let current = read_generation(vault)?;
    let records = Tree::records(&vault.join(current).join("config/LearnHarness/data"))?;
    let revision = crate::learnharness_export::ExportManifest::from_payload(&records)?.revision;
    ensure!(
        revision == transaction.merged_revision,
        "LearnHarness application readback differs from the prepared revision"
    );
    transaction.new_generation = read_generation(vault)?;
    transaction.state = "application_readback_verified".into();
    archive_transaction(vault, &transaction)?;
    fs::remove_file(path)?;
    sync_dir(vault)?;
    Ok(
        serde_json::json!({"confirmed":true,"revision":transaction.incoming_revision,"merged_revision":transaction.merged_revision}),
    )
}

pub fn confirm_pending_readback(vault: &Path) -> Result<serde_json::Value> {
    let path = vault.join("apply.pending.json");
    if !path.try_exists()? {
        return Ok(serde_json::json!({"confirmed":false}));
    }
    let transaction = read_transaction(vault)?;
    confirm_readback(vault, &transaction.incoming_revision)
}

pub fn recover_transaction(vault: &Path) -> Result<serde_json::Value> {
    let mut outcome = "no_pending_apply";
    let pending = vault.join("apply.pending.json");
    if pending.try_exists()? {
        let mut transaction = read_transaction(vault)?;
        validate_generation_name(&transaction.old_generation)?;
        validate_generation_name(&transaction.new_generation)?;
        ensure!(
            subvolume_readonly(&vault.join(&transaction.old_generation))?,
            "Prior generation backup is unavailable or mutable"
        );
        let current = read_generation(vault)?;
        if current == transaction.old_generation {
            outcome = "recovered_before_pointer_switch";
        } else if current == transaction.new_generation {
            set_current(vault, &transaction.old_generation)?;
            outcome = "rolled_back_unverified_apply";
        } else {
            validate_generation_name(&current)?;
            let current_records =
                Tree::records(&vault.join(&current).join("config/LearnHarness/data"))?;
            let current_revision =
                crate::learnharness_export::ExportManifest::from_payload(&current_records)?
                    .revision;
            if current_revision == transaction.merged_revision {
                transaction.state = "application_readback_verified_during_recovery".into();
                outcome = "verified_clean_application_exit";
            } else {
                set_current(vault, &transaction.old_generation)?;
                outcome = "rolled_back_unverified_apply";
            }
        }
        transaction.state = outcome.into();
        archive_transaction(vault, &transaction)?;
        fs::remove_file(pending)?;
        sync_dir(vault)?;
    }
    if vault.join("active").try_exists()? {
        fs::rename(
            vault.join("active"),
            vault.join(format!("interrupted.{}", uuid()?)),
        )?;
        sync_dir(vault)?;
    }
    Ok(serde_json::json!({"recovered":true,"outcome":outcome}))
}

fn read_transaction(vault: &Path) -> Result<ApplyTransaction> {
    let transaction: ApplyTransaction = serde_json::from_slice(&crate::storage::read(
        &vault.join("apply.pending.json"),
        4096,
    )?)?;
    ensure!(
        transaction.version == 1 && transaction.state == "awaiting_application_readback",
        "Unsupported managed apply journal"
    );
    ensure!(
        harness_sync_core::valid_id(&transaction.incoming_revision)
            && harness_sync_core::valid_id(&transaction.merged_revision),
        "Invalid managed apply journal revision"
    );
    ensure!(
        transaction.backup.starts_with("backup-") && !transaction.backup.contains('/'),
        "Invalid managed backup path"
    );
    Ok(transaction)
}

fn archive_transaction(vault: &Path, transaction: &ApplyTransaction) -> Result<()> {
    let directory = vault.join("transactions");
    private_dir(&directory)?;
    let path = directory.join(format!(
        "apply-{}-{}.json",
        transaction.incoming_revision,
        uuid()?
    ));
    crate::storage::atomic(&path, &serde_json::to_vec(transaction)?)?;
    sync_dir(&directory)
}

fn read_generation(vault: &Path) -> Result<String> {
    let name = String::from_utf8(crate::storage::read(&vault.join("current"), 128)?)?;
    let name = name.trim_end_matches('\n').to_owned();
    validate_generation_name(&name)?;
    ensure!(
        vault.join(&name).is_dir(),
        "Current managed generation is missing"
    );
    Ok(name)
}

fn validate_generation_name(name: &str) -> Result<()> {
    ensure!(
        name.starts_with("session.")
            && name.len() == 44
            && name[8..]
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "Invalid managed generation name"
    );
    Ok(())
}

fn set_current(vault: &Path, generation: &str) -> Result<()> {
    validate_generation_name(generation)?;
    crate::storage::atomic(
        &vault.join("current.next"),
        format!("{generation}\n").as_bytes(),
    )?;
    fs::rename(vault.join("current.next"), vault.join("current"))?;
    sync_dir(vault)
}

fn uuid() -> Result<String> {
    let id = fs::read_to_string("/proc/sys/kernel/random/uuid")?
        .trim()
        .to_owned();
    ensure!(
        id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "Kernel UUID source returned invalid data"
    );
    Ok(id)
}

fn btrfs_snapshot(source: &Path, target: &Path, readonly: bool) -> Result<()> {
    let source = source.to_str().context("Invalid Btrfs source path")?;
    let target = target.to_str().context("Invalid Btrfs destination path")?;
    let mut command = Command::new("btrfs");
    command.arg("subvolume").arg("snapshot");
    if readonly {
        command.arg("-r");
    }
    let output = command
        .arg(source)
        .arg(target)
        .output()
        .context("Could not run Btrfs snapshot utility")?;
    ensure!(output.status.success(), "Btrfs snapshot operation failed");
    Ok(())
}

fn btrfs_set_readonly(path: &Path) -> Result<()> {
    let output = Command::new("btrfs")
        .args([
            "property",
            "set",
            "-ts",
            path.to_str().context("Invalid Btrfs generation path")?,
            "ro",
            "true",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "Could not make managed generation immutable"
    );
    Ok(())
}

fn subvolume_readonly(path: &Path) -> Result<bool> {
    let output = Command::new("btrfs")
        .args([
            "property",
            "get",
            "-ts",
            path.to_str().context("Invalid Btrfs generation path")?,
            "ro",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "Could not verify Btrfs generation properties"
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "ro=true")
}

fn exchange_directories(first: &Path, second: &Path) -> Result<()> {
    let first = CString::new(first.as_os_str().as_bytes())?;
    let second = CString::new(second.as_os_str().as_bytes())?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            first.as_ptr(),
            libc::AT_FDCWD,
            second.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    ensure!(
        result == 0,
        "Atomic managed data exchange failed: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
