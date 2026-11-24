use crate::storage::{atomic, read};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use harness_sync_core::sha256;
use ring::signature::{self, Ed25519KeyPair, KeyPair as _};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    certificate: String,
    private_key: String,
}
pub struct Identity {
    pub certificate: Vec<u8>,
    key: Ed25519KeyPair,
    pub id: String,
}
impl Identity {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("identity.json");
        if !path.exists() {
            let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)?;
            let params = rcgen::CertificateParams::new(vec!["harnesssync.local".into()])?;
            let cert = params.self_signed(&key)?.der().to_vec();
            atomic(
                &path,
                &serde_json::to_vec(&Saved {
                    version: 1,
                    certificate: STANDARD.encode(cert),
                    private_key: STANDARD.encode(key.serialize_der()),
                })?,
            )?;
        }
        let saved: Saved = serde_json::from_slice(&read(&path, 16384)?)?;
        ensure!(saved.version == 1, "Unknown identity version");
        let certificate = STANDARD.decode(saved.certificate)?;
        let key = Ed25519KeyPair::from_pkcs8(&STANDARD.decode(saved.private_key)?)
            .map_err(|_| anyhow::anyhow!("Invalid identity key"))?;
        ensure!(
            public_key(&certificate)? == key.public_key().as_ref(),
            "Identity certificate/key mismatch"
        );
        Ok(Self {
            id: sha256(&certificate),
            certificate,
            key,
        })
    }
    pub fn sign(&self, bytes: &[u8]) -> Vec<u8> {
        self.key.sign(bytes).as_ref().to_vec()
    }
    pub fn private_der(&self, root: &Path) -> Result<Vec<u8>> {
        let saved: Saved = serde_json::from_slice(&read(&root.join("identity.json"), 16384)?)?;
        Ok(STANDARD.decode(saved.private_key)?)
    }
}
pub fn public_key(cert: &[u8]) -> Result<Vec<u8>> {
    ensure!(cert.len() <= 4096, "Certificate exceeds limit");
    let (rest, c) = x509_parser::parse_x509_certificate(cert)
        .map_err(|_| anyhow::anyhow!("Invalid peer certificate"))?;
    ensure!(
        rest.is_empty() && c.public_key().algorithm.algorithm.to_id_string() == "1.3.101.112",
        "Expected Ed25519 peer identity"
    );
    let p = c.public_key().subject_public_key.data.to_vec();
    ensure!(p.len() == 32, "Invalid peer key");
    Ok(p)
}
pub fn verify(cert: &[u8], bytes: &[u8], sig: &[u8]) -> Result<()> {
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key(cert)?)
        .verify(bytes, sig)
        .map_err(|_| anyhow::anyhow!("Peer identity signature failed"))
        .context("Authenticated handshake refused")
}
