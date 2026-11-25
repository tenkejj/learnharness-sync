//! HSQ/1: TLS 1.3 QUIC with exact certificate roots; Ed25519 client proof is
//! bound to the TLS exporter. No insecure TLS verifier and no 0-RTT.
use crate::{
    engine::{Engine, Peer, Pending},
    identity,
    learnharness_export::{self, ExportManifest},
    storage::{nonce, MAX_METADATA},
};
use anyhow::{bail, ensure, Context, Result};
use harness_sync_core::{sha256, valid_id, Manifest, MAX_FILE, MAX_HISTORY};
use quinn::{Connection, RecvStream, SendStream};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::BTreeSet, net::SocketAddr, sync::Arc, time::Duration};

pub const VERSION: u32 = 2;
pub const ALPN: &[u8] = b"harnesssync/2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub version: u32,
    pub capabilities: Vec<String>,
    pub name: String,
    pub certificate: Vec<u8>,
    pub signature: Vec<u8>,
    pub pairing: bool,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Message {
    ManagedExportBegin {
        manifest: ExportManifest,
    },
    ManagedExportReady {
        revision: String,
    },
    ManagedExportFile {
        path: String,
        hash: String,
    },
    ManagedExportEnd {
        revision: String,
    },
    ManagedExportComplete {
        revision: String,
        digest: String,
        staged: bool,
    },
    Head {
        head: Option<String>,
    },
    Manifest {
        id: String,
    },
    Blob {
        id: String,
    },
    End,
    Ack,
    Committed {
        head: Option<String>,
        received: Option<String>,
        conflict: bool,
    },
    Decision {
        accept: bool,
    },
    Error,
}

