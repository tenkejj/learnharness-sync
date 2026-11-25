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
fn configure_verified_image(cmd: &mut Command) -> Result<(), String> {
    let selected =
        harness_sync_native::compatibility::detect_from_env().map_err(|e| e.to_string())?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?);
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let pinned = harness_sync_native::compatibility::retain_verified_image(&selected, &data)
        .map_err(|e| format!("Could not retain the verified LearnHarness executable: {e}"))?;
    cmd.env("LEARNHARNESS_SYNC_VERIFIED_IMAGE", pinned)
        .env("LEARNHARNESS_SYNC_VERIFIED_SHA", &selected.build.sha256)
        .env(
            "LEARNHARNESS_SYNC_VERIFIED_SIZE",
            selected.build.size.to_string(),
        )
        .env(
            "LEARNHARNESS_SYNC_VERIFIED_RELEASE",
            &selected.build.release_key,
        );
    Ok(())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn configure_verified_image(_cmd: &mut Command) -> Result<(), String> {
    Err("Managed LearnHarness requires Linux x86_64".into())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn snapshot() -> Result<serde_json::Value, String> {
    let value = request(harness_sync_native::managed_bridge::Operation::Snapshot)?;
    validate_snapshot_metadata(&value)?;
    Ok(value)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn capture_for_export(
    consent: bool,
) -> Result<
    (
        harness_sync_native::learnharness_export::ExportManifest,
        std::collections::BTreeMap<String, Vec<u8>>,
    ),
    String,
> {
    if !consent {
        return Err("Confirm before sending real conversation data to the paired device.".into());
    }
    use base64::Engine as _;
    let result = request(harness_sync_native::managed_bridge::Operation::Capture { consent })?;
    let object = result
        .as_object()
        .ok_or("Managed helper returned an invalid export bundle")?;
    if object.len() != 2 {
        return Err("Managed helper returned an unsupported export bundle".into());
    }
    let manifest: harness_sync_native::learnharness_export::ExportManifest =
        serde_json::from_value(
            object
                .get("manifest")
                .cloned()
                .ok_or("Export manifest missing")?,
        )
        .map_err(|_| "Managed helper returned an invalid export manifest")?;
    manifest.validate().map_err(|e| e.to_string())?;
    let rows = object
        .get("records")
        .and_then(serde_json::Value::as_array)
        .ok_or("Export records missing")?;
    if rows.len() != manifest.files.len() {
        return Err("Export inventory is incomplete".into());
    }
    let mut payload = std::collections::BTreeMap::new();
    for row in rows {
        let row = row.as_object().ok_or("Invalid export record")?;
        if row.len() != 2 {
            return Err("Unsupported export record fields".into());
        }
        let path = row
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("Invalid export record path")?
            .to_owned();
        let data = row
            .get("data")
            .and_then(serde_json::Value::as_str)
            .ok_or("Invalid export record data")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| "Invalid encoded export record")?;
        if payload.insert(path, bytes).is_some() {
            return Err("Duplicate export record path".into());
        }
    }
    harness_sync_native::learnharness_export::verify(&manifest, &payload)
        .map_err(|e| e.to_string())?;
    Ok((manifest, payload))
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn capture_for_export(
    _consent: bool,
) -> Result<
    (
        harness_sync_native::learnharness_export::ExportManifest,
        std::collections::BTreeMap<String, Vec<u8>>,
    ),
    String,
> {
    Err("Managed LearnHarness export requires Linux x86_64".into())
}

fn validate_snapshot_metadata(value: &serde_json::Value) -> Result<(), String> {
    if !matches!(
        value.get("format").and_then(serde_json::Value::as_str),
        Some("learnharness-0.6.1-disposable-v1" | "learnharness-0.7.0-disposable-v1")
    ) {
        return Err("Managed helper returned an unsupported snapshot profile".into());
    }
    let valid_hash = |hash: &str| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if !value
        .get("digest")
        .and_then(serde_json::Value::as_str)
        .is_some_and(valid_hash)
    {
        return Err("Managed helper returned invalid snapshot integrity metadata".into());
    }
    let files = value
        .get("files")
        .and_then(serde_json::Value::as_u64)
        .ok_or("Managed helper returned an invalid file count")?;
    let bytes = value
        .get("bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or("Managed helper returned an invalid byte count")?;
    let manifest = value
        .get("manifest")
        .and_then(serde_json::Value::as_array)
        .ok_or("Managed helper returned no file manifest")?;
    if manifest.len() as u64 != files
        || serde_json::to_vec(manifest).map_or(true, |b| b.len() > 48 * 1024)
    {
        return Err("Managed helper returned an inconsistent file manifest".into());
    }
    let mut total = 0u64;
    let mut names = std::collections::BTreeSet::new();
    for entry in manifest {
        let object = entry
            .as_object()
            .ok_or("Managed helper returned a malformed manifest entry")?;
        if object.len() != 3 {
            return Err("Managed helper returned an unsupported manifest entry".into());
        }
        let path = object
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("Managed helper returned an invalid manifest path")?;
        let size = object
            .get("bytes")
            .and_then(serde_json::Value::as_u64)
            .ok_or("Managed helper returned an invalid manifest size")?;
        let digest = object
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .ok_or("Managed helper returned an invalid manifest hash")?;
        let supported = if let Some(name) = path
            .strip_prefix("chats/")
            .and_then(|p| p.strip_suffix(".json"))
        {
            name.strip_prefix("chat_")
                .and_then(|name| name.split_once('_'))
                .is_some_and(|(stamp, nonce)| {
                    !stamp.is_empty()
                        && stamp
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                        && nonce.len() == 8
                        && nonce.bytes().all(|b| b.is_ascii_hexdigit())
                })
        } else {
            matches!(
                path,
                "graph/format.json"
                    | "graph/nodes.jsonl"
                    | "graph/chains.jsonl"
                    | "graph/coverage.jsonl"
                    | "graph/positions.json"
                    | "graph/source-organization.json"
            )
        };
        if !supported || !valid_hash(digest) || !names.insert(path) {
            return Err("Managed helper returned an unsafe or duplicate manifest path".into());
        }
        total = total
            .checked_add(size)
            .ok_or("Managed helper returned an overflowing manifest size")?;
    }
    if total != bytes {
        return Err("Managed helper returned an inconsistent manifest byte count".into());
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn request(
    operation: harness_sync_native::managed_bridge::Operation,
) -> Result<serde_json::Value, String> {
    request_via(operation, "bridge", false)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn apply(revision: String, bundle: serde_json::Value) -> Result<serde_json::Value, String> {
    request_via(
        harness_sync_native::managed_bridge::Operation::PrepareApply {
            head: revision,
            bundle,
            consent: true,
        },
        "apply",
        false,
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn confirm_readback() -> Result<bool, String> {
    let value = request_via(
        harness_sync_native::managed_bridge::Operation::ConfirmReadback,
        "confirm",
        false,
    )?;
    Ok(value.get("confirmed").and_then(serde_json::Value::as_bool) == Some(true))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn recover() -> Result<String, String> {
    let value = request_via(
        harness_sync_native::managed_bridge::Operation::Recover,
        "recover",
        false,
    )?;
    ensure_helper_boolean(&value, "recovered")?;
    Ok("recovered".into())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn ensure_helper_boolean(value: &serde_json::Value, field: &str) -> Result<(), String> {
    if value.get(field).and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err("Managed helper did not confirm the requested operation".into())
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn apply(_revision: String, _bundle: serde_json::Value) -> Result<serde_json::Value, String> {
    Err("Managed Apply requires Linux x86_64".into())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn request_via(
    operation: harness_sync_native::managed_bridge::Operation,
    supervisor_operation: &str,
    qa_probe: bool,
) -> Result<serde_json::Value, String> {
    let timeout_seconds = if matches!(
        &operation,
        harness_sync_native::managed_bridge::Operation::Capture { consent: true }
            | harness_sync_native::managed_bridge::Operation::PrepareApply { consent: true, .. }
    ) {
        180
    } else {
        25
    };
    let source = std::env::current_exe().map_err(|_| "LearnHarness Sync executable unavailable")?;
    let helper = stage_helper(source)?;
    let mut cmd = Command::new("/usr/bin/bash");
    cmd.env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C.UTF-8");
    configure_verified_image(&mut cmd)?;
    if qa_probe {
        cmd.env("LEARNHARNESS_SYNC_MANAGED_QA_LOG", "1");
    }
    #[cfg(feature = "managed-acceptance")]
    if supervisor_operation == "apply" {
        if let Ok(failpoint) = std::env::var("LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT") {
            if matches!(
                failpoint.as_str(),
                "pause-after-apply-switch" | "enospc-after-backup"
            ) {
                cmd.env("LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT", failpoint);
            }
        }
    }
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
            supervisor_operation,
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
    let result = match receive.recv_timeout(Duration::from_secs(timeout_seconds)) {
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

#[cfg(all(debug_assertions, target_os = "linux", target_arch = "x86_64"))]
pub fn request_interruption_test() -> Result<serde_json::Value, String> {
    request_via(
        harness_sync_native::managed_bridge::Operation::Snapshot,
        "bridge-delay",
        true,
    )
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

#[cfg(all(
    any(debug_assertions, feature = "managed-acceptance"),
    target_os = "linux",
    target_arch = "x86_64"
))]
pub fn populate_disposable_real_app_profile() -> Result<String, String> {
    qa_real_app_session("qa-populate")
}

#[cfg(all(
    feature = "managed-acceptance",
    target_os = "linux",
    target_arch = "x86_64"
))]
pub fn extend_disposable_real_app_profile() -> Result<String, String> {
    qa_real_app_session("qa-extend")
}

#[cfg(any(
    all(debug_assertions, target_os = "linux", target_arch = "x86_64"),
    all(
        feature = "managed-acceptance",
        target_os = "linux",
        target_arch = "x86_64"
    )
))]
fn qa_real_app_session(operation: &str) -> Result<String, String> {
    if !matches!(operation, "qa-populate" | "qa-extend") {
        return Err("Unsupported disposable LearnHarness test operation".into());
    }
    let packaged_assets = std::env::var_os("APPDIR")
        .map(|appdir| PathBuf::from(appdir).join("usr/lib/LearnHarness Sync/acceptance"));
    struct DisposableAssets(PathBuf);
    impl Drop for DisposableAssets {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let staged_assets = if let Some(source) = packaged_assets.as_ref() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "learnharness-sync-acceptance-{}",
            harness_sync_native::storage::nonce().map_err(|_| "Could not stage test assets")?
        ));
        std::fs::create_dir(&path).map_err(|_| "Could not stage test assets")?;
        let staged = DisposableAssets(path.clone());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|_| "Could not stage test assets")?;
        let copied = Command::new("/usr/bin/cp")
            .arg("-a")
            .arg(source.join("."))
            .arg(&path)
            .status()
            .map_err(|_| "Could not copy packaged test assets")?;
        if !copied.success() {
            return Err("Could not copy packaged test assets".into());
        }
        Some(staged)
    } else {
        None
    };
    let qa_root = match packaged_assets {
        Some(_) => staged_assets.as_ref().unwrap().0.clone(),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("Managed application test assets are unavailable")?
            .to_path_buf(),
    }
    .canonicalize()
    .map_err(|_| "Managed application test assets are unavailable")?;
    let close_window = qa_root.join("build/managed-close-window");
    if std::env::var_os("APPDIR").is_none() {
        let compiled = Command::new("/usr/bin/cc")
            .args([
                "-Wall",
                "-Werror",
                qa_root
                    .join("tests/learnharness/close-window.c")
                    .to_str()
                    .ok_or("Invalid managed application test path")?,
                "-lX11",
                "-o",
            ])
            .arg(&close_window)
            .status()
            .map_err(|_| "Could not build the disposable LearnHarness window closer")?;
        if !compiled.success() {
            return Err("Could not build the disposable LearnHarness window closer".into());
        }
    }
    let mut cmd = Command::new("/usr/bin/bash");
    cmd.env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C.UTF-8");
    configure_verified_image(&mut cmd)?;
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
            qa_root
                .to_str()
                .ok_or("Invalid managed application test path")?,
        ])
        .output()
        .map_err(|_| "Could not start the isolated LearnHarness acceptance run")?;
    if !output.status.success() {
        let diagnostic = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        return Err(diagnostic
            .lines()
            .rev()
            .take(60)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("; "));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report = stdout
        .lines()
        .find(|line| line.starts_with("MANAGED_REAL_APP_REPORT="))
        .ok_or("The real LearnHarness profile report was unavailable")?;
    if stdout.lines().last() != Some("saved") {
        return Err("The real LearnHarness managed profile was not saved cleanly".into());
    }
    Ok(format!("{report}\nsaved"))
}

pub fn run(operation: &str) -> Result<String, String> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err("Managed sessions require Linux x86_64.".into());
    }
    if !matches!(operation, "status" | "launch" | "recover") {
        return Err("Unsupported managed operation.".into());
    }
    if operation == "recover" {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        return recover();
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        return Err("Managed sessions require Linux x86_64.".into());
    }
    let mut cmd = Command::new("/usr/bin/bash");
    cmd.env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C.UTF-8");
    configure_verified_image(&mut cmd)?;
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
        "saved" if operation == "launch" => {
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            if confirm_readback()? {
                return Ok("verified".into());
            }
            Ok("saved".into())
        }
        s @ ("new" | "active" | "recovery" | "awaiting-readback" | "verified" | "saved"
        | "recovered") => Ok(s.to_owned()),
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

    #[test]
    fn snapshot_metadata_rejects_unsafe_manifest_entries() {
        let valid = serde_json::json!({
            "format":"learnharness-0.6.1-disposable-v1",
            "digest":"a".repeat(64),
            "files":1,
            "bytes":2,
            "manifest":[{"path":"chats/chat_abc_1234abcd.json","bytes":2,"sha256":"b".repeat(64)}]
        });
        assert!(super::validate_snapshot_metadata(&valid).is_ok());
        let mut invalid = valid.clone();
        invalid["manifest"][0]["path"] = serde_json::json!("../auth.json");
        assert!(super::validate_snapshot_metadata(&invalid).is_err());
        let mut invalid = valid;
        invalid["bytes"] = serde_json::json!(3);
        assert!(super::validate_snapshot_metadata(&invalid).is_err());
    }
}
