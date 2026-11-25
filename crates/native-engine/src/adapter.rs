//! Installation detection and the fail-closed live profile mutation gate.
//! The separate backup inspector only accepts an immutable Btrfs source.
use anyhow::{bail, Result};
use harness_sync_core::Payload;
pub const PRODUCTION_BLOCK: &str = "The legacy direct-profile commands remain disabled; use the explicitly consented managed-profile workflow.";
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

/// Detect a reviewed build by exact bytes, then describe any unverified update.
/// Only the AppImage directory and our own private pinned-image cache are read.
pub fn detect_installation() -> std::result::Result<String, String> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let selected = crate::compatibility::detect_from_env().map_err(|e| e.to_string())?;
        let mut status = format!("LearnHarness {} verified", selected.build.version);
        if let Some(newer) = selected.newer_unverified {
            status.push_str(&format!(
                "; unverified LearnHarness {newer} also detected. The managed profile stays on the pinned build."
            ));
        }
        Ok(status)
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    Err("Detection requires Linux x86_64".into())
}
