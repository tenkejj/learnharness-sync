//! Untrusted LAN hints only. No discovery record grants trust.
use crate::engine::{Engine, Peer};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use harness_sync_core::sha256;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
pub const SERVICE: &str = "_harnesssync._udp.local.";
pub fn start(engine: Arc<Engine>) -> Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    let encoded = STANDARD.encode(&engine.identity.certificate);
    let mut txt = HashMap::from([
        ("v".into(), "1".into()),
        ("id".into(), engine.identity.id.clone()),
        ("name".into(), engine.name()),
    ]);
    for (i, chunk) in encoded.as_bytes().chunks(180).enumerate() {
        txt.insert(format!("c{i}"), std::str::from_utf8(chunk)?.to_owned());
    }
    let short = &engine.identity.id[..16];
    let info = ServiceInfo::new(
        SERVICE,
        &format!("LearnHarnessSync-{short}"),
        &format!("hs-{short}.local."),
        "",
        engine.local_addr()?.port(),
        txt,
    )?
    .enable_addr_auto();
    daemon.register(info)?;
    let rx = daemon.browse(SERVICE)?;
    tokio::spawn(async move {
        while let Ok(event) = rx.recv_async().await {
            if let ServiceEvent::ServiceResolved(service) = event {
                if service.get_property_val_str("v") != Some("1") {
                    continue;
                }
                let name = service
                    .get_property_val_str("name")
                    .unwrap_or("")
                    .to_string();
                let mut cert = String::new();
                for i in 0..32 {
                    match service.get_property_val_str(&format!("c{i}")) {
                        Some(p) if p.len() <= 180 => cert.push_str(p),
                        _ => break,
                    }
                }
                let Ok(cert) = STANDARD.decode(cert) else {
                    continue;
                };
                if service.get_property_val_str("id") != Some(sha256(&cert).as_str()) {
                    continue;
                }
                if let Some(ip) = service
                    .get_addresses_v4()
                    .into_iter()
                    .filter(|ip| !ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast())
                    .min()
                {
                    engine.discovered(Peer {
                        id: sha256(&cert),
                        name,
                        certificate: cert,
                        address: SocketAddr::new(IpAddr::V4(ip), service.get_port()).to_string(),
                    });
                }
            }
        }
    });
    Ok(daemon)
}
