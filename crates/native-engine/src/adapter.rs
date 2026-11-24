//! Installation detection and the fail-closed live profile mutation gate.
//! The separate backup inspector only accepts an immutable Btrfs source.
use anyhow::{bail, Result};
use harness_sync_core::Payload;
pub const PRODUCTION_BLOCK: &str = "The legacy direct-profile commands remain disabled; use the explicitly consented LearnHarness 0.6.1 managed-profile workflow.";
pub fn production_operation() -> Result<()> {
    bail!(PRODUCTION_BLOCK)
}
pub fn fixture(text: &str) -> Result<Payload> {
    anyhow::ensure!(text.len() <= 64 * 1024, "Synthetic note exceeds 64 KiB");
    Ok(std::collections::BTreeMap::from([
        ("settings.json".into(), br#"{"version":1}"#.to_vec()),
        (
            "chats/example.json".into(),
            serde_json::to_vec(
                &serde_json::json!({"version":1,"id":"example","title":"Invented test note","text":text}),
            )?,
        ),
        ("graph/format.json".into(), br#"{"version":1}"#.to_vec()),
    ]))
}

/// Inspect only the installed executable. A match describes the bytes inspected;
/// any future launcher must pin and verify its own private copy before execution.
pub fn detect_installation() -> std::result::Result<(), &'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let Some(home) = std::env::var_os("HOME") else {
            return Err("Installation location unavailable");
        };
        let candidate =
            std::path::PathBuf::from(home).join("Applications/LearnHarness-0.6.1-x86_64.AppImage");
        match pinned_image(&candidate) {
            Ok(true) => Ok(()),
            Ok(false) => Err("LearnHarness AppImage differs from the tested 0.6.1 release"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err("LearnHarness 0.6.1 AppImage not found in ~/Applications")
            }
            Err(_) => Err("LearnHarness AppImage could not be verified"),
        }
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    Err("Detection requires Linux x86_64")
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn pinned_image(path: &std::path::Path) -> std::io::Result<bool> {
    use std::{
        io::Read,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    const SIZE: u64 = 196_784_591;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() != SIZE || meta.permissions().mode() & 0o111 == 0 {
        return Ok(false);
    }
    let mut input = file.take(SIZE + 1);
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0; 32768];
    let mut count = 0;
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        digest.update(&buffer[..n]);
    }
    Ok(count == SIZE
        && digest.finish().as_ref()
            == [
                0xb8, 0xa0, 0x33, 0xeb, 0x74, 0x21, 0x67, 0x78, 0x93, 0xd9, 0x6e, 0x49, 0x88, 0xb3,
                0x01, 0x53, 0xfd, 0xfe, 0x4f, 0xc6, 0xf0, 0xdc, 0x45, 0x6f, 0x79, 0x47, 0x16, 0x1b,
                0xba, 0x04, 0x1b, 0x98,
            ])
}
