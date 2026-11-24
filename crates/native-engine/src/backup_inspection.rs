//! Local read-only schema inspection. No engine store, peer, launcher or apply capability.
use anyhow::{ensure, Context, Result};
use harness_sync_core::{learnharness_061, Payload, MAX_FILES};
use rustix::fs::{openat2, Dir, FileType, Mode, OFlags, ResolveFlags, CWD};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::Read,
    os::unix::{fs::MetadataExt, io::AsRawFd},
    path::{Component, Path},
};

const LIMIT: usize = 16 * 1024 * 1024;
const DIR: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC);
const SAFE: ResolveFlags = ResolveFlags::BENEATH
    .union(ResolveFlags::NO_SYMLINKS)
    .union(ResolveFlags::NO_XDEV);

fn readonly_snapshot(file: &File) -> Result<()> {
    let mut flags = 0u64;
    // linux/btrfs.h: BTRFS_IOC_SUBVOL_GETFLAGS writes one u64 to the supplied address.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), libc::_IOR::<u64>(0x94, 25), &mut flags) };
    ensure!(
        file.metadata()?.ino() == 256 && result == 0 && flags & 2 != 0,
        "Choose the root of a read-only Btrfs backup. A writable profile cannot be inspected."
    );
    Ok(())
}

fn child(parent: &File, name: &str, flags: OFlags) -> Result<File> {
    let file = File::from(openat2(
        parent,
        name,
        flags | OFlags::CLOEXEC,
        Mode::empty(),
        SAFE,
    )?);
    ensure!(
        file.metadata()?.dev() == parent.metadata()?.dev(),
        "Nested subvolume refused"
    );
    Ok(file)
}

pub fn inspect(path: &Path) -> Result<Value> {
    ensure!(
        path.is_absolute() && !path.components().any(|c| c == Component::ParentDir),
        "Enter an absolute backup path without parent traversal"
    );
    let snapshot = File::from(
        openat2(CWD, path, DIR, Mode::empty(), ResolveFlags::NO_SYMLINKS)
            .context("Cannot open the backup root")?,
    );
    readonly_snapshot(&snapshot)?;
    let mut data = None;
    for layout in [
        "data",
        "LearnHarness/data",
        "config/LearnHarness/data",
        ".config/LearnHarness/data",
    ] {
        match openat2(&snapshot, layout, DIR, Mode::empty(), SAFE) {
            Ok(fd) => {
                let dir = File::from(fd);
                ensure!(
                    dir.metadata()?.dev() == snapshot.metadata()?.dev(),
                    "Nested data subvolume refused"
                );
                ensure!(
                    data.replace(dir).is_none(),
                    "Multiple LearnHarness data directories found"
                );
            }
            Err(rustix::io::Errno::NOENT) => (),
            Err(_) => anyhow::bail!("Unsafe or unreadable data directory"),
        }
    }
    let data = data.context("No supported LearnHarness data directory in this backup")?;
    let mut records = Payload::new();
    let mut total = 0;
    for entry in Dir::read_from(&data)? {
        let entry = entry?;
        let name = entry.file_name().to_str()?;
        match name {
            "." | ".." => continue,
            "settings.json" => {
                let metadata = child(&data, name, OFlags::PATH)?.metadata()?;
                ensure!(
                    metadata.is_file() && metadata.nlink() == 1,
                    "Unsafe local settings file"
                );
                continue;
            }
            "chats" | "graph" | "jobs" | "plugins" => (),
            _ => anyhow::bail!("Backup contains unsupported durable data"),
        }
        let dir = child(&data, name, DIR)?;
        for record in Dir::read_from(&dir)? {
            let record = record?;
            let filename = record.file_name().to_str()?;
            if matches!(filename, "." | "..") {
                continue;
            }
            let key = format!("{name}/{filename}");
            ensure!(
                record.file_type() == FileType::RegularFile
                    && learnharness_061::supported_path(&key),
                "Unsupported record, attachment or link"
            );
            ensure!(
                records.len() < MAX_FILES,
                "Backup inventory exceeds inspection limit"
            );
            let file = child(&dir, filename, OFlags::RDONLY | OFlags::NONBLOCK)?;
            let meta = file.metadata()?;
            let remaining = LIMIT - total;
            ensure!(
                meta.is_file() && meta.nlink() == 1 && meta.len() <= remaining as u64,
                "Invalid record or 16 MiB inspection limit exceeded"
            );
            let mut bytes = Vec::new();
            file.take(remaining as u64 + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 == meta.len(),
                "Record changed during inspection"
            );
            total += bytes.len();
            records.insert(key, bytes);
        }
    }
    learnharness_061::validate(&records).map_err(anyhow::Error::msg)?;
    readonly_snapshot(&snapshot)?;
    let rows = |name: &str| {
        records
            .get(name)
            .map_or(0, |bytes| bytes.iter().filter(|&&b| b == b'\n').count())
    };
    Ok(json!({
        "sessions": records.keys().filter(|p| p.starts_with("chats/")).count(),
        "propositions": rows("graph/nodes.jsonl"),
        "chains": rows("graph/chains.jsonl")
    }))
}
