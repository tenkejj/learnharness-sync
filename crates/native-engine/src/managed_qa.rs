//! Closed-profile transaction used only by the subordinate-UID Linux qualification runner.
//! The caller owns the vault lock and has reaped the application PID namespace.
use crate::managed_profile::Tree;
use crate::storage::Store;
use crate::storage::{atomic, nonce, read, sync_dir};
use anyhow::{bail, ensure, Context, Result};
use harness_sync_core::{learnharness_061, sha256, Payload};
use serde::{Deserialize, Serialize};
use std::{
    ffi::CString,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    old: String,
    new: String,
}

fn exchange(a: &Path, b: &Path, interrupt: bool) -> Result<()> {
    let x = CString::new(a.as_os_str().as_bytes())?;
    let y = CString::new(b.as_os_str().as_bytes())?;
    // Both NUL-terminated names remain alive for this synchronous syscall.
    if unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            x.as_ptr(),
            libc::AT_FDCWD,
            y.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    if interrupt {
        // QA fault at the namespace switch, before either directory fsync.
        unsafe { libc::raise(libc::SIGKILL) };
        bail!("Fault injection failed");
    }
    sync_dir(a.parent().context("Missing parent")?)?;
    sync_dir(b.parent().context("Missing parent")?)
}

fn tree_records(root: &Path) -> Result<Payload> {
    Tree::records(&root.join("config/LearnHarness/data"))
}

fn peer_profile(store: &Store) -> Result<PathBuf> {
    let name = store
        .root
        .file_name()
        .and_then(|p| p.to_str())
        .context("Expected managed peer")?;
    let label = name
        .strip_prefix("engine-")
        .context("Expected managed peer")?;
    ensure!(matches!(label, "A" | "B" | "C" | "D"), "Unknown peer");
    let vault = Path::new("/lab");
    ensure!(
        store.root.parent() == Some(vault) && read(&vault.join("active"), 64)? == b"transfer\n",
        "Managed transfer required"
    );
    let root = vault.join(label);
    ensure!(
        !root.join("pending-apply").try_exists()? && !root.join("pending-receipt").try_exists()?,
        "Profile needs recovery or application readback"
    );
    Ok(root)
}

pub fn capture(store: &Store, device: &str) -> Result<String> {
    let root = peer_profile(store)?;
    let payload = tree_records(&root)?;
    learnharness_061::validate(&payload).map_err(anyhow::Error::msg)?;
    store.writer_safe()?;
    let mut ledger = store.ledger()?;
    ensure!(ledger.initialized, "Peer is uninitialized");
    if let Some(base) = &ledger.managed_base {
        if payload == store.payload(&store.manifest(base)?)? {
            return Ok(base.clone());
        }
    }
    // Receiving a head advances transport state before an offline application
    // has adopted it. Parent the edit to its last confirmed checkout.
    let manifest = harness_sync_core::Manifest::from_payload(
        device.into(),
        ledger.managed_base.iter().cloned().collect(),
        &payload,
    )
    .map_err(anyhow::Error::msg)?;
    for bytes in payload.values() {
        store.put_object(&sha256(bytes), bytes)?;
    }
    let head = store.commit_manifest(&manifest)?;
    if let Some(previous) = &ledger.head {
        if previous != &head {
            store.backup(previous)?;
            ledger.backups.insert(previous.clone());
            if ledger.managed_base.as_ref() != Some(previous) {
                ledger.conflicts.insert(previous.clone());
            }
        }
    }
    ledger.head = Some(head.clone());
    ledger.managed_base = Some(head.clone());
    store.save(&ledger)?;
    Ok(head)
}

pub fn stage_received(store: &Store, expected: &str) -> Result<()> {
    let root = peer_profile(store)?;
    let ledger = store.ledger()?;
    ensure!(
        ledger.head.as_deref() == Some(expected) && ledger.conflicts.is_empty(),
        "Incoming state is divergent or stale"
    );
    let mut current = Tree::read(&root.join("config/LearnHarness/data"))?;
    let settings = current.files.remove("settings.json");
    let previous = match &ledger.managed_base {
        Some(base) => {
            ensure!(
                store.history(expected)?.contains_key(base),
                "Received state does not descend from the application checkout"
            );
            store.payload(&store.manifest(base)?)?
        }
        None => Payload::new(),
    };
    ensure!(
        current.files == previous,
        "Closed profile contains unpublished edits"
    );
    let current = Tree::from_records(store.payload(&store.manifest(expected)?)?, settings)?;
    let stage = root.join("received");
    current.copy_to(&stage)?;
    atomic(&root.join("pending-receipt"), expected.as_bytes())
}

