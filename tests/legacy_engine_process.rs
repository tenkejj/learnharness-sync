//! Dependency-free, bounded native sidecar RPC transport. Testable with rustc.
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub const MAX_REQUEST: usize = 32768;
pub const MAX_RESPONSE: usize = 2 * 1024 * 1024;
pub struct Response {
    pub success: bool,
    pub stdout: Vec<u8>,
}

fn terminate_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // The child is spawned into a fresh private process group. PyInstaller's
        // onefile bootloader and Python child must BOTH be stopped on timeout.
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        unsafe {
            kill(-(child.id() as i32), 9);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn request(
    executable: &Path,
    profile: &Path,
    input: &[u8],
    timeout: Duration,
) -> Result<Response, String> {
    if input.len() > MAX_REQUEST || timeout.is_zero() {
        return Err("Command exceeds native request limits".into());
    }
    let mut cmd = Command::new(executable);
    cmd.arg("rpc")
        .arg("--profile")
        .arg(profile)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    #[cfg(windows)]
    return Err(
        "Windows process-tree containment has not been verified; native commands are disabled."
            .into(),
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|_| "Bundled engine unavailable. Reinstall a verified package.")?;
    let mut stdin = child.stdin.take().ok_or("Engine input unavailable")?;
    let stdout = child.stdout.take().ok_or("Engine output unavailable")?;
    let bytes = input.to_vec();
    thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut result = Vec::new();
        let read = stdout
            .take((MAX_RESPONSE + 1) as u64)
            .read_to_end(&mut result);
        let _ = sender.send(read.map(|_| result));
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                terminate_tree(&mut child);
                return Err("Engine timed out or stopped unexpectedly. Operation outcome may be unknown; verify local status before retrying.".into());
            }
        }
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    let stdout = match receiver.recv_timeout(remaining) {
        Ok(Ok(bytes)) => bytes,
        _ => {
            terminate_tree(&mut child);
            return Err("Engine output did not close. Operation outcome may be unknown; verify local status before retrying.".into());
        }
    };
    if stdout.len() > MAX_RESPONSE {
        return Err("Engine response exceeds limit. Verify local status before retrying.".into());
    }
    Ok(Response {
        success: status.success(),
        stdout,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn script(body: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "harness-rpc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let file = root.join("engine");
        fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
        file
    }
    #[test]
    fn real_child_stdin_stdout_and_status() {
        let file = script("cat >/dev/null; printf '{\"result\":true}'");
        let response = request(
            &file,
            Path::new("/unused-test-profile"),
            b"{}",
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(response.success);
        assert_eq!(response.stdout, b"{\"result\":true}");
        fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
    #[test]
    fn hung_engine_is_bounded() {
        let file = script("exec sleep 5");
        let start = Instant::now();
        let result = request(
            &file,
            Path::new("/unused-test-profile"),
            b"{}",
            Duration::from_millis(100),
        );
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
    #[test]
    fn oversized_input_is_rejected_before_launch() {
        let result = request(
            Path::new("/must-not-launch"),
            Path::new("/unused-test-profile"),
            &vec![0; MAX_REQUEST + 1],
            Duration::from_secs(1),
        );
        assert!(result.is_err());
    }
    #[test]
    fn oversized_output_is_rejected() {
        let file = script("exec head -c 2097153 /dev/zero");
        let result = request(
            &file,
            Path::new("/unused-test-profile"),
            b"{}",
            Duration::from_secs(2),
        );
        assert!(result.is_err());
        fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn timeout_stops_bootloader_style_parent_and_worker() {
        let file = script("(sleep 0.4; echo should-not-exist > \"$0.marker\") & wait");
        let result = request(
            &file,
            Path::new("/unused-test-profile"),
            b"{}",
            Duration::from_millis(100),
        );
        assert!(result.is_err());
        thread::sleep(Duration::from_millis(600));
        let marker = std::path::PathBuf::from(format!("{}.marker", file.display()));
        assert!(
            !marker.exists(),
            "A timed-out engine worker continued mutating state"
        );
        let positive = request(
            &file,
            Path::new("/unused-test-profile"),
            b"{}",
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(positive.success);
        assert!(
            marker.exists(),
            "Positive control did not execute the worker"
        );
        fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn nonzero_status_is_not_success() {
        let file = script("printf '{\"error\":\"Safety refusal\"}';exit 7");
        let response = request(
            &file,
            Path::new("/unused-test-profile"),
            b"{}",
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(!response.success);
        fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
}
