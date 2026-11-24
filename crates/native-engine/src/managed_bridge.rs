//! Bounded one-use IPC for a managed-profile helper.
//!
//! Unix sockets support in-process local clients; the production supervisor
//! uses anonymous stdio pipes inherited by the helper as a one-use capability.
//! Neither transport creates a pathname in the vault or exposes device keys.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const VERSION: u32 = 1;
// A full supported LearnHarness profile can be 128 MiB. The helper returns
// bounded base64 records in one one-use pipe response; cap the JSON expansion
// above that contract and reject larger profiles before they leave the vault.
const MAX_FRAME: usize = 180 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Status,
    Snapshot,
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
pub struct StdioClient<R, W> {
    reader: R,
    writer: W,
}

/// Creates an unforgeable, unnamed local channel. The caller must pass the
/// server endpoint only to the isolated helper and retain the client endpoint.
pub fn pair() -> Result<(Client, Server)> {
    let (client, server) = UnixStream::pair().context("Could not create managed IPC channel")?;
    client.set_read_timeout(Some(Duration::from_secs(20)))?;
    client.set_write_timeout(Some(Duration::from_secs(20)))?;
    server.set_read_timeout(Some(Duration::from_secs(20)))?;
    server.set_write_timeout(Some(Duration::from_secs(20)))?;
    Ok((Client(client), Server(server)))
}

/// Take ownership of the fixed inherited socket descriptor used by the
/// supervisor. It must refer to a connected AF_UNIX stream socket.
///
/// # Safety
/// Descriptor 3 must be open, exclusively handed to this process, and not used
/// after this function takes ownership.
pub unsafe fn server_from_fd(fd: i32) -> Result<Server> {
    use std::os::fd::FromRawFd;
    ensure!(
        fd == 3,
        "Managed helper accepts only its fixed IPC descriptor"
    );
    let mut kind: libc::c_int = 0;
    let mut length = std::mem::size_of_val(&kind) as libc::socklen_t;
    ensure!(
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            &mut kind as *mut _ as *mut _,
            &mut length
        ) == 0
            && kind == libc::SOCK_STREAM,
        "Managed helper received an invalid IPC descriptor"
    );
    let stream = UnixStream::from_raw_fd(fd);
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    Ok(Server(stream))
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

impl<R: Read, W: Write> StdioClient<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    pub fn request(&mut self, operation: Operation) -> Result<Value> {
        if matches!(operation, Operation::Capture { consent: false }) {
            anyhow::bail!("Explicit consent is required before exporting conversations");
        }
        let id = crate::storage::nonce()?;
        write_frame(
            &mut self.writer,
            &serde_json::to_vec(&Request {
                version: VERSION,
                id: id.clone(),
                operation,
            })?,
        )?;
        let response: Response = serde_json::from_slice(&read_frame(&mut self.reader)?)?;
        ensure!(
            response.version == VERSION && response.id == id,
            "Managed helper response did not match this request"
        );
        Ok(response.result)
    }
}

impl Server {
    pub fn into_raw_fd(self) -> std::os::fd::RawFd {
        use std::os::fd::IntoRawFd;
        self.0.into_raw_fd()
    }

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

    pub fn serve_stdio_once<R: Read, W: Write, F>(
        reader: &mut R,
        writer: &mut W,
        operation: F,
    ) -> Result<()>
    where
        F: FnOnce(Operation) -> Result<Value>,
    {
        serve_one(reader, writer, operation)
    }
}

fn serve_one<R: Read, W: Write, F>(reader: &mut R, writer: &mut W, operation: F) -> Result<()>
where
    F: FnOnce(Operation) -> Result<Value>,
{
    let request: Request = serde_json::from_slice(&read_frame(reader)?)?;
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
        writer,
        &serde_json::to_vec(&Response {
            version: VERSION,
            id: request.id,
            result,
        })?,
    )
}

fn write_frame<W: Write>(stream: &mut W, bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_FRAME,
        "Managed IPC frame exceeds limit"
    );
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()?;
    Ok(())
}

fn read_frame<R: Read>(stream: &mut R) -> Result<Vec<u8>> {
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
