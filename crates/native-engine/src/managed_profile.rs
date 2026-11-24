//! Strict 0.6.1 managed-profile snapshots and durable tree operations.
//!
//! This module has no `/lab`, peer labels, or fixture assumptions. Callers must
//! establish exclusive writer control before reading or replacing a profile.
use crate::storage::{atomic, private_dir, read, safe_path, sync_dir, MAX_METADATA};
use anyhow::{ensure, Context, Result};
use harness_sync_core::{learnharness_061, Payload, MAX_FILE, MAX_FILES, MAX_TOTAL};
use std::{collections::BTreeSet, fs, path::Path};

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
        Ok(serde_json::json!({
            "format": harness_sync_core::learnharness_061::FORMAT,
            "digest": tree.id()?,
            "files": records.len(),
            "bytes": bytes,
        }))
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
        fs::create_dir(target)?;
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