pub fn server(
    identity: &identity::Identity,
    root: &std::path::Path,
) -> Result<quinn::ServerConfig> {
    let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from(identity.certificate.clone())],
        PrivatePkcs8KeyDer::from(identity.private_der(root)?).into(),
    )?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let mut c = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls)?,
    ));
    c.transport = transport();
    Ok(c)
}
fn transport() -> Arc<quinn::TransportConfig> {
    let mut t = quinn::TransportConfig::default();
    t.max_concurrent_bidi_streams(1u32.into())
        .max_concurrent_uni_streams(0u32.into());
    t.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    t.keep_alive_interval(Some(Duration::from_secs(5)));
    t.receive_window((2u32 * 1024 * 1024).into())
        .stream_receive_window((1024u32 * 1024).into());
    Arc::new(t)
}
pub fn client(cert: &[u8]) -> Result<quinn::ClientConfig> {
    identity::public_key(cert)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(cert.to_vec()))?;
    let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_root_certificates(roots)
    .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    tls.enable_early_data = false;
    let mut c = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls)?,
    ));
    c.transport_config(transport());
    Ok(c)
}
pub async fn write_bytes(s: &mut SendStream, b: &[u8], limit: usize) -> Result<()> {
    ensure!(b.len() <= limit, "Outgoing frame exceeds limit");
    s.write_all(&(b.len() as u32).to_be_bytes()).await?;
    s.write_all(b).await?;
    Ok(())
}
pub async fn read_bytes(r: &mut RecvStream, limit: usize) -> Result<Vec<u8>> {
    let mut len = [0u8; 4];
    tokio::time::timeout(Duration::from_secs(15), r.read_exact(&mut len)).await??;
    let n = u32::from_be_bytes(len) as usize;
    ensure!(n <= limit, "Incoming frame exceeds size limit");
    let mut b = vec![0u8; n];
    tokio::time::timeout(Duration::from_secs(30), r.read_exact(&mut b)).await??;
    Ok(b)
}
pub async fn send<T: Serialize>(s: &mut SendStream, v: &T) -> Result<()> {
    write_bytes(s, &serde_json::to_vec(v)?, MAX_METADATA).await
}
pub async fn recv<T: DeserializeOwned>(r: &mut RecvStream) -> Result<T> {
    Ok(serde_json::from_slice(&read_bytes(r, MAX_METADATA).await?)?)
}
fn binding(c: &Connection, client_id: &str, server_id: &str, role: u8) -> Result<Vec<u8>> {
    ensure!(
        valid_id(client_id) && valid_id(server_id),
        "Invalid handshake identity"
    );
    let mut b = vec![0u8; 64];
    c.export_keying_material(&mut b, b"EXPORTER-HarnessSync-HSQ1", b"identity-proof")
        .map_err(|_| anyhow::anyhow!("TLS channel binding unavailable"))?;
    b.extend_from_slice(client_id.as_bytes());
    b.extend_from_slice(server_id.as_bytes());
    b.push(role);
    Ok(b)
}
fn verification(c: &Connection, client_id: &str, server_id: &str) -> Result<String> {
    let h = sha256(&binding(c, client_id, server_id, 2)?);
    Ok(h.as_bytes()[..32]
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join(" "))
}
fn hello(
    e: &Engine,
    c: &Connection,
    client: &str,
    server: &str,
    role: u8,
    pairing: bool,
) -> Result<Hello> {
    Ok(Hello {
        version: VERSION,
        capabilities: vec!["synthetic-v2".into()],
        name: e.name(),
        certificate: e.identity.certificate.clone(),
        signature: e.identity.sign(&binding(c, client, server, role)?),
        pairing,
    })
}
fn validate(h: &Hello) -> Result<()> {
    ensure!(
        h.version == VERSION && h.capabilities == ["synthetic-v2"],
        "Protocol version or capability mismatch"
    );
    ensure!(
        !h.name.is_empty() && h.name.len() <= 80 && !h.name.chars().any(char::is_control),
        "Invalid device name"
    );
    identity::public_key(&h.certificate)?;
    ensure!(h.signature.len() == 64, "Invalid signature length");
    Ok(())
}
async fn approval(
    e: &Engine,
    c: &Connection,
    s: &mut SendStream,
    r: &mut RecvStream,
    peer: &Peer,
    client_id: &str,
    server_id: &str,
) -> Result<()> {
    let id = nonce()?;
    let pending = Pending {
        id: id.clone(),
        peer_id: peer.id.clone(),
        name: peer.name.clone(),
        code: verification(c, client_id, server_id)?,
        decision: None,
    };
    e.add_pending(pending)?;
    let result = async {
        let decision = async {
            for _ in 0..1200 {
                if let Some(d) = e.pending_decision(&id)? {
                    return Ok::<bool, anyhow::Error>(d);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            bail!("Pairing timed out; start again on both devices")
        };
        let send_decision = async {
            let accept = decision.await?;
            send(s, &Message::Decision { accept }).await?;
            Ok::<bool, anyhow::Error>(accept)
        };
        let receive_decision = async {
            match tokio::time::timeout(Duration::from_secs(125), recv::<Message>(r)).await?? {
                Message::Decision { accept } => Ok(accept),
                _ => bail!("Expected pairing decision"),
            }
        };
        let (local, remote) = tokio::try_join!(send_decision, receive_decision)?;
        ensure!(local && remote, "Pairing rejected; no trust added");
        e.trust(peer.clone())?;
        send(s, &Message::Ack).await?;
        ensure!(
            matches!(recv(r).await?, Message::Ack),
            "Pairing acknowledgment missing"
        );
        s.finish()?;
        tokio::time::timeout(Duration::from_secs(5), r.read_to_end(0)).await??;
        tokio::time::timeout(Duration::from_secs(5), s.stopped()).await??;
        Ok(())
    }
    .await;
    e.remove_pending(&id);
    result
}
pub async fn connect(e: Arc<Engine>, peer: Peer, pairing: bool) -> Result<()> {
    let addr: SocketAddr = peer
        .address
        .parse()
        .context("Enter a numeric LAN address and port")?;
    let c = tokio::time::timeout(
        Duration::from_secs(10),
        e.endpoint
            .connect_with(client(&peer.certificate)?, addr, "harnesssync.local")?,
    )
    .await??;
    let result = async {
        let chain = c
            .peer_identity()
            .context("TLS identity missing")?
            .downcast::<Vec<CertificateDer<'static>>>()
            .map_err(|_| anyhow::anyhow!("Unexpected TLS identity"))?;
        ensure!(
            chain.first().map(|c| c.as_ref()) == Some(peer.certificate.as_slice())
                && sha256(&peer.certificate) == peer.id,
            "Pinned TLS identity mismatch"
        );
        let (mut s, mut r) = c.open_bi().await?;
        send(
            &mut s,
            &hello(&e, &c, &e.identity.id, &peer.id, 0, pairing)?,
        )
        .await?;
        let h: Hello = recv(&mut r).await?;
        validate(&h)?;
        ensure!(
            h.certificate == peer.certificate && h.pairing == pairing,
            "Peer identity or session mode mismatch"
        );
        identity::verify(
            &h.certificate,
            &binding(&c, &e.identity.id, &peer.id, 1)?,
            &h.signature,
        )?;
        let mut peer = peer.clone();
        peer.name = h.name;
        if pairing {
            approval(&e, &c, &mut s, &mut r, &peer, &e.identity.id, &peer.id).await?;
        } else {
            e.authorize(&peer.id, &peer.certificate)?;
            e.connected(&peer.id, c.clone());
            loop {
                e.authorize(&peer.id, &peer.certificate)?;
                let receipt = exchange_client(&e, &mut s, &mut r).await?;
                e.synced(&peer.id, c.stable_id(), receipt).await?;
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    c.close(0u32.into(), b"Session ended");
    e.disconnected(&peer.id);
    result
}
pub async fn accept(e: Arc<Engine>, incoming: quinn::Incoming) -> Result<()> {
    let c = tokio::time::timeout(Duration::from_secs(10), incoming).await??;
    let mut peer_id = String::new();
    let result = async {
        let (mut s, mut r) = tokio::time::timeout(Duration::from_secs(10), c.accept_bi()).await??;
        let h: Hello = recv(&mut r).await?;
        validate(&h)?;
        peer_id = sha256(&h.certificate);
        ensure!(peer_id != e.identity.id, "Self-pairing refused");
        identity::verify(
            &h.certificate,
            &binding(&c, &peer_id, &e.identity.id, 0)?,
            &h.signature,
        )?;
        let peer = Peer {
            id: peer_id.clone(),
            name: h.name.clone(),
            certificate: h.certificate.clone(),
            address: c.remote_address().to_string(),
        };
        if h.pairing {
            ensure!(
                e.pairing_open(),
                "Click Add Device on this computer before pairing"
            );
        } else {
            e.authorize(&peer_id, &h.certificate)?;
        }
        send(
            &mut s,
            &hello(&e, &c, &peer_id, &e.identity.id, 1, h.pairing)?,
        )
        .await?;
        if h.pairing {
            approval(&e, &c, &mut s, &mut r, &peer, &peer_id, &e.identity.id).await?;
        } else {
            match recv::<Message>(&mut r).await? {
                Message::ManagedExportBegin { manifest } => {
                    e.authorize(&peer_id, &h.certificate)?;
                    let receipt = receive_managed_export(&e, &mut s, &mut r, manifest).await?;
                    send(&mut s, &receipt).await?;
                    s.finish()?;
                    tokio::time::timeout(Duration::from_secs(5), r.read_to_end(0)).await??;
                    return Ok(());
                }
                first => {
                    // Preserve the established synthetic reconciliation protocol.
                    e.connected(&peer_id, c.clone());
                    let receipt = exchange_server_with_first(&e, &mut s, &mut r, first).await?;
                    e.synced(&peer_id, c.stable_id(), receipt).await?;
                }
            }
            loop {
                e.authorize(&peer_id, &h.certificate)?;
                let receipt = exchange_server(&e, &mut s, &mut r).await?;
                e.synced(&peer_id, c.stable_id(), receipt).await?;
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    c.close(0u32.into(), b"Session ended");
    e.disconnected(&peer_id);
    if let Err(ref error) = result {
        e.peer_error(&peer_id,format!("Synchronization stopped: {error}. Keep the profile intact; check free disk space and retry."));
    }
    result
}

/// A separate authenticated QUIC connection carrying real managed data. It
/// never calls the synthetic ledger reconciliation code.
pub async fn export_managed(
    e: &Engine,
    peer: Peer,
    manifest: ExportManifest,
    payload: harness_sync_core::Payload,
) -> Result<serde_json::Value> {
    learnharness_export::verify(&manifest, &payload)?;
    e.authorize(&peer.id, &peer.certificate)?;
    let addr: SocketAddr = peer
        .address
        .parse()
        .context("Invalid paired peer address")?;
    let c = tokio::time::timeout(
        Duration::from_secs(10),
        e.endpoint
            .connect_with(client(&peer.certificate)?, addr, "harnesssync.local")?,
    )
    .await??;
    let result = async {
        let chain = c.peer_identity().context("TLS identity missing")?.downcast::<Vec<CertificateDer<'static>>>().map_err(|_| anyhow::anyhow!("Unexpected TLS identity"))?;
        ensure!(chain.first().map(|c| c.as_ref()) == Some(peer.certificate.as_slice()) && sha256(&peer.certificate) == peer.id, "Pinned TLS identity mismatch");
        let (mut s, mut r) = c.open_bi().await?;
        send(&mut s, &hello(e, &c, &e.identity.id, &peer.id, 0, false)?).await?;
        let h: Hello = recv(&mut r).await?;
        validate(&h)?;
        ensure!(h.certificate == peer.certificate && !h.pairing, "Peer identity or session mode mismatch");
        identity::verify(&h.certificate, &binding(&c, &e.identity.id, &peer.id, 1)?, &h.signature)?;
        e.authorize(&peer.id, &peer.certificate)?;
        send(&mut s, &Message::ManagedExportBegin { manifest: manifest.clone() }).await?;
        ensure!(matches!(recv::<Message>(&mut r).await?, Message::ManagedExportReady { revision } if revision == manifest.revision), "Peer rejected or mismatched the export manifest");
        for record in &manifest.files {
            let bytes = payload.get(&record.path).context("Verified export content missing")?;
            send(&mut s, &Message::ManagedExportFile { path: record.path.clone(), hash: record.hash.clone() }).await?;
            write_bytes(&mut s, bytes, MAX_FILE).await?;
        }
        send(&mut s, &Message::ManagedExportEnd { revision: manifest.revision.clone() }).await?;
        let reply: Message = recv(&mut r).await?;
        match reply {
            Message::ManagedExportComplete { revision, digest, staged } => {
                ensure!(revision == manifest.revision && digest == manifest.revision && staged, "Peer did not confirm quarantined content verification");
                Ok(serde_json::json!({"state":"transferred_and_staged","revision":revision,"files":manifest.files.len(),"bytes":manifest.files.iter().map(|f|f.size).sum::<u64>(),"applied":false}))
            }
            _ => bail!("Peer returned no verified quarantine receipt"),
        }
    }.await;
    c.close(0u32.into(), b"Managed export finished");
    result
}

async fn receive_managed_export(
    e: &Engine,
    s: &mut SendStream,
    r: &mut RecvStream,
    manifest: ExportManifest,
) -> Result<Message> {
    manifest.validate()?;
    send(
        s,
        &Message::ManagedExportReady {
            revision: manifest.revision.clone(),
        },
    )
    .await?;
    let mut payload = harness_sync_core::Payload::new();
    let mut total = 0usize;
    for record in &manifest.files {
        match recv::<Message>(r).await? {
            Message::ManagedExportFile { path, hash } => ensure!(
                path == record.path && hash == record.hash,
                "Incoming record differs from validated inventory"
            ),
            _ => bail!("Unexpected managed export record"),
        }
        let bytes = read_bytes(r, MAX_FILE).await?;
        total = total
            .checked_add(bytes.len())
            .context("Managed export size overflow")?;
        ensure!(
            total <= harness_sync_core::MAX_TOTAL,
            "Managed export exceeds total limit"
        );
        ensure!(
            bytes.len() as u64 == record.size && sha256(&bytes) == record.hash,
            "Managed export record integrity failure"
        );
        payload.insert(record.path.clone(), bytes);
    }
    ensure!(
        matches!(recv::<Message>(r).await?, Message::ManagedExportEnd { revision } if revision == manifest.revision),
        "Managed export ended unexpectedly"
    );
    let revision = manifest.revision.clone();
    e.disk(move |store| learnharness_export::stage(&store.root, &manifest, &payload))
        .await?;
    Ok(Message::ManagedExportComplete {
        revision: revision.clone(),
        digest: revision,
        staged: true,
    })
}
async fn head(e: &Engine) -> Result<Option<String>> {
    e.disk(|s| Ok(s.ledger()?.head)).await
}
async fn pull(e: &Engine, s: &mut SendStream, r: &mut RecvStream, head: &str) -> Result<()> {
    ensure!(valid_id(head), "Invalid remote head");
    let mut pending = vec![(head.to_string(), None::<Manifest>)];
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    while let Some((id, ready)) = pending.pop() {
        if let Some(m) = ready {
            for f in &m.files {
                let object_id = f.hash.clone();
                if e.disk(move |s| Ok(s.has_object(&object_id))).await? {
                    continue;
                }
                send(s, &Message::Blob { id: f.hash.clone() }).await?;
                let b = read_bytes(r, MAX_FILE).await?;
                total = total
                    .checked_add(b.len())
                    .context("Transfer size overflow")?;
                ensure!(
                    total <= 256 * 1024 * 1024,
                    "Transfer exceeds session budget"
                );
                ensure!(
                    b.len() as u64 == f.size && sha256(&b) == f.hash,
                    "Received object integrity failure"
                );
                let hash = f.hash.clone();
                let received = b.len();
                e.disk(move |s| s.put_object(&hash, &b)).await?;
                e.received_verified(received);
            }
            e.disk(move |s| s.commit_manifest(&m).map(|_| ())).await?;
            continue;
        }
        if !seen.insert(id.clone()) {
            bail!("Repeated or cyclic snapshot ancestry");
        }
        ensure!(seen.len() <= MAX_HISTORY, "Remote history exceeds limit");
        let check = id.clone();
        if e.disk(move |s| Ok(s.history(&check).is_ok())).await? {
            continue;
        }
        send(s, &Message::Manifest { id: id.clone() }).await?;
        let b = read_bytes(r, MAX_METADATA).await?;
        let m: Manifest = serde_json::from_slice(&b)?;
        m.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            m.id() == id && m.bytes() == b,
            "Manifest hash/canonical encoding mismatch"
        );
        let parents = m.parents.clone();
        pending.push((id, Some(m)));
        for p in parents {
            pending.push((p, None));
        }
    }
    let id = head.to_string();
    e.disk(move |s| {
        s.history(&id)?;
        Ok(())
    })
    .await
}
async fn serve(
    e: &Engine,
    s: &mut SendStream,
    r: &mut RecvStream,
    head: Option<String>,
) -> Result<()> {
    // A trusted peer may read only objects reachable from the advertised head;
    // local backup branches and unadvertised history are not arbitrary file APIs.
    let allowed = e
        .disk(move |store| {
            let mut manifests = BTreeSet::new();
            let mut blobs = BTreeSet::new();
            if let Some(head) = head {
                for id in store.history(&head)?.keys() {
                    manifests.insert(id.clone());
                    for f in store.manifest(id)?.files {
                        blobs.insert(f.hash);
                    }
                }
            }
            Ok((manifests, blobs))
        })
        .await?;
    let mut served_bytes = 0usize;
    for _ in 0..MAX_HISTORY * 5 {
        match recv(r).await? {
            Message::Manifest { id } => {
                ensure!(
                    allowed.0.contains(&id),
                    "Manifest outside advertised history"
                );
                let b = e
                    .disk(move |store| Ok(store.manifest(&id)?.bytes()))
                    .await?;
                write_bytes(s, &b, MAX_METADATA).await?;
            }
            Message::Blob { id } => {
                ensure!(allowed.1.contains(&id), "Object outside advertised history");
                let b = e.disk(move |store| store.object(&id)).await?;
                served_bytes = served_bytes
                    .checked_add(b.len())
                    .context("Transfer size overflow")?;
                ensure!(
                    served_bytes <= 256 * 1024 * 1024,
                    "Outbound exchange budget exceeded"
                );
                write_bytes(s, &b, MAX_FILE).await?;
            }
            Message::End => return Ok(()),
            _ => bail!("Unexpected transfer message"),
        }
    }
    bail!("Request budget exceeded")
}
async fn exchange_client(e: &Engine, s: &mut SendStream, r: &mut RecvStream) -> Result<Receipt> {
    let local = head(e).await?;
    send(
        s,
        &Message::Head {
            head: local.clone(),
        },
    )
    .await?;
    let remote = match recv(r).await? {
        Message::Head { head } => head,
        _ => bail!("Expected peer head"),
    };
    if let Some(ref h) = remote {
        pull(e, s, r, h).await?;
    }
    send(s, &Message::End).await?;
    serve(e, s, r, local).await?;
    let receipt = commit(e, remote).await?;
    send(
        s,
        &Message::Committed {
            head: receipt.head,
            received: receipt.received,
            conflict: receipt.conflict,
        },
    )
    .await?;
    receive_receipt(r).await
}

async fn exchange_server(e: &Engine, s: &mut SendStream, r: &mut RecvStream) -> Result<Receipt> {
    let first = recv(r).await?;
    exchange_server_with_first(e, s, r, first).await
}
async fn exchange_server_with_first(
    e: &Engine,
    s: &mut SendStream,
    r: &mut RecvStream,
    first: Message,
) -> Result<Receipt> {
    let remote = match first {
        Message::Head { head } => head,
        _ => bail!("Expected peer head"),
    };
    let local = head(e).await?;
    send(
        s,
        &Message::Head {
            head: local.clone(),
        },
    )
    .await?;
    serve(e, s, r, local).await?;
    if let Some(ref h) = remote {
        pull(e, s, r, h).await?;
    }
    send(s, &Message::End).await?;
    let receipt = commit(e, remote).await?;
    let peer = receive_receipt(r).await?;
    send(
        s,
        &Message::Committed {
            head: receipt.head,
            received: receipt.received,
            conflict: receipt.conflict,
        },
    )
    .await?;
    Ok(peer)
}

/// Sent only after object verification and durable reconciliation, over the
/// authenticated stream. Connectivity alone never creates this evidence.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub head: Option<String>,
    pub received: Option<String>,
    pub conflict: bool,
}
async fn commit(e: &Engine, remote: Option<String>) -> Result<Receipt> {
    e.disk(move |store| {
        if let Some(ref id) = remote {
            // The profile marker guarantees synthetic-only storage. Preparing a
            // receiving fixture needs no separate user setup or real-data access.
            if !store.ledger()?.initialized {
                store.initialize()?;
            }
            store.reconcile(id)?;
        }
        let ledger = store.ledger()?;
        Ok(Receipt {
            head: ledger.head,
            received: remote,
            conflict: !ledger.conflicts.is_empty(),
        })
    })
    .await
}
async fn receive_receipt(r: &mut RecvStream) -> Result<Receipt> {
    match recv(r).await? {
        Message::Committed {
            head,
            received,
            conflict,
        } => {
            ensure!(
                head.iter().chain(received.iter()).all(|id| valid_id(id)),
                "Invalid committed head acknowledgment"
            );
            Ok(Receipt {
                head,
                received,
                conflict,
            })
        }
        _ => bail!("Missing verified head acknowledgment; update both computers"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{adapter, engine::Peer, storage::Store};
    use tempfile::TempDir;
    async fn local(t: &TempDir) -> Arc<Engine> {
        let e = Engine::start(
            t.path().join("local"),
            "Local".into(),
            "127.0.0.1:0".parse().unwrap(),
            false,
        )
        .await
        .unwrap();
        e.request(
            "initialize_synthetic",
            serde_json::json!({"confirmed":true}),
        )
        .await
        .unwrap();
        e
    }
    async fn raw(e: &Engine, remote: &Engine) -> (Connection, SendStream, RecvStream) {
        let c = e
            .endpoint
            .connect_with(
                client(&remote.identity.certificate).unwrap(),
                remote.local_addr().unwrap(),
                "harnesssync.local",
            )
            .unwrap()
            .await
            .unwrap();
        let (s, r) = c.open_bi().await.unwrap();
        (c, s, r)
    }

    fn minimal_managed_payload() -> harness_sync_core::Payload {
        let chat = serde_json::json!({
            "version":2,"id":"chat_abc123_1234abcd","title":"Disposable",
            "createdAt":"2026-11-24T10:00:00Z","updatedAt":"2026-11-24T10:00:00Z",
            "messages":[{"role":"user","id":"msg_abc123_1234abcd","content":"test","timestamp":1}]
        });
        [(
            "chats/chat_abc123_1234abcd.json".into(),
            serde_json::to_vec(&chat).unwrap(),
        )]
        .into_iter()
        .collect()
    }

    async fn managed_handshake(a: &Engine, b: &Engine) -> (Connection, SendStream, RecvStream) {
        a.trust(Peer {
            id: b.identity.id.clone(),
            name: b.name(),
            certificate: b.identity.certificate.clone(),
            address: b.local_addr().unwrap().to_string(),
        })
        .unwrap();
        b.trust(Peer {
            id: a.identity.id.clone(),
            name: a.name(),
            certificate: a.identity.certificate.clone(),
            address: a.local_addr().unwrap().to_string(),
        })
        .unwrap();
        let (c, mut s, mut r) = raw(a, b).await;
        send(
            &mut s,
            &hello(a, &c, &a.identity.id, &b.identity.id, 0, false).unwrap(),
        )
        .await
        .unwrap();
        let response: Hello = recv(&mut r).await.unwrap();
        assert_eq!(response.certificate, b.identity.certificate);
        (c, s, r)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn malformed_and_interrupted_managed_exports_never_reach_quarantine() {
        let t = crate::storage::test_tempdir();
        let a = local(&t).await;
        let b = Engine::start(
            t.path().join("receiver"),
            "Receiver".into(),
            "127.0.0.1:0".parse().unwrap(),
            false,
        )
        .await
        .unwrap();
        let payload = minimal_managed_payload();
        let good = ExportManifest::from_payload(&payload).unwrap();

        let (c, mut s, mut r) = managed_handshake(&a, &b).await;
        let mut invalid = good.clone();
        invalid.format = "unknown-schema".into();
        send(&mut s, &Message::ManagedExportBegin { manifest: invalid })
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), recv::<Message>(&mut r))
                .await
                .unwrap()
                .is_err()
        );
        c.close(0u32.into(), b"invalid manifest test");
        assert!(!t.path().join("receiver/quarantine").exists());

        let (c, mut s, mut r) = managed_handshake(&a, &b).await;
        send(
            &mut s,
            &Message::ManagedExportBegin {
                manifest: good.clone(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            recv::<Message>(&mut r).await.unwrap(),
            Message::ManagedExportReady { .. }
        ));
        let first = &good.files[0];
        send(
            &mut s,
            &Message::ManagedExportFile {
                path: first.path.clone(),
                hash: first.hash.clone(),
            },
        )
        .await
        .unwrap();
        s.write_all(&(first.size as u32).to_be_bytes())
            .await
            .unwrap();
        s.write_all(b"partial").await.unwrap();
        c.close(0u32.into(), b"interrupted transfer test");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!t.path().join("receiver/quarantine").exists());
        a.shutdown();
        b.shutdown();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn protocol_version_and_message_size_limits() {
        let t = crate::storage::test_tempdir();
        let a = local(&t).await;
        let b = Engine::start(
            t.path().join("remote"),
            "Remote".into(),
            "127.0.0.1:0".parse().unwrap(),
            false,
        )
        .await
        .unwrap();
        let (c, mut s, mut r) = raw(&a, &b).await;
        let mut h = hello(&a, &c, &a.identity.id, &b.identity.id, 0, true).unwrap();
        h.version = 99;
        send(&mut s, &h).await.unwrap();
        assert!(recv::<Hello>(&mut r).await.is_err());
        c.close(0u32.into(), b"test done");
        let (c, mut s, mut r) = raw(&a, &b).await;
        s.write_all(&((MAX_METADATA + 1) as u32).to_be_bytes())
            .await
            .unwrap();
        assert!(recv::<Hello>(&mut r).await.is_err());
        assert!(a.status().await.unwrap()["transport"]["peers"]
            .as_array()
            .unwrap()
            .is_empty());
        c.close(0u32.into(), b"test done");
        a.shutdown();
        b.shutdown();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn replayed_proof_is_invalid_on_new_tls_session() {
        let t = crate::storage::test_tempdir();
        let a = local(&t).await;
        let b = Engine::start(
            t.path().join("remote"),
            "Remote".into(),
            "127.0.0.1:0".parse().unwrap(),
            false,
        )
        .await
        .unwrap();
        let (first, _, _) = raw(&a, &b).await;
        let proof = hello(&a, &first, &a.identity.id, &b.identity.id, 0, true).unwrap();
        first.close(0u32.into(), b"new session");
        let (second, mut s, mut r) = raw(&a, &b).await;
        send(&mut s, &proof).await.unwrap();
        assert!(recv::<Hello>(&mut r).await.is_err());
        second.close(0u32.into(), b"done");
        a.shutdown();
        b.shutdown();
    }
    async fn malicious_response(kind: &'static str) {
        let t = crate::storage::test_tempdir();
        let a = local(&t).await;
        let old = a
            .request(
                "synthetic_edit",
                serde_json::json!({"text":"Invented local history"}),
            )
            .await
            .unwrap()["snapshot"]
            .as_str()
            .unwrap()
            .to_string();
        let store = Store::open(t.path().join("malicious")).unwrap();
        let identity = identity::Identity::load(&store.root).unwrap();
        let endpoint = quinn::Endpoint::server(
            server(&identity, &store.root).unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let p = Peer {
            id: identity.id.clone(),
            name: "Test remote".into(),
            certificate: identity.certificate.clone(),
            address: endpoint.local_addr().unwrap().to_string(),
        };
        a.trust(p.clone()).unwrap();
        let m = Manifest::from_payload(
            identity.id.clone(),
            vec![],
            &adapter::fixture("Invented remote payload").unwrap(),
        )
        .unwrap();
        let remote_id = m.id();
        let remote = tokio::spawn(async move {
            let c = endpoint.accept().await.unwrap().await.unwrap();
            let (mut s, mut r) = c.accept_bi().await.unwrap();
            let h: Hello = recv(&mut r).await.unwrap();
            let client_id = sha256(&h.certificate);
            let reply = Hello {
                version: VERSION,
                capabilities: vec!["synthetic-v2".into()],
                name: "Test remote".into(),
                certificate: identity.certificate.clone(),
                signature: identity.sign(&binding(&c, &client_id, &identity.id, 1).unwrap()),
                pairing: false,
            };
            send(&mut s, &reply).await.unwrap();
            assert!(matches!(recv(&mut r).await.unwrap(), Message::Head { .. }));
            send(
                &mut s,
                &Message::Head {
                    head: Some(if kind == "invalid-id" {
                        "../ledger.json".into()
                    } else {
                        m.id()
                    }),
                },
            )
            .await
            .unwrap();
            if kind != "invalid-id" {
                assert!(matches!(
                    recv(&mut r).await.unwrap(),
                    Message::Manifest { .. }
                ));
                write_bytes(&mut s, &m.bytes(), MAX_METADATA).await.unwrap();
                assert!(matches!(recv(&mut r).await.unwrap(), Message::Blob { .. }));
                if kind == "corrupt" {
                    write_bytes(&mut s, b"corrupted", MAX_FILE).await.unwrap();
                } else {
                    s.write_all(&(100u32).to_be_bytes()).await.unwrap();
                    s.write_all(b"partial").await.unwrap();
                    s.finish().unwrap();
                }
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), c.closed()).await;
        });
        assert!(connect(a.clone(), p, false).await.is_err());
        remote.await.unwrap();
        assert_eq!(a.status().await.unwrap()["head"], old);
        a.disk(move |s| {
            ensure!(
                s.manifest(&remote_id).is_err(),
                "Unverified remote manifest was committed"
            );
            s.history(&old)?;
            Ok(())
        })
        .await
        .unwrap();
        a.shutdown();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn corrupt_payload_never_commits() {
        malicious_response("corrupt").await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn interrupted_payload_never_commits() {
        malicious_response("interrupted").await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn invalid_remote_object_id_is_refused() {
        malicious_response("invalid-id").await;
    }
}
