//! A fixed, unprivileged launcher. No IPC argument supplies a path or shell program.
use std::process::Command;

pub fn run(operation: &str) -> Result<String, String> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err("Managed sessions require Linux x86_64.".into());
    }
    if !matches!(operation, "status" | "launch" | "recover") {
        return Err("Unsupported managed operation.".into());
    }
    let mut cmd = Command::new("/usr/bin/bash");
    cmd.env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C.UTF-8");
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
        s @ ("new" | "active" | "recovery" | "saved" | "recovered") => Ok(s.to_owned()),
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
}
