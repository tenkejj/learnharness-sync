//! One desktop process owns discovery, identity, storage and QUIC. No sidecars.
mod managed;
use harness_sync_native::{storage, Engine};
use serde::Deserialize;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{Emitter, Manager};

// The exact 0.6.1 managed-profile apply path passed packaged A→B→A,
// application readback, active-writer refusal, and interrupted-apply recovery.
// Other LearnHarness versions and profile types remain unsupported.
const MANAGED_APPLY_QUALIFIED: bool = true;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Command {
    ManagedStatus,
    ManagedLaunch,
    ManagedRecover,
    ManagedSnapshot,
    ManagedExport,
    ManagedApply,
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
            Self::ManagedSnapshot => "managed_snapshot",
            Self::ManagedExport => "managed_export",
            Self::ManagedApply => "managed_apply",
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
    dispatch_engine_request(state.engine.clone(), command, args).await
}

async fn dispatch_engine_request(
    engine: Arc<Engine>,
    command: Command,
    args: Value,
) -> Result<Value, String> {
    if let Some(op) = command.name().strip_prefix("managed_") {
        if op == "export" {
            if args.get("consent") != Some(&Value::Bool(true)) {
                return Err(
                    "Confirm before sending real conversation data to the paired device.".into(),
                );
            }
            let peer = args
                .get("device")
                .and_then(Value::as_str)
                .ok_or("Choose a paired device")?
                .to_owned();
            let (manifest, payload) =
                tauri::async_runtime::spawn_blocking(|| managed::capture_for_export(true))
                    .await
                    .map_err(|_| "Managed export helper stopped unexpectedly.")??;
            return engine
                .export_managed(&peer, manifest, payload, true)
                .await
                .map_err(|e| e.to_string());
        }
        if op == "snapshot" {
            return tauri::async_runtime::spawn_blocking(managed::snapshot)
                .await
                .map_err(|_| "Managed snapshot helper stopped unexpectedly.")?;
        }
        if op == "apply" {
            if !MANAGED_APPLY_QUALIFIED {
                return Err("Applying received LearnHarness data is unavailable in this build. The verified copy remains quarantined.".into());
            }
            if args.get("confirmed") != Some(&Value::Bool(true)) {
                return Err("Confirm applying this quarantined LearnHarness revision to the managed profile.".into());
            }
            let revision = args
                .get("revision")
                .and_then(Value::as_str)
                .ok_or("Choose a quarantined LearnHarness revision")?
                .to_owned();
            let (manifest, payload) = engine
                .staged_managed(&revision)
                .await
                .map_err(|e| e.to_string())?;
            let bundle =
                harness_sync_native::learnharness_export::bundle_value(&manifest, &payload)
                    .map_err(|e| e.to_string())?;
            let result = tauri::async_runtime::spawn_blocking(move || {
                managed::apply(manifest.revision, bundle)
            })
            .await
            .map_err(|_| "Managed apply helper stopped unexpectedly.")??;
            return Ok(result);
        }
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
    engine
        .request(command.name(), args)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(feature = "managed-acceptance")]
fn managed_acceptance_console(engine: Arc<Engine>, app: tauri::AppHandle) {
    use std::io::{BufRead, Write};
    println!("ACCEPTANCE_READY");
    let _ = std::io::stdout().flush();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.len() > 128 * 1024 {
            println!("ACCEPTANCE_ERROR command line exceeds limit");
            let _ = std::io::stdout().flush();
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                println!("ACCEPTANCE_ERROR malformed request");
                let _ = std::io::stdout().flush();
                continue;
            }
        };
        if request.get("command").and_then(Value::as_str) == Some("__shutdown") {
            println!("ACCEPTANCE_RESULT {{\"ok\":true}}");
            let _ = std::io::stdout().flush();
            engine.shutdown();
            app.exit(0);
            return;
        }
        if request.get("command").and_then(Value::as_str) == Some("__managed_capture_for_test") {
            let captured =
                tauri::async_runtime::block_on(tauri::async_runtime::spawn_blocking(|| {
                    let (manifest, payload) = managed::capture_for_export(true)?;
                    harness_sync_native::learnharness_export::bundle_value(&manifest, &payload)
                        .map_err(|error| error.to_string())
                }));
            match captured {
                Ok(Ok(value)) => println!("ACCEPTANCE_RESULT {{\"ok\":true,\"result\":{}}}", value),
                Ok(Err(error)) => println!(
                    "ACCEPTANCE_RESULT {{\"ok\":false,\"error\":{}}}",
                    serde_json::to_string(&error).unwrap()
                ),
                Err(_) => println!(
                    "ACCEPTANCE_RESULT {{\"ok\":false,\"error\":\"capture worker stopped\"}}"
                ),
            }
            let _ = std::io::stdout().flush();
            continue;
        }
        if request.get("command").and_then(Value::as_str) == Some("__qa_populate_profile")
            || request.get("command").and_then(Value::as_str) == Some("__qa_extend_profile")
        {
            let command = request
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let work = tauri::async_runtime::spawn_blocking(move || {
                if command == "__qa_populate_profile" {
                    managed::populate_disposable_real_app_profile()
                } else {
                    managed::extend_disposable_real_app_profile()
                }
            });
            match tauri::async_runtime::block_on(work) {
                Ok(Ok(report)) => println!(
                    "ACCEPTANCE_RESULT {{\"ok\":true,\"result\":{}}}",
                    serde_json::to_string(&report).unwrap()
                ),
                Ok(Err(error)) => println!(
                    "ACCEPTANCE_RESULT {{\"ok\":false,\"error\":{}}}",
                    serde_json::to_string(&error).unwrap()
                ),
                Err(_) => println!(
                    "ACCEPTANCE_RESULT {{\"ok\":false,\"error\":\"test session stopped\"}}"
                ),
            }
            let _ = std::io::stdout().flush();
            continue;
        }
        let Some(command) = request
            .get("command")
            .and_then(Value::as_str)
            .and_then(|name| {
                serde_json::from_value::<Command>(Value::String(name.to_owned())).ok()
            })
        else {
            println!("ACCEPTANCE_ERROR unknown command");
            let _ = std::io::stdout().flush();
            continue;
        };
        let args = request
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let result =
            tauri::async_runtime::block_on(dispatch_engine_request(engine.clone(), command, args));
        match result {
            Ok(value) => println!("ACCEPTANCE_RESULT {{\"ok\":true,\"result\":{}}}", value),
            Err(error) => println!(
                "ACCEPTANCE_RESULT {{\"ok\":false,\"error\":{}}}",
                serde_json::to_string(&error)
                    .unwrap_or_else(|_| "\"error encoding refusal\"".into())
            ),
        }
        let _ = std::io::stdout().flush();
    }
    engine.shutdown();
    app.exit(0);
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
    let bridge_args: Vec<String> = std::env::args().collect();
    #[cfg(debug_assertions)]
    let interrupt_probe = bridge_args.len() == 3
        && bridge_args[1] == "--managed-bridge-stdio"
        && bridge_args[2] == "--test-pause-before-read";
    #[cfg(not(debug_assertions))]
    let interrupt_probe = false;
    if (bridge_args.len() == 2 && bridge_args[1] == "--managed-bridge-stdio") || interrupt_probe {
        if interrupt_probe {
            std::thread::sleep(Duration::from_secs(8));
        }
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut input = stdin.lock();
        let mut output = stdout.lock();
        let result = harness_sync_native::managed_bridge::Server::serve_stdio_once(
            &mut input,
            &mut output,
            |operation| match operation {
                harness_sync_native::managed_bridge::Operation::Snapshot => {
                    let vault = std::path::Path::new("/capture");
                    anyhow::ensure!(
                        std::fs::read(vault.join("active"))? == b"bridge\n",
                        "Managed capture is not active"
                    );
                    let generation = vault.join("generation");
                    anyhow::ensure!(generation.is_dir(), "Managed generation is unavailable");
                    let path = generation.join("config/LearnHarness/data");
                    let format = std::env::var("LEARNHARNESS_SYNC_PROFILE_FORMAT")?;
                    harness_sync_native::managed_profile::Tree::snapshot_summary_for_format(
                        &path, &format,
                    )
                }
                harness_sync_native::managed_bridge::Operation::Capture { consent: true } => {
                    let vault = std::path::Path::new("/capture");
                    anyhow::ensure!(
                        std::fs::read(vault.join("active"))? == b"bridge\n",
                        "Managed capture is not active"
                    );
                    let generation = vault.join("generation");
                    anyhow::ensure!(generation.is_dir(), "Managed generation is unavailable");
                    let format = std::env::var("LEARNHARNESS_SYNC_PROFILE_FORMAT")?;
                    harness_sync_native::managed_profile::Tree::export_bundle_for_format(
                        &generation.join("config/LearnHarness/data"),
                        &format,
                    )
                }
                harness_sync_native::managed_bridge::Operation::PrepareApply {
                    head,
                    bundle,
                    consent: true,
                } => {
                    let vault = std::path::Path::new("/vault");
                    anyhow::ensure!(
                        std::fs::read(vault.join("active"))? == b"apply\n",
                        "Managed apply is not exclusively active"
                    );
                    let (manifest, payload) =
                        harness_sync_native::learnharness_export::parse_bundle(&bundle)?;
                    anyhow::ensure!(
                        manifest.revision == head,
                        "Apply revision does not match its verified content"
                    );
                    let applied = harness_sync_native::managed_profile::apply_incoming(
                        vault, &manifest, &payload,
                    )?;
                    #[cfg(feature = "managed-acceptance")]
                    if std::env::var("LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT").as_deref()
                        == Ok("pause-after-apply-switch")
                    {
                        eprintln!("ACCEPTANCE_FAILPOINT=apply-switched");
                        std::thread::sleep(Duration::from_secs(300));
                    }
                    Ok(applied)
                }
                harness_sync_native::managed_bridge::Operation::ConfirmReadback => {
                    let vault = std::path::Path::new("/vault");
                    anyhow::ensure!(
                        std::fs::read(vault.join("active"))? == b"confirm\n",
                        "Managed application readback is not exclusively active"
                    );
                    harness_sync_native::managed_profile::confirm_pending_readback(vault)
                }
                harness_sync_native::managed_bridge::Operation::Recover => {
                    harness_sync_native::managed_profile::recover_transaction(std::path::Path::new(
                        "/vault",
                    ))
                }
                operation => Ok(serde_json::json!({
                    "error": format!("Operation {operation:?} is not enabled in the managed bridge")
                })),
            },
        );
        if let Err(error) = result {
            eprintln!("Managed bridge refused: {error:#}");
            std::process::exit(1);
        }
        return;
    }
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
    if std::env::args().nth(1).as_deref() == Some("--managed-snapshot") {
        match managed::snapshot() {
            Ok(value) => println!("{value}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    #[cfg(all(debug_assertions, target_os = "linux", target_arch = "x86_64"))]
    if std::env::args().nth(1).as_deref() == Some("--managed-populate-real-app-test") {
        match managed::populate_disposable_real_app_profile() {
            Ok(report) => println!("{report}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    #[cfg(all(debug_assertions, target_os = "linux", target_arch = "x86_64"))]
    if std::env::args().nth(1).as_deref() == Some("--managed-snapshot-interruption-test") {
        match managed::request_interruption_test() {
            Ok(_) => std::process::exit(2),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    // This diagnostic exists only in debug builds so the cross-UID acceptance
    // test can prove the helper rejects an otherwise well-formed request.
    #[cfg(debug_assertions)]
    if std::env::args().nth(1).as_deref() == Some("--managed-bridge-unauthorized-test") {
        match managed::request(harness_sync_native::managed_bridge::Operation::Status) {
            Ok(value) if value["error"].as_str().is_some() => {
                println!("{}", value["error"].as_str().unwrap());
            }
            Ok(_) => std::process::exit(2),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let run_smoke = std::env::args().any(|s| s == "--smoke-test");
    let inspect_smoke_backup = std::env::args().any(|s| s == "--smoke-backup");
    let inspect_managed_snapshot = std::env::args().any(|s| s == "--smoke-managed-snapshot");
    let visual = std::env::args().any(|s| s == "--visual-test");
    let acceptance_peer = std::env::args().any(|s| s == "--managed-acceptance-peer");
    #[cfg(not(feature = "managed-acceptance"))]
    if acceptance_peer {
        eprintln!("This test-only peer console is absent from production builds.");
        std::process::exit(64);
    }
    #[cfg(feature = "managed-acceptance")]
    if acceptance_peer && std::env::var("LEARNHARNESS_SYNC_ACCEPTANCE").as_deref() != Ok("1") {
        eprintln!("The test-only peer console requires explicit acceptance mode.");
        std::process::exit(64);
    }
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
            #[cfg(feature = "managed-acceptance")]
            if acceptance_peer {
                let console_engine = status_engine.clone();
                let console_app = handle.clone();
                std::thread::spawn(move || managed_acceptance_console(console_engine, console_app));
            }
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
                let mut prelude = String::new();
                if inspect_smoke_backup {
                    prelude.push_str("window.__CHECK_TEST_BACKUP__ = true;\n");
                }
                if inspect_managed_snapshot {
                    prelude.push_str("window.__CHECK_MANAGED_SNAPSHOT__ = true;\n");
                }
                let _ = webview.eval(format!("{prelude}{script}"));
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
