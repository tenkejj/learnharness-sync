//! Fixed launcher plus capability-passed, one-shot managed snapshot bridge.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

struct StagedHelper(PathBuf);

impl Drop for StagedHelper {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stage_helper(source: PathBuf) -> Result<StagedHelper, String> {
    let directory = PathBuf::from("/tmp").join(format!(
        "learnharness-sync-helper-{}",
        harness_sync_native::storage::nonce().map_err(|_| "Could not allocate managed helper")?
    ));
    fs::create_dir(&directory).map_err(|_| "Could not create private managed helper staging")?;
    let staging = StagedHelper(directory);
    fs::set_permissions(&staging.0, fs::Permissions::from_mode(0o755))
        .map_err(|_| "Could not set managed helper staging permissions")?;
    let executable = staging.0.join("helper");
    fs::copy(source, &executable).map_err(|_| "Could not stage managed helper executable")?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
        .map_err(|_| "Could not set managed helper executable permissions")?;
    Ok(staging)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn snapshot() -> Result<serde_json::Value, String> {
    let value = request(harness_sync_native::managed_bridge::Operation::Snapshot)?;
    if value.get("format").and_then(serde_json::Value::as_str)
        != Some("learnharness-0.6.1-disposable-v1")
    {
        return Err("Managed helper returned an unsupported snapshot profile".into());
    }
    if !value
        .get("digest")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Managed helper returned invalid snapshot integrity metadata".into());
    }
    Ok(value)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn request(
    operation: harness_sync_native::managed_bridge::Operation,
) -> Result<serde_json::Value, String> {
    let source = std::env::current_exe().map_err(|_| "LearnHarness Sync executable unavailable")?;
    let helper = stage_helper(source)?;
    let mut cmd = Command::new("/usr/bin/bash");
    cmd.env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C.UTF-8");
    for key in ["HOME", "XDG_DATA_HOME"] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    let spawn = cmd
        .args([
            "-c",
            include_str!("../managed/host.sh"),
            "launcher",
            "bridge",
            include_str!("../managed/worker.sh"),
            helper
                .0
                .join("helper")
                .to_str()
                .ok_or("Invalid LearnHarness Sync executable path")?,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(_) => return Err("Could not start isolated managed helper".into()),
    };
    let reader = child
        .stdout
        .take()
        .ok_or("Managed helper output unavailable")?;
    let writer = child
        .stdin
        .take()
        .ok_or("Managed helper input unavailable")?;
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut client = harness_sync_native::managed_bridge::StdioClient::new(reader, writer);
        let _ = send.send(client.request(operation));
    });
    let result = match receive.recv_timeout(Duration::from_secs(25)) {
        Ok(result) => {
            result.map_err(|_| "Managed snapshot helper disconnected or timed out".to_string())
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Managed snapshot helper timed out; recovery is required".into());
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                break child.wait().ok();
            }
            Err(_) => {
                let _ = child.kill();
                break child.wait().ok();
            }
        }
    };
    let output = child
        .wait_with_output()
        .map_err(|_| "Managed helper process failed")?;
    if !status.is_some_and(|status| status.success()) || !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let tail = detail
            .lines()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("; ");
        return Err(if tail.is_empty() {
            "Managed helper exited abnormally; recovery is required".into()
        } else {
            format!("Managed helper exited abnormally; recovery is required: {tail}")
        });
    }
    if !result.as_ref().is_ok_and(|value| value.is_object()) {
        return Err(result
            .err()
            .unwrap_or_else(|| "Managed helper returned malformed snapshot metadata".into()));
    }
    result
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn snapshot() -> Result<serde_json::Value, String> {
    Err("Managed snapshots require Linux x86_64".into())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn request(
    _operation: harness_sync_native::managed_bridge::Operation,
) -> Result<serde_json::Value, String> {
    Err("Managed snapshots require Linux x86_64".into())
}

pub fn run(operation: &str) -> Result<String, String> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err("Managed sessions require Linux x86_64.".into());
    }
    if !matches!(operation, "status" | "launch" | "recover") {
        return Err("Unsupported managed operation.".into());
    }
    let mut cmd = Command::new("/usr/bin/bash");
    cmd.env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C.UTF-8");
    for key in [
        "HOME",
        "XDG_DATA_HOME",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XAUTHORITY",
        "LEARNHARNESS_SYNC_MANAGED_QA_LOG",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    let output = cmd
        .args([
            "-c",
            include_str!("../managed/host.sh"),
            "launcher",
            operation,
            include_str!("../managed/worker.sh"),
        ])
        .output()
        .map_err(|_| "Linux isolation tools are unavailable.")?;
    if !output.status.success() {
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        if std::env::var_os("LEARNHARNESS_SYNC_MANAGED_QA_LOG").is_some() {
            return Err(diagnostic
                .chars()
                .rev()
                .take(4000)
                .collect::<String>()
                .chars()
                .rev()
                .collect());
        }
        return Err(diagnostic
            .lines()
            .last()
            .unwrap_or(
                "Managed storage operation failed. Check Btrfs and subordinate UID/GID mappings.",
            )
            .chars()
            .take(300)
            .collect());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let state = stdout.lines().last().unwrap_or_default().trim();
    match state {
        s @ ("new" | "active" | "recovery" | "saved" | "recovered") => Ok(s.to_owned()),
        _ => Err("The managed supervisor returned an unknown state.".into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_operations_outside_the_fixed_launcher_contract() {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert_eq!(
            super::run("import").unwrap_err(),
            "Unsupported managed operation."
        );
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        assert_eq!(
            super::run("import").unwrap_err(),
            "Managed sessions require Linux x86_64."
        );
    }
}