pub fn run(action: &str, label: &str) -> Result<()> {
    ensure!(
        matches!(label, "A" | "B" | "C" | "D" | "E"),
        "Unknown QA profile"
    );
    let vault = Path::new("/lab");
    let operation = read(&vault.join("active"), 64)?;
    ensure!(
        operation == b"transfer\n"
            || (action == "migrate" && operation == b"migrate\n")
            || (action == "confirm" && operation == b"desktop\n"),
        "Closed managed transfer required"
    );
    let root = vault.join(label);
    let data = root.join("config/LearnHarness/data");
    let stage = root.join("received");
    let pending = root.join("pending-apply");
    let backup = pending.join("backup");
    let journal = pending.join("intent.json");
    let current = Tree::read(&data)?;
    if action == "confirm" {
        let receipt = root.join("pending-receipt");
        if !receipt.try_exists()? {
            return Ok(());
        }
        let id = String::from_utf8(read(&receipt, 64)?)?;
        let store = Store::open(vault.join(format!("engine-{label}")))?;
        ensure!(
            tree_records(&root)? == store.payload(&store.manifest(&id)?)?,
            "Application readback differs from verified snapshot"
        );
        let mut ledger = store.ledger()?;
        ledger.managed_base = Some(id);
        store.save(&ledger)?;
        if stage.try_exists()? {
            fs::rename(stage, root.join(format!("received-{}", nonce()?)))?;
        }
        fs::remove_file(receipt)?;
        return sync_dir(&root);
    }
    if action == "migrate" {
        ensure!(
            current.files.keys().all(|p| p == "settings.json"),
            "Migration requires an empty destination"
        );
        let mut imported = Tree::read(Path::new("/source"))?;
        imported.files.remove("settings.json");
        ensure!(!imported.files.is_empty(), "No sessions to migrate");
        if let Some(local) = current.files.get("settings.json") {
            imported.files.insert("settings.json".into(), local.clone());
        }
        imported.copy_to(&stage)?;
    }
    match action {
        "prepare" | "migrate" => {
            let incoming = Tree::read(&stage)?;
            ensure!(
                current.files.get("settings.json") == incoming.files.get("settings.json"),
                "Local settings changed"
            );
            fs::create_dir(&pending).context("Pending transaction requires inspection")?;
            current.copy_to(&backup)?;
            // Write a fresh durable stage from the verified input before publishing intent.
            incoming.copy_to(&pending.join("next"))?;
            atomic(
                &journal,
                &serde_json::to_vec(&Intent {
                    old: current.id()?,
                    new: incoming.id()?,
                })?,
            )?;
            sync_dir(&root)?;
            if action == "migrate" {
                let next = pending.join("next");
                exchange(&next, &data, false)?;
                ensure!(Tree::read(&data)? == incoming, "Migration readback failed");
                fs::rename(&pending, root.join(format!("transaction-{}", nonce()?)))?;
                sync_dir(&root)?;
            }
        }
        "activate" | "interrupt" => {
            let intent: Intent = serde_json::from_slice(&read(&journal, 1024)?)?;
            ensure!(
                current.id()? == intent.old && Tree::read(&backup)?.id()? == intent.old,
                "Previous state or backup changed"
            );
            let next = pending.join("next");
            ensure!(
                Tree::read(&next)?.id()? == intent.new,
                "Damaged staging tree"
            );
            exchange(&next, &data, action == "interrupt")?;
            ensure!(
                Tree::read(&data)?.id()? == intent.new,
                "Activation readback failed"
            );
        }
        "recover" if !journal.try_exists()? => {
            // No activation can precede a durable intent. Preserve incomplete staging.
            if pending.try_exists()? {
                fs::rename(&pending, root.join(format!("incomplete-{}", nonce()?)))?;
                sync_dir(&root)?;
            }
        }
        "recover" | "finish" => {
            let intent: Intent = serde_json::from_slice(&read(&journal, 1024)?)?;
            let saved = Tree::read(&backup)?;
            ensure!(
                saved.id()? == intent.old,
                "Independent backup failed verification"
            );
            let current_id = current.id()?;
            if action == "recover" {
                if current_id != intent.old {
                    ensure!(
                        current_id == intent.new,
                        "Unrecognized active state; manual review required"
                    );
                    let restore = pending.join(format!("restore-{}", nonce()?));
                    saved.copy_to(&restore)?;
                    exchange(&restore, &data, false)?;
                }
                ensure!(Tree::read(&data)? == saved, "Restoration readback failed");
            } else {
                ensure!(
                    current_id == intent.new,
                    "Received state failed verification"
                );
            }
            let archive: PathBuf = root.join(format!("transaction-{}", nonce()?));
            fs::rename(&pending, archive)?;
            sync_dir(&root)?;
        }
        _ => bail!("Unknown QA transaction operation"),
    }
    Ok(())
}
