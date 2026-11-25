use crate::{
    adapter, discovery,
    identity::Identity,
    network,
    storage::{atomic, now, private_dir, read, Store, MAX_METADATA},
};
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use harness_sync_core::{sha256, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub certificate: Vec<u8>,
    pub address: String,
}
#[derive(Clone, Serialize, Debug)]
pub struct Pending {
    pub id: String,
    pub peer_id: String,
    pub name: String,
    pub code: String,
    #[serde(skip)]
    pub decision: Option<bool>,
}
#[derive(Clone)]
pub struct Discovered {
    pub peer: Peer,
    pub seen: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    version: u32,
    name: String,
    #[serde(default)]
    listen_port: u16,
    peers: BTreeMap<String, Peer>,
}
struct Runtime {
    settings: Settings,
    discovered: BTreeMap<String, Discovered>,
    pending: BTreeMap<String, Pending>,
    connections: BTreeMap<String, quinn::Connection>,
    live_receipts: BTreeMap<String, network::Receipt>,
    connecting: BTreeSet<String>,
    errors: BTreeMap<String, String>,
    pairing_until: u64,
    pairing_generation: u64,
    discovery: String,
    verified_received_bytes: u64,
    verified_received_objects: u64,
}
pub struct Engine {
    pub identity: Identity,
    pub endpoint: quinn::Endpoint,
    pub store: Arc<Mutex<Store>>,
    installation: std::result::Result<String, String>,
    runtime: Mutex<Runtime>,
    mdns: Mutex<Option<mdns_sd::ServiceDaemon>>,
    stopped: AtomicBool,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    inspection_lock: Arc<tokio::sync::Semaphore>,
    pub(crate) test_lock: tokio::sync::Mutex<()>,
}
impl Engine {
    pub async fn start(
        root: PathBuf,
        name: String,
        bind: SocketAddr,
        lan_discovery: bool,
    ) -> Result<Arc<Self>> {
        validate_name(&name)?;
        private_dir(&root)?;
        let marker = root.join("ENGINE_PROFILE.json");
        let marker_bytes = br#"{"format":"harness-sync-native-v2","purpose":"synthetic-only"}"#;
        if marker.exists() {
            ensure!(
                read(&marker, 1024)? == marker_bytes,
                "Invalid isolated engine profile marker"
            );
        } else {
            ensure!(
                std::fs::read_dir(&root)?.next().is_none(),
                "Refusing to adopt a nonempty directory as an isolated engine profile"
            );
            atomic(&marker, marker_bytes)?;
        }
        let store = Store::open(root.clone())?;
        let identity = Identity::load(&root)?;
        let config_path = root.join("devices.json");
        let mut settings = if config_path.exists() {
            let s: Settings = serde_json::from_slice(&read(&config_path, MAX_METADATA)?)?;
            ensure!(
                s.version == 1 && s.peers.len() <= 32,
                "Unsupported trusted device store"
            );
            validate_name(&s.name)?;
            for (id, p) in &s.peers {
                validate_peer(p)?;
                ensure!(id == &p.id, "Trusted device identity mismatch");
            }
            s
        } else {
            let s = Settings {
                version: 1,
                name,
                listen_port: 0,
                peers: BTreeMap::new(),
            };
            atomic(&config_path, &serde_json::to_vec(&s)?)?;
            s
        };
        let bind = if bind.port() == 0 && settings.listen_port != 0 {
            SocketAddr::new(bind.ip(), settings.listen_port)
        } else {
            bind
        };
        let endpoint = quinn::Endpoint::server(network::server(&identity, &root)?, bind)?;
        settings.listen_port = endpoint.local_addr()?.port();
        atomic(&config_path, &serde_json::to_vec(&settings)?)?;
        let installation = tokio::task::spawn_blocking(adapter::detect_installation).await?;
        let e = Arc::new(Self {
            installation,
            identity,
            endpoint,
            store: Arc::new(Mutex::new(store)),
            runtime: Mutex::new(Runtime {
                settings,
                discovered: BTreeMap::new(),
                pending: BTreeMap::new(),
                connections: BTreeMap::new(),
                live_receipts: BTreeMap::new(),
                connecting: BTreeSet::new(),
                errors: BTreeMap::new(),
                pairing_until: 0,
                pairing_generation: 0,
                discovery: "Disabled for isolated test".into(),
                verified_received_bytes: 0,
                verified_received_objects: 0,
            }),
            mdns: Mutex::new(None),
            stopped: AtomicBool::new(false),
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            inspection_lock: Arc::new(tokio::sync::Semaphore::new(1)),
            test_lock: tokio::sync::Mutex::new(()),
        });
        let a = e.clone();
        tokio::spawn(async move {
            let limit = Arc::new(tokio::sync::Semaphore::new(8));
            while let Some(incoming) = a.endpoint.accept().await {
                let Ok(permit) = limit.clone().try_acquire_owned() else {
                    incoming.refuse();
                    continue;
                };
                let e = a.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    if network::accept(e.clone(), incoming).await.is_err() {
                        e.diagnostic("Incoming connection refused or interrupted");
                    }
                });
            }
        });
        if lan_discovery {
            match discovery::start(e.clone()) {
                Ok(d) => {
                    *e.mdns.lock().unwrap() = Some(d);
                    e.set_discovery("Discovering devices on your local network");
                }
                Err(_) => e.set_discovery(
                    "LAN discovery unavailable. Use a pairing invitation on both devices.",
                ),
            }
        }
        let a = e.clone();
        tokio::spawn(async move {
            while !a.stopped.load(Ordering::Relaxed) {
                let peers = a.retry_peers();
                for p in peers {
                    let e = a.clone();
                    tokio::spawn(async move {
                        let id = p.id.clone();
                        let result = network::connect(e.clone(), p, false).await;
                        e.finished_connection(&id,result.err().map(|error|format!("Synchronization interrupted: {error}. Check device diagnostics and free disk space; history is retained and retries are automatic.")));
                    });
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        Ok(e)
    }
    pub async fn disk<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let guard = store
                .lock()
                .map_err(|_| anyhow::anyhow!("Local storage lock failed"))?;
            f(&guard)
        })
        .await?
    }
    pub fn name(&self) -> String {
        self.runtime.lock().unwrap().settings.name.clone()
    }
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }
    pub async fn export_managed(
        &self,
        peer_id: &str,
        manifest: crate::learnharness_export::ExportManifest,
        payload: harness_sync_core::Payload,
        consent: bool,
    ) -> Result<Value> {
        ensure!(
            consent,
            "Explicit consent is required before sending real conversation data"
        );
        let peer = self
            .runtime
            .lock()
            .unwrap()
            .settings
            .peers
            .get(peer_id)
            .cloned()
            .context("Choose a paired trusted device")?;
        network::export_managed(self, peer, manifest, payload).await
    }
    pub async fn staged_managed(
        &self,
        revision: &str,
    ) -> Result<(
        crate::learnharness_export::ExportManifest,
        harness_sync_core::Payload,
    )> {
        let revision = revision.to_owned();
        self.disk(move |store| crate::learnharness_export::load(&store.root, &revision))
            .await
    }
    pub fn set_discovery(&self, message: &str) {
        self.runtime.lock().unwrap().discovery = message.into();
    }
    pub fn discovered(&self, peer: Peer) {
        if peer.id == self.identity.id || validate_peer(&peer).is_err() {
            return;
        }
        let mut r = self.runtime.lock().unwrap();
        if r.discovered.len() < 64 || r.discovered.contains_key(&peer.id) {
            r.discovered
                .insert(peer.id.clone(), Discovered { peer, seen: now() });
        }
    }
    pub fn pairing_open(&self) -> bool {
        self.runtime.lock().unwrap().pairing_until > now()
    }
    pub fn add_pending(&self, p: Pending) -> Result<()> {
        let mut r = self.runtime.lock().unwrap();
        ensure!(
            r.pairing_until > now(),
            "Pairing window closed; no new approval request created"
        );
        ensure!(
            r.pending.len() < 4,
            "Too many pairing attempts; retry later"
        );
        ensure!(
            !r.pending.values().any(|old| old.peer_id == p.peer_id),
            "Pairing already pending for this device"
        );
        r.pending.insert(p.id.clone(), p);
        Ok(())
    }
    pub fn pending_decision(&self, id: &str) -> Result<Option<bool>> {
        Ok(self
            .runtime
            .lock()
            .unwrap()
            .pending
            .get(id)
            .context("Pairing no longer active")?
            .decision)
    }
    pub fn remove_pending(&self, id: &str) {
        self.runtime.lock().unwrap().pending.remove(id);
    }
    pub fn trust(&self, peer: Peer) -> Result<()> {
        validate_peer(&peer)?;
        let mut r = self.runtime.lock().unwrap();
        ensure!(
            r.settings.peers.len() < 32 || r.settings.peers.contains_key(&peer.id),
            "Trusted device limit reached"
        );
        // Persist first, then expose trust in memory. A failed disk write grants nothing.
        let mut peers = r.settings.peers.clone();
        peers.insert(peer.id.clone(), peer);
        let next = Settings {
            version: 1,
            name: r.settings.name.clone(),
            listen_port: r.settings.listen_port,
            peers,
        };
        let store = self.store.lock().unwrap();
        atomic(
            &store.root.join("devices.json"),
            &serde_json::to_vec(&next)?,
        )?;
        r.settings = next;
        Ok(())
    }
    pub fn authorize(&self, id: &str, cert: &[u8]) -> Result<()> {
        let r = self.runtime.lock().unwrap();
        let p = r
            .settings
            .peers
            .get(id)
            .context("Untrusted device refused")?;
        ensure!(
            p.certificate == cert,
            "Trusted identity changed; pairing required"
        );
        Ok(())
    }
    pub fn connected(&self, id: &str, c: quinn::Connection) {
        let mut r = self.runtime.lock().unwrap();
        r.live_receipts.remove(id);
        if let Some(old) = r.connections.insert(id.into(), c) {
            old.close(0u32.into(), b"Superseded connection");
        }
        r.errors.remove(id);
    }
    pub fn disconnected(&self, id: &str) {
        let mut r = self.runtime.lock().unwrap();
        if r.connections
            .get(id)
            .is_some_and(|c| c.close_reason().is_some())
        {
            r.connections.remove(id);
            r.live_receipts.remove(id);
        }
    }
    pub async fn synced(
        &self,
        id: &str,
        connection: usize,
        receipt: network::Receipt,
    ) -> Result<()> {
        let peer_id = id.to_string();
        let live_receipt = receipt.clone();
        let id = id.to_string();
        self.disk(move |s| {
            let mut exchanges = crate::sync_test::exchanges(s)?;
            let mut test = crate::sync_test::latest(s)?;
            if test["state"] == "pending" && test["device"] == id {
                let expected = test["snapshot"].as_str();
                if receipt.conflict {
                    test["state"] = json!("conflict");
                    test["message"] =
                        json!("Two versions need attention. Both versions have been preserved.");
                } else if expected.is_some()
                    && receipt.head.as_deref() == expected
                    && receipt.received.as_deref() == expected
                    && s.ledger()?.head.as_deref() == expected
                {
                    test["state"] = json!("success");
                    test["message"] =
                        json!("The other computer received, verified and adopted your test data.");
                }
                if test["state"] != "pending" {
                    test["finished"] = json!(now());
                    atomic(&s.root.join("sync-test.json"), &serde_json::to_vec(&test)?)?;
                }
            }
            // An unchanged heartbeat proves liveness but is not a new data
            // exchange. Retain its meaningful time and avoid needless fsyncs.
            if exchanges.get(&id).is_some_and(|old| old.receipt == receipt) {
                return Ok(());
            }
            exchanges.insert(
                id,
                crate::sync_test::Exchange {
                    time: now(),
                    receipt,
                },
            );
            while exchanges.len() > 64 {
                if let Some(oldest) = exchanges
                    .iter()
                    .min_by_key(|(_, value)| value.time)
                    .map(|(id, _)| id.clone())
                {
                    exchanges.remove(&oldest);
                }
            }
            atomic(
                &s.root.join("exchanges.json"),
                &serde_json::to_vec(&exchanges)?,
            )
        })
        .await?;
        let mut r = self.runtime.lock().unwrap();
        if r.connections
            .get(&peer_id)
            .is_some_and(|c| c.stable_id() == connection && c.close_reason().is_none())
        {
            r.live_receipts.insert(peer_id, live_receipt);
        }
        Ok(())
    }
    pub fn received_verified(&self, bytes: usize) {
        let mut r = self.runtime.lock().unwrap();
        r.verified_received_bytes = r.verified_received_bytes.saturating_add(bytes as u64);
        r.verified_received_objects = r.verified_received_objects.saturating_add(1);
    }
    pub fn peer_error(&self, id: &str, message: String) {
        let mut r = self.runtime.lock().unwrap();
        if r.settings.peers.contains_key(id) {
            r.errors.insert(id.into(), message);
        }
    }
    pub fn diagnostic(&self, message: &str) {
        let mut r = self.runtime.lock().unwrap();
        r.errors.insert("engine".into(), message.into());
    }
    fn retry_peers(&self) -> Vec<Peer> {
        let mut r = self.runtime.lock().unwrap();
        r.discovered
            .retain(|_, d| now().saturating_sub(d.seen) < 120);
        let peers: Vec<_> = r
            .settings
            .peers
            .values()
            .filter(|p| {
                self.identity.id < p.id
                    && !r
                        .connections
                        .get(&p.id)
                        .is_some_and(|c| c.close_reason().is_none())
                    && !r.connecting.contains(&p.id)
            })
            .cloned()
            .collect();
        peers
            .into_iter()
            .map(|mut p| {
                if let Some(d) = r.discovered.get(&p.id) {
                    p.address = d.peer.address.clone();
                }
                r.connecting.insert(p.id.clone());
                p
            })
            .collect()
    }
    fn finished_connection(&self, id: &str, error: Option<String>) {
        let mut r = self.runtime.lock().unwrap();
        r.connecting.remove(id);
        if let Some(message) = error {
            r.errors.insert(id.into(), message);
        }
    }
    pub fn shutdown(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.endpoint.close(0u32.into(), b"Application closed");
        if let Some(d) = self.mdns.lock().unwrap().take() {
            let _ = d.shutdown();
        }
    }
    pub fn invitation(&self) -> Result<String> {
        let port = self.local_addr()?.port();
        let mut addresses: Vec<String> = if_addrs::get_if_addrs()?
            .into_iter()
            .filter_map(|i| match i.ip() {
                std::net::IpAddr::V4(ip) if ip.is_private() && !ip.is_loopback() => {
                    Some(SocketAddr::new(ip.into(), port).to_string())
                }
                _ => None,
            })
            .collect();
        if addresses.is_empty() {
            addresses.push(format!("127.0.0.1:{port}"));
        }
        addresses.sort();
        addresses.dedup();
        Ok(STANDARD.encode(serde_json::to_vec(&json!({"version":1,"name":self.name(),"certificate":STANDARD.encode(&self.identity.certificate),"addresses":addresses}))?))
    }
    pub async fn status(&self) -> Result<Value> {
        let (ledger, snapshots, path, exchanges, sync_test, managed_exports) = self
            .disk(|s| {
                Ok((
                    s.ledger()?,
                    s.list()?,
                    s.root.to_string_lossy().to_string(),
                    crate::sync_test::exchanges(s)?,
                    crate::sync_test::latest(s)?,
                    crate::learnharness_export::list(&s.root)?,
                ))
            })
            .await?;
        let r = self.runtime.lock().unwrap();
        let peers:Vec<_>=r.settings.peers.values().map(|p|json!({"id":p.id,"name":p.name,"connected":r.connections.get(&p.id).is_some_and(|c|c.close_reason().is_none()),"last_sync":exchanges.get(&p.id).map(|x| x.time),"receipt":if r.connections.get(&p.id).is_some_and(|c|c.close_reason().is_none()) { r.live_receipts.get(&p.id) } else { exchanges.get(&p.id).map(|x| &x.receipt) },"error":r.errors.get(&p.id)})).collect();
        let mut snaps = Vec::new();
        let mut backups = Vec::new();
        for (id, m) in snapshots {
            let v = json!({"id":id,"device":if m.device==self.identity.id { r.settings.name.clone() } else { r.settings.peers.get(&m.device).map(|p|p.name.clone()).unwrap_or_else(||m.device[..12].into()) },"time":0,"files":m.files.len(),"bytes":m.files.iter().map(|f|f.size).sum::<u64>(),"format":m.format,"verified":true,"applied":ledger.head.as_ref()==Some(&id)});
            if ledger.backups.contains(&id) {
                backups.push(v.clone());
            }
            snaps.push(v);
        }
        let discovered: Vec<_> = r
            .discovered
            .values()
            .filter(|d| {
                now().saturating_sub(d.seen) < 120 && !r.settings.peers.contains_key(&d.peer.id)
            })
            .map(|d| json!({"id":d.peer.id,"name":d.peer.name,"address":d.peer.address}))
            .collect();
        Ok(
            json!({"mode":if ledger.initialized {"synthetic"}else{"monitor"},"state":if !ledger.conflicts.is_empty(){"Conflict preserved"}else if ledger.head.is_some(){"Verified isolated history"}else{"Ready to add a device"},"device":r.settings.name,"data_path":path,"detected":self.installation.is_ok(),"head":ledger.head,"sync_test":sync_test,"snapshots":snaps,"backups":backups,"events":ledger.events,"conflicts":ledger.conflicts,"automatic":ledger.initialized,"managed_exports":managed_exports,"integration":"LearnHarness 0.6.1 and 0.7.0 managed export and transactional apply; explicit consent and application readback are required","transport_installed":true,"transport":{"state":if peers.iter().any(|p|p["connected"]==true){"connected"}else{"listening"},"device_id":self.identity.id,"peers":peers,"message":r.discovery},"discovered":discovered,"pending":r.pending.values().collect::<Vec<_>>(),"pairing_open":r.pairing_until>now(),"listen_port":self.local_addr()?.port(),"diagnostics":r.errors,"transfer":{"verified_received_bytes":r.verified_received_bytes,"verified_received_objects":r.verified_received_objects},"limitations":[adapter::PRODUCTION_BLOCK,"LearnHarness export/apply is restricted to verified 0.6.1 or 0.7.0 managed profiles; unsupported records, cross-version applies, and conflicting edits are refused.","Incoming revisions remain quarantined until the user explicitly applies them. Application readback is required to complete synchronization.","Same-LAN IPv4 only. Internet traversal and relays are unsupported.","Closing LearnHarness Sync stops syncing. No background daemon is installed.","All committed objects are retained; storage is capped at 1 GiB."]}),
        )
    }
    pub async fn request(self: &Arc<Self>, command: &str, args: Value) -> Result<Value> {
        ensure!(
            serde_json::to_vec(&args)?.len() <= 96 * 1024,
            "Command exceeds limit"
        );
        let arg = |key: &str| {
            args.get(key)
                .and_then(Value::as_str)
                .context("Required command argument missing")
        };
        match command {
            "status" => self.status().await,
            "inspect_learnharness_backup" => {
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                {
                    let path = std::path::PathBuf::from(arg("path")?);
                    let permit = self
                        .inspection_lock
                        .clone()
                        .try_acquire_owned()
                        .context("A backup inspection is already running")?;
                    tokio::task::spawn_blocking(move || {
                        let _permit = permit;
                        crate::backup_inspection::inspect(&path)
                    })
                    .await?
                }
                #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
                {
                    anyhow::bail!("Backup inspection requires Linux x86_64")
                }
            }
            "settings" => {
                // A desktop can stay open while the upstream AppImage changes.
                // Recheck exact bytes whenever Settings opens; managed commands
                // perform their own independent check immediately before use.
                let detected = tokio::task::spawn_blocking(adapter::detect_installation).await?;
                let compatible = detected.is_ok();
                let installation = match detected {
                    Ok(version) | Err(version) => version,
                };
                Ok(
                    json!({"version":2,"name":self.name(),"production_mutation":false,"installation":installation,"compatible":compatible,"profile":self.disk(|s|Ok(s.root.to_string_lossy().to_string())).await?}),
                )
            }
            "save_settings" => {
                let name = arg("name")?.to_string();
                validate_name(&name)?;
                {
                    let mut r = self.runtime.lock().unwrap();
                    let next = Settings {
                        version: 1,
                        name,
                        listen_port: r.settings.listen_port,
                        peers: r.settings.peers.clone(),
                    };
                    let store = self.store.lock().unwrap();
                    atomic(
                        &store.root.join("devices.json"),
                        &serde_json::to_vec(&next)?,
                    )?;
                    r.settings = next;
                }
                Ok(
                    json!({"saved":true,"message":"Device name saved. Restart LearnHarness Sync to refresh its discovery advertisement."}),
                )
            }
            "run_sync_test" => crate::sync_test::run(self, arg("device")?).await,
            "close_pairing" => {
                let mut r = self.runtime.lock().unwrap();
                r.pairing_until = 0;
                r.pairing_generation = r.pairing_generation.wrapping_add(1);
                for p in r.pending.values_mut() {
                    if p.decision.is_none() {
                        p.decision = Some(false);
                    }
                }
                Ok(json!({"closed":true}))
            }
            "rename_peer" => {
                let id = arg("device")?;
                let name = arg("name")?.trim().to_string();
                validate_name(&name)?;
                let mut r = self.runtime.lock().unwrap();
                let mut next = Settings {
                    version: 1,
                    name: r.settings.name.clone(),
                    listen_port: r.settings.listen_port,
                    peers: r.settings.peers.clone(),
                };
                next.peers
                    .get_mut(id)
                    .context("Device no longer paired")?
                    .name = name;
                let store = self.store.lock().unwrap();
                atomic(
                    &store.root.join("devices.json"),
                    &serde_json::to_vec(&next)?,
                )?;
                r.settings = next;
                Ok(json!({"saved":true}))
            }
            "open_pairing" => {
                let mut r = self.runtime.lock().unwrap();
                r.pairing_until = now() + 120;
                r.pairing_generation = r.pairing_generation.wrapping_add(1);
                Ok(json!({"expires_in":120}))
            }
            "invitation" => Ok(json!({"invitation":self.invitation()?})),
            "start_pairing" => {
                let peer = if let Some(inv) = args.get("invitation").and_then(Value::as_str) {
                    parse_invitation(inv)?
                } else {
                    self.runtime
                        .lock()
                        .unwrap()
                        .discovered
                        .get(arg("device")?)
                        .context("Device is no longer discovered; try an invitation")?
                        .peer
                        .clone()
                };
                ensure!(
                    peer.id != self.identity.id,
                    "Choose the other computer's invitation"
                );
                // A revoked automatic connection may still be unwinding its
                // stream. Wait for it to release the slot rather than forcing
                // the user to guess when Pair becomes safe again.
                let generation = self.runtime.lock().unwrap().pairing_generation;
                let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
                loop {
                    let started = {
                        let mut r = self.runtime.lock().unwrap();
                        ensure!(
                            r.pairing_generation == generation,
                            "Pairing window closed; request canceled"
                        );
                        ensure!(!r.settings.peers.contains_key(&peer.id), "This device is already paired. Remove it on both computers before pairing again.");
                        ensure!(
                            !r.pending.values().any(|p| p.peer_id == peer.id),
                            "Pairing already in progress"
                        );
                        if r.connecting.contains(&peer.id) {
                            false
                        } else {
                            r.connecting.insert(peer.id.clone());
                            true
                        }
                    };
                    if started {
                        break;
                    }
                    ensure!(
                        tokio::time::Instant::now() < deadline,
                        "The previous connection is still closing. Try pairing again in a moment."
                    );
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                self.runtime.lock().unwrap().pairing_until = now() + 120;
                let e = self.clone();
                tokio::spawn(async move {
                    let id = peer.id.clone();
                    let result = network::connect(e.clone(), peer, true).await;
                    e.finished_connection(&id,result.err().map(|e|format!("Pairing did not complete: {e}. Click Add Device on both computers and try again.")));
                });
                Ok(json!({"started":true}))
            }
            "pairing_decision" => {
                let mut r = self.runtime.lock().unwrap();
                let p = r
                    .pending
                    .get_mut(arg("id")?)
                    .context("Pairing expired; start again")?;
                ensure!(p.decision.is_none(), "Decision already recorded");
                p.decision = Some(
                    args.get("accept")
                        .and_then(Value::as_bool)
                        .context("Pairing decision required")?,
                );
                Ok(json!({"recorded":true}))
            }
            "revoke" => {
                let id = arg("device")?;
                ensure!(valid_id(id), "Invalid identity");
                let mut r = self.runtime.lock().unwrap();
                let mut peers = r.settings.peers.clone();
                peers.remove(id);
                let next = Settings {
                    version: 1,
                    name: r.settings.name.clone(),
                    listen_port: r.settings.listen_port,
                    peers,
                };
                let store = self.store.lock().unwrap();
                atomic(
                    &store.root.join("devices.json"),
                    &serde_json::to_vec(&next)?,
                )?;
                r.settings = next;
                r.live_receipts.remove(id);
                if let Some(c) = r.connections.remove(id) {
                    c.close(0u32.into(), b"Device revoked");
                }
                for p in r.pending.values_mut().filter(|p| p.peer_id == id) {
                    p.decision = Some(false);
                }
                Ok(json!({"revoked":true}))
            }
            "disconnect" => {
                let id = arg("device")?;
                let mut r = self.runtime.lock().unwrap();
                if let Some(c) = r.connections.remove(id) {
                    c.close(0u32.into(), b"Disconnected");
                }
                Ok(
                    json!({"disconnected":true,"message":"Trusted devices reconnect automatically. Remove the device to stop sharing."}),
                )
            }
            "initialize_synthetic" => {
                ensure!(
                    args.get("confirmed") == Some(&Value::Bool(true)),
                    "Confirm invented data only"
                );
                self.disk(|s| s.initialize()).await?;
                Ok(json!({"initialized":true}))
            }
            "synthetic_edit" => {
                let payload = adapter::fixture(arg("text")?)?;
                let device = self.identity.id.clone();
                let id = self.disk(move |s| s.publish(&device, &payload)).await?;
                Ok(json!({"snapshot":id}))
            }
            "inspect_revision" => {
                let id = arg("snapshot")?.to_string();
                self.disk(move |s| {
                    let m = s.manifest(&id)?;
                    let payload = s.payload(&m)?;
                    let note: Value = serde_json::from_slice(
                        payload
                            .get("chats/example.json")
                            .context("Test note unavailable")?,
                    )?;
                    Ok(json!({"text": note["text"].as_str().context("Invalid test note")?}))
                })
                .await
            }
            "export" => {
                self.disk(|s| {
                    s.writer_safe()?;
                    let l = s.ledger()?;
                    ensure!(l.initialized, "Create isolated workspace first");
                    if let Some(h) = l.head {
                        s.history(&h)?;
                        Ok(json!({"snapshot":h}))
                    } else {
                        bail!("Write an invented note first")
                    }
                })
                .await
            }
            "apply" => {
                ensure!(
                    args.get("confirmed") == Some(&Value::Bool(true)),
                    "Confirm isolated import"
                );
                let id = arg("snapshot")?.to_string();
                let d = self.disk(move |s| s.reconcile(&id)).await?;
                ensure!(
                    d != harness_sync_core::Decision::Conflict,
                    "Divergence preserved; no import applied"
                );
                Ok(json!({"result":format!("{d:?}")}))
            }
            "restore" => {
                ensure!(
                    arg("confirmation")? == "RESTORE SYNTHETIC BACKUP",
                    "Explicit restore confirmation required"
                );
                let id = arg("snapshot")?.to_string();
                let device = self.identity.id.clone();
                let id = self.disk(move |s| s.restore(&device, &id)).await?;
                Ok(json!({"snapshot":id}))
            }
            "production_export" | "production_import" | "learnharness_audit" => {
                adapter::production_operation()?;
                unreachable!()
            }
            "diagnostics" => self.status().await,
            _ => bail!("Unsupported native engine command"),
        }
    }
}
fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.trim().is_empty() && name.len() <= 80 && !name.chars().any(char::is_control),
        "Use a device name of 1–80 bytes without control characters"
    );
    Ok(())
}
fn validate_peer(peer: &Peer) -> Result<()> {
    validate_name(&peer.name)?;
    ensure!(
        valid_id(&peer.id) && sha256(&peer.certificate) == peer.id,
        "Invalid peer identity"
    );
    crate::identity::public_key(&peer.certificate)?;
    let a: SocketAddr = peer.address.parse()?;
    ensure!(
        a.is_ipv4() && !a.ip().is_unspecified() && !a.ip().is_multicast() && a.port() != 0,
        "Use a reachable IPv4 address and port"
    );
    Ok(())
}
fn parse_invitation(inv: &str) -> Result<Peer> {
    ensure!(inv.len() <= 16384, "Invitation exceeds limit");
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Invitation {
        version: u32,
        name: String,
        certificate: String,
        addresses: Vec<String>,
    }
    let i: Invitation = serde_json::from_slice(&STANDARD.decode(inv.trim())?)?;
    ensure!(
        i.version == 1 && i.addresses.len() <= 32,
        "Unsupported invitation"
    );
    let cert = STANDARD.decode(i.certificate)?;
    let p = Peer {
        id: sha256(&cert),
        name: i.name,
        certificate: cert,
        address: i
            .addresses
            .first()
            .context("Invitation has no address")?
            .clone(),
    };
    validate_peer(&p)?;
    Ok(p)
}
