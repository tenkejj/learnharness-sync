//! One desktop process owns discovery, identity, storage and QUIC. No sidecars.
mod managed;
use harness_sync_native::{storage, Engine};
use serde::Deserialize;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{Emitter, Manager};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Command {
    ManagedStatus,
    ManagedLaunch,
    ManagedRecover,
    Status,
    Settings,
    SaveSettings,
    OpenPairing,
    ClosePairing,
    RunSyncTest,
    RenamePeer,
    Invitation,
    StartPairing,
    PairingDecision,
    Revoke,
    Disconnect,
    InitializeSynthetic,
    SyntheticEdit,
    InspectRevision,
    InspectLearnharnessBackup,
    Export,
    Apply,
    Restore,
    ProductionExport,
    ProductionImport,
    Diagnostics,
}
impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::ManagedStatus => "managed_status",
            Self::ManagedLaunch => "managed_launch",
            Self::ManagedRecover => "managed_recover",
            Self::Status => "status",
            Self::Settings => "settings",
            Self::SaveSettings => "save_settings",
            Self::OpenPairing => "open_pairing",
            Self::ClosePairing => "close_pairing",
            Self::RunSyncTest => "run_sync_test",
            Self::RenamePeer => "rename_peer",
            Self::Invitation => "invitation",
            Self::StartPairing => "start_pairing",
            Self::PairingDecision => "pairing_decision",
            Self::Revoke => "revoke",
            Self::Disconnect => "disconnect",
            Self::InitializeSynthetic => "initialize_synthetic",
            Self::SyntheticEdit => "synthetic_edit",
            Self::InspectRevision => "inspect_revision",
            Self::InspectLearnharnessBackup => "inspect_learnharness_backup",
            Self::Export => "export",
            Self::Apply => "apply",
            Self::Restore => "restore",
            Self::ProductionExport => "production_export",
            Self::ProductionImport => "production_import",
            Self::Diagnostics => "diagnostics",
        }
    }
}
struct Desktop {
    engine: Arc<Engine>,
    smoke_report: Option<PathBuf>,
}
#[tauri::command]
async fn engine_request(
    state: tauri::State<'_, Desktop>,
    command: Command,
    args: Value,
) -> Result<Value, String> {
    if let Some(op) = command.name().strip_prefix("managed_") {
        if op == "recover" && args.get("confirmed") != Some(&Value::Bool(true)) {
            return Err(
                "Confirm recovery to retain the interrupted session and resume the last saved one."
                    .into(),
            );
        }
        return tauri::async_runtime::spawn_blocking(move || managed::run(op))
            .await
            .map_err(|_| "Managed supervisor stopped unexpectedly.")?
            .map(Value::String);
    }
    state
        .engine
        .request(command.name(), args)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn smoke_report(
    app: tauri::AppHandle,
    state: tauri::State<'_, Desktop>,
    report: Value,
) -> Result<(), String> {
    let path = state
        .smoke_report
        .as_ref()
        .ok_or("Smoke reporting is disabled")?;
    let b = serde_json::to_vec(&report).map_err(|e| e.to_string())?;
    if b.len() > 16384 {
        return Err("Smoke report exceeds limit".into());
    }
    storage::atomic(path, &b).map_err(|e| e.to_string())?;
    println!("NATIVE_SMOKE_REPORT={}", path.display());
    state.engine.shutdown();
    app.exit(if report["ok"] == true { 0 } else { 1 });
    Ok(())
}
fn main() {
    // Conservative WebKit rendering avoids the reproducible DMA-BUF protocol
    // failure on the target Linux/Wayland desktop. This changes this process
    // only, before GTK or worker threads start, and respects explicit overrides.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    if let Some(arg) = std::env::args().nth(1) {
        let op = match arg.as_str() {
            "--managed-status" => Some("status"),
            "--managed-launch" => Some("launch"),
            "--managed-recover-confirmed" => Some("recover"),
            _ => None,
        };
        if let Some(op) = op {
            match managed::run(op) {
                Ok(state) => println!("{state}"),
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            }
            return;
        }
    }
    let run_smoke = std::env::args().any(|s| s == "--smoke-test");
    let inspect_smoke_backup = std::env::args().any(|s| s == "--smoke-backup");
    let visual = std::env::args().any(|s| s == "--visual-test");
    let smoke = run_smoke || visual;
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![engine_request, smoke_report])
        .setup(move |app| {
            let root = if smoke {
                let p = std::env::temp_dir()
                    .join(format!("harnesssync-native-smoke-{}", storage::nonce()?));
                storage::private_dir(&p)?;
                p
            } else if let Some(path) = std::env::var_os("HARNESS_SYNC_TEST_PROFILE") {
                // This profile holds ONLY engine-generated invented fixtures; it is never adopted as application data.
                PathBuf::from(path)
            } else {
                // Keep the established native store across the bundle identifier change.
                app.path()
                    .data_dir()?
                    .join("community.harnesssync.desktop/native-v2")
            };
            let name = if smoke {
                "Native smoke test".into()
            } else {
                std::env::var("HOSTNAME")
                    .ok()
                    .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
                    .map(|name| name.trim().to_string())
                    .filter(|name| {
                        !name.is_empty() && name.len() <= 80 && !name.chars().any(char::is_control)
                    })
                    .unwrap_or_else(|| "Linux computer".into())
            };
            let engine = tauri::async_runtime::block_on(Engine::start(
                root.clone(),
                name,
                "0.0.0.0:0".parse()?,
                !smoke,
            ))?;
            if visual {
                if let Some(window) = app.get_webview_window("main") {
                    window.set_title("LearnHarness Sync - isolated visual check")?;
                }
            }
            let status_engine = engine.clone();
            let handle = app.handle().clone();
            app.manage(Desktop {
                engine,
                smoke_report: smoke.then(|| root.join("smoke-report.json")),
            });
            tauri::async_runtime::spawn(async move {
                loop {
                    if let Ok(status) = status_engine.status().await {
                        let _ = handle.emit("engine-status", status);
                    }
                    std::thread::yield_now();
                    tauri::async_runtime::spawn_blocking(|| {
                        std::thread::sleep(Duration::from_secs(2))
                    })
                    .await
                    .ok();
                }
            });
            Ok(())
        })
        .on_page_load(move |webview, payload| {
            if run_smoke && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let script = include_str!("smoke.js");
                if inspect_smoke_backup {
                    let _ = webview.eval(format!("window.__CHECK_TEST_BACKUP__ = true;\n{script}"));
                } else {
                    let _ = webview.eval(script);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("LearnHarness Sync could not start");
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(s) = handle.try_state::<Desktop>() {
                s.engine.shutdown();
            }
        }
    });
}
