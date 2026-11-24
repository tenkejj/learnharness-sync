use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use harness_sync_core::{
    reconcile, sha256, valid_id, Decision, Manifest, Payload, Revision, MAX_FILE, MAX_HISTORY,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_METADATA: usize = 512 * 1024;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub version: u32,
    pub initialized: bool,
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_base: Option<String>,
    pub conflicts: BTreeSet<String>,
    pub backups: BTreeSet<String>,
    pub events: Vec<Event>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub time: u64,
    pub kind: String,
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn nonce() -> Result<String> {
    let mut b = [0u8; 32];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| anyhow::anyhow!("Random source unavailable"))?;
    Ok(sha256(&b))
}
pub fn safe_path(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Engine path must be absolute");
    for p in path.ancestors() {
        if let Ok(m) = fs::symlink_metadata(p) {
            ensure!(!m.file_type().is_symlink(), "Symlink ancestry refused");
        }
    }
    Ok(())
}
pub fn private_dir(path: &Path) -> Result<()> {
    safe_path(path)?;
    if !path.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(path)?;
    }
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "Unsafe engine directory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0,
            "Engine directory must be private (0700) and owned by this user"
        );
    }
    Ok(())
}
fn options(write: bool) -> OpenOptions {
    let mut o = OpenOptions::new();
    if write {
        o.write(true).create_new(true);
    } else {
        o.read(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    o
}
pub fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    safe_path(path)?;
    let f = options(false)
        .open(path)
        .context("Required local object unavailable")?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.len() <= limit as u64,
        "Unsafe file type or size"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(m.nlink() == 1, "Hard-linked file refused");
    }
    let mut b = Vec::new();
    f.take(limit as u64 + 1).read_to_end(&mut b)?;
    ensure!(b.len() <= limit, "File exceeds limit");
    Ok(b)
}
pub fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    safe_path(path)?;
    let parent = path.parent().context("Missing parent")?;
    let tmp = parent.join(format!(".pending-{}", nonce()?));
    let result = (|| {
        let mut f = options(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        sync_dir(parent)
    })();
    if tmp.exists() {
        let _ = fs::remove_file(tmp);
    }
    result
}
pub struct Store {
    pub root: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        private_dir(&root)?;
        let lock_path = root.join("engine.lock");
        let mut o = OpenOptions::new();
        o.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let lock = o.open(lock_path)?;
        lock.try_lock_exclusive()
            .context("Another LearnHarness Sync instance owns this profile")?;
        for d in ["objects", "manifests", "staging", "backups"] {
            private_dir(&root.join(d))?;
        }
        let s = Self { root, _lock: lock };
        // Only unpublished staging belongs to recovery cleanup; committed history is never collected.
        for e in fs::read_dir(s.root.join("staging"))? {
            let p = e?.path();
            if fs::symlink_metadata(&p)?.is_file() {
                fs::remove_file(p)?;
            } else {
                bail!("Unexpected staging entry; inspect diagnostics");
            }
        }
        if !s.root.join("ledger.json").exists() {
            s.save(&Ledger {
                version: 2,
                ..Default::default()
            })?;
        }
        s.ledger()?;
        Ok(s)
    }
    pub fn ledger(&self) -> Result<Ledger> {
        let l: Ledger =
            serde_json::from_slice(&read(&self.root.join("ledger.json"), MAX_METADATA)?)?;
        ensure!(
            l.version == 2
                && l.conflicts.len() <= MAX_HISTORY
                && l.backups.len() <= MAX_HISTORY
                && l.head
                    .iter()
                    .chain(l.conflicts.iter())
                    .chain(l.managed_base.iter())
                    .chain(l.backups.iter())
                    .all(|id| valid_id(id)),
            "Unsupported or invalid local ledger"
        );
        Ok(l)
    }
    pub fn save(&self, l: &Ledger) -> Result<()> {
        let b = serde_json::to_vec(l)?;
        ensure!(
            b.len() <= MAX_METADATA,
            "Ledger exceeds limit; history retained"
        );
        atomic(&self.root.join("ledger.json"), &b)
    }
    pub fn event(l: &mut Ledger, kind: &str) {
        if l.events.len() >= 128 {
            l.events.remove(0);
        }
        l.events.push(Event {
            time: now(),
            kind: kind.into(),
        });
    }
    fn object_path(&self, kind: &str, id: &str) -> Result<PathBuf> {
        ensure!(valid_id(id), "Invalid object ID");
        Ok(self.root.join(kind).join(id))
    }
    pub fn object(&self, id: &str) -> Result<Vec<u8>> {
        let b = read(&self.object_path("objects", id)?, MAX_FILE)?;
        ensure!(sha256(&b) == id, "Local object integrity failure");
        Ok(b)
    }
    pub fn has_object(&self, id: &str) -> bool {
        self.object(id).is_ok()
    }
    fn put(&self, kind: &str, id: &str, bytes: &[u8]) -> Result<()> {
        ensure!(sha256(bytes) == id, "Object hash mismatch");
        let dest = self.object_path(kind, id)?;
        if dest.exists() {
            ensure!(
                read(&dest, MAX_FILE)? == bytes,
                "Existing object is corrupt; repair required"
            );
            return Ok(());
        }
        self.reserve(bytes.len() as u64, 1)?;
        if kind == "manifests" {
            ensure!(
                fs::read_dir(self.root.join(kind))?.count() < MAX_HISTORY,
                "Manifest limit reached; history retained"
            );
        }
        let tmp = self.root.join("staging").join(nonce()?);
        let result = (|| {
            let mut f = options(true).open(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            fs::rename(&tmp, &dest)?;
            sync_dir(dest.parent().unwrap())
        })();
        if tmp.exists() {
            let _ = fs::remove_file(tmp);
        }
        result
    }
    fn reserve(&self, additional_bytes: u64, additional_files: usize) -> Result<()> {
        let mut total = 0u64;
        let mut files = 0usize;
        let mut pending = vec![
            self.root.join("objects"),
            self.root.join("manifests"),
            self.root.join("backups"),
        ];
        while let Some(dir) = pending.pop() {
            safe_path(&dir)?;
            for e in fs::read_dir(dir)? {
                let entry = e?;
                let m = fs::symlink_metadata(entry.path())?;
                ensure!(!m.file_type().is_symlink(), "Symlink in immutable storage");
                if m.is_dir() {
                    pending.push(entry.path());
                } else {
                    ensure!(m.is_file(), "Unexpected storage entry");
                    total = total
                        .checked_add(m.len())
                        .context("Storage size overflow")?;
                    files += 1;
                }
                ensure!(
                    total.saturating_add(additional_bytes) <= 1024 * 1024 * 1024
                        && files.saturating_add(additional_files) <= 100_000
                        && pending.len() <= MAX_HISTORY,
                    "Local 1 GiB storage or file-count quota reached; all history retained"
                );
            }
        }
        ensure!(
            total.saturating_add(additional_bytes) <= 1024 * 1024 * 1024
                && files.saturating_add(additional_files) <= 100_000,
            "Storage quota reached"
        );
        Ok(())
    }
    pub fn put_object(&self, id: &str, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() <= MAX_FILE, "Object exceeds limit");
        self.put("objects", id, bytes)
    }
    pub fn manifest(&self, id: &str) -> Result<Manifest> {
        let b = read(&self.object_path("manifests", id)?, MAX_METADATA)?;
        let m: Manifest = serde_json::from_slice(&b)?;
        m.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            m.id() == id && m.bytes() == b,
            "Noncanonical or corrupt manifest"
        );
        Ok(m)
    }
    pub fn payload(&self, m: &Manifest) -> Result<Payload> {
        m.validate().map_err(anyhow::Error::msg)?;
        let mut p = Payload::new();
        for f in &m.files {
            let b = self.object(&f.hash)?;
            ensure!(b.len() as u64 == f.size, "Object length mismatch");
            p.insert(f.path.clone(), b);
        }
        harness_sync_core::validate_format(&m.format, &p).map_err(anyhow::Error::msg)?;
        Ok(p)
    }
    pub fn commit_manifest(&self, m: &Manifest) -> Result<String> {
        self.payload(m)?;
        for p in &m.parents {
            self.history(p)?;
        }
        let id = m.id();
        ensure!(!m.parents.contains(&id), "Self parent");
        self.put("manifests", &id, &m.bytes())?;
        Ok(id)
    }
    pub fn history(&self, head: &str) -> Result<BTreeMap<String, Revision>> {
        let mut h = BTreeMap::new();
        let mut pending = vec![head.to_string()];
        while let Some(id) = pending.pop() {
            if h.contains_key(&id) {
                continue;
            }
            ensure!(h.len() < MAX_HISTORY, "History exceeds limit");
            let m = self.manifest(&id)?;
            self.payload(&m)?;
            pending.extend(m.parents.clone());
            h.insert(
                id.clone(),
                Revision {
                    snapshot_id: id,
                    parent_ids: m.parents,
                },
            );
        }
        harness_sync_core::is_ancestor(head, head, &h).map_err(anyhow::Error::msg)?;
        Ok(h)
    }
    pub fn publish(&self, device: &str, payload: &Payload) -> Result<String> {
        self.writer_safe()?;
        let mut l = self.ledger()?;
        ensure!(l.initialized, "Create an isolated workspace first");
        if let Some(ref head) = l.head {
            if self.payload(&self.manifest(head)?)? == *payload {
                return Ok(head.clone());
            }
        }
        let m = Manifest::from_payload(device.into(), l.head.iter().cloned().collect(), payload)
            .map_err(anyhow::Error::msg)?;
        for (id, b) in payload.values().map(|b| (sha256(b), b)) {
            self.put_object(&id, b)?;
        }
        let id = self.commit_manifest(&m)?;
        l.head = Some(id.clone());
        Self::event(&mut l, "Published isolated synthetic revision");
        self.save(&l)?;
        Ok(id)
    }
    pub fn writer_safe(&self) -> Result<()> {
        ensure!(!self.root.join("WRITER_ACTIVE").try_exists()?,"Isolated writer active or quarantined; close the isolated writer and review its marker");
        Ok(())
    }
    pub fn initialize(&self) -> Result<()> {
        self.writer_safe()?;
        let mut l = self.ledger()?;
        l.initialized = true;
        self.save(&l)
    }
    pub(crate) fn backup(&self, id: &str) -> Result<()> {
        let dest = self.root.join("backups").join(id);
        if dest.try_exists()? {
            self.backup_payload(id)?;
            return Ok(());
        }
        let stage = self
            .root
            .join("backups")
            .join(format!(".pending-{}", nonce()?));
        let m = self.manifest(id)?;
        let unique: BTreeMap<_, _> = m.files.iter().map(|f| (&f.hash, f.size)).collect();
        self.reserve(
            unique.values().sum::<u64>() + m.bytes().len() as u64,
            unique.len() + 1,
        )?;
        private_dir(&stage)?;
        atomic(&stage.join("manifest.json"), &m.bytes())?;
        for f in &m.files {
            atomic(&stage.join(&f.hash), &self.object(&f.hash)?)?;
        }
        sync_dir(&stage)?;
        fs::rename(stage, &dest)?;
        sync_dir(dest.parent().unwrap())?;
        self.backup_payload(id)?;
        Ok(())
    }
    fn backup_payload(&self, id: &str) -> Result<(Manifest, Payload)> {
        let dir = self.object_path("backups", id)?;
        let bytes = read(&dir.join("manifest.json"), MAX_METADATA)?;
        ensure!(sha256(&bytes) == id, "Backup manifest corrupt");
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        manifest.validate().map_err(anyhow::Error::msg)?;
        ensure!(manifest.bytes() == bytes, "Noncanonical backup manifest");
        let mut payload = Payload::new();
        for entry in &manifest.files {
            let bytes = read(&dir.join(&entry.hash), MAX_FILE)?;
            ensure!(
                bytes.len() as u64 == entry.size && sha256(&bytes) == entry.hash,
                "Backup object corrupt"
            );
            payload.insert(entry.path.clone(), bytes);
        }
        harness_sync_core::validate_format(&manifest.format, &payload)
            .map_err(anyhow::Error::msg)?;
        Ok((manifest, payload))
    }
    pub fn reconcile(&self, remote: &str) -> Result<Decision> {
        self.writer_safe()?;
        let mut l = self.ledger()?;
        ensure!(
            l.initialized,
            "Received history retained; initialize isolated workspace to reconcile"
        );
        let mut h = self.history(remote)?;
        if let Some(ref id) = l.head {
            h.extend(self.history(id)?);
        }
        let d = reconcile(l.head.as_deref(), remote, &h).map_err(anyhow::Error::msg)?;
        match d {
            Decision::FastForward => {
                if let Some(ref head) = l.head {
                    self.backup(head)?;
                    l.backups.insert(head.clone());
                }
                l.head = Some(remote.into());
                Self::event(
                    &mut l,
                    "Verified synthetic fast-forward; previous state backed up",
                );
            }
            Decision::Conflict if l.conflicts.insert(remote.into()) => {
                Self::event(
                    &mut l,
                    "Divergent history preserved; automatic apply blocked",
                );
            }
            _ => (),
        }
        self.save(&l)?;
        Ok(d)
    }
    pub fn restore(&self, device: &str, id: &str) -> Result<String> {
        self.writer_safe()?;
        let mut l = self.ledger()?;
        ensure!(l.backups.contains(id), "Unknown backup");
        let (manifest, payload) = self.backup_payload(id)?;
        for bytes in payload.values() {
            self.put_object(&sha256(bytes), bytes)?;
        }
        self.commit_manifest(&manifest)?;
        if let Some(ref head) = l.head {
            self.backup(head)?;
            l.backups.insert(head.clone());
            self.save(&l)?;
        }
        self.publish(device, &payload)
    }
    pub fn list(&self) -> Result<Vec<(String, Manifest)>> {
        let mut result = Vec::new();
        for e in fs::read_dir(self.root.join("manifests"))? {
            ensure!(
                result.len() < MAX_HISTORY,
                "Manifest inventory exceeds limit"
            );
            let id = e?
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Invalid local object name"))?;
            let m = self.manifest(&id)?;
            self.payload(&m)?;
            result.push((id, m));
        }
        Ok(result)
    }
}
