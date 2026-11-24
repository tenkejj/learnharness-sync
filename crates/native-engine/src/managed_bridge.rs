//! Capability-based IPC for a managed-profile helper.
//!
//! A connected Unix socket is passed to the child as an inherited file
//! descriptor. There is no pathname in the vault, and peer authentication is
//! the unforgeable descriptor handoff by the spawning coordinator.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

const VERSION: u32 = 1;
const MAX_FRAME: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Status,
    Capture { consent: bool },
    ValidateIncoming { head: String },
    PrepareApply { head: String },
    CommitApply { head: String },
    ConfirmReadback { head: String },
    Recover,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    id: String,
    operation: Operation,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    version: u32,
    id: String,
    result: Value,
}

pub struct Client(UnixStream);
pub struct Server(UnixStream);

/// Creates an unforgeable, unnamed local channel. The caller must pass the
/// server endpoint only to the isolated helper and retain the client endpoint.
pub fn pair() -> Result<(Client, Server)> {
    let (client, server) = UnixStream::pair().context("Could not create managed IPC channel")?;
    Ok((Client(client), Server(server)))
}

impl Client {
    pub fn request(&mut self, operation: Operation) -> Result<Value> {
        if matches!(operation, Operation::Capture { consent: false }) {
            anyhow::bail!("Explicit consent is required before exporting conversations");
        }
        let id = crate::storage::nonce()?;
        let request = Request {
            version: VERSION,
            id: id.clone(),
            operation,
        };
        write_frame(&mut self.0, &serde_json::to_vec(&request)?)?;
        let response: Response = serde_json::from_slice(&read_frame(&mut self.0)?)?;
        ensure!(
            response.version == VERSION && response.id == id,
            "Managed helper response did not match this request"
        );
        Ok(response.result)
    }
}

impl Server {
    /// Handles one request then closes the protocol. A fresh capability is
    /// created for every managed operation so a stale helper cannot replay it.
    pub fn serve_once<F>(mut self, operation: F) -> Result<()>
    where
        F: FnOnce(Operation) -> Result<Value>,
    {
        let request: Request = serde_json::from_slice(&read_frame(&mut self.0)?)?;
        ensure!(
            request.version == VERSION
                && request.id.len() == 64
                && request.id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid managed IPC request"
        );
        if matches!(request.operation, Operation::Capture { consent: false }) {
            anyhow::bail!("Explicit consent is required before exporting conversations");
        }
        let result = operation(request.operation)?;
        write_frame(
            &mut self.0,
            &serde_json::to_vec(&Response {
                version: VERSION,
                id: request.id,
                result,
            })?,
        )
    }
}

fn write_frame(stream: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_FRAME,
        "Managed IPC frame exceeds limit"
    );
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()?;
    Ok(())
}

fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    ensure!(
        length > 0 && length <= MAX_FRAME,
        "Invalid managed IPC frame length"
    );
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn one_use_capability_roundtrips_and_requires_consent() {
        let (mut client, server) = pair().unwrap();
        let worker = std::thread::spawn(move || {
            server.serve_once(|request| {
                assert_eq!(request, Operation::Capture { consent: true });
                Ok(json!({"snapshot":"verified"}))
            })
        });
        assert!(client
            .request(Operation::Capture { consent: false })
            .is_err());
        assert_eq!(
            client
                .request(Operation::Capture { consent: true })
                .unwrap(),
            json!({"snapshot":"verified"})
        );
        worker.join().unwrap().unwrap();
    }

    #[test]
    fn rejects_oversized_or_mismatched_protocol_frames() {
        let (mut client, server) = pair().unwrap();
        let worker = std::thread::spawn(move || server.serve_once(|_| Ok(Value::Null)));
        client.0.write_all(&u32::MAX.to_be_bytes()).unwrap();
        assert!(worker.join().unwrap().is_err());
    }
}
