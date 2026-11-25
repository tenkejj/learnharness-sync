"""Exercise the packaged launcher on a private Wayland display and disposable Btrfs home."""
import argparse
import errno
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time


def until(probe, seconds=120):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = probe()
        if value:
            return value
        time.sleep(.2)
    raise RuntimeError(f"Timed out waiting for {probe.__name__}")


def descendants(pid):
    try:
        children = Path(f"/proc/{pid}/task/{pid}/children").read_text().split()
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        return
    for child in map(int, children):
        yield child
        yield from descendants(child)


def find_child(process, name):
    for pid in descendants(process.pid):
        try:
            if Path(f"/proc/{pid}/comm").read_text().strip() == name:
                return pid
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass
    if process.poll() is not None:
        raise RuntimeError("Managed launcher exited before the window appeared")


def find_descendant_command(process, marker):
    for pid in descendants(process.pid):
        try:
            command = Path(f"/proc/{pid}/cmdline").read_bytes().decode(errors="ignore")
            comm = Path(f"/proc/{pid}/comm").read_text().strip()
            if comm == "helper" and marker in command:
                return pid
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass


def exercise(package, image, weston_root, host_display=False, release="0.6.1"):
    repo = Path(__file__).resolve().parents[1]
    root = Path(tempfile.mkdtemp(prefix="managed-accept-", dir=repo/"build"))
    home, data = (root/p for p in ("home", "data"))
    runtime = Path(os.environ["XDG_RUNTIME_DIR"]) if host_display else root/"runtime"
    for folder in (home, data, *([] if host_display else [runtime])):
        folder.mkdir(mode=0o700)
    (home/"Applications").mkdir()
    installed = home/f"Applications/LearnHarness-{release}-x86_64.AppImage"
    shutil.copy2(image, installed)
    installed.chmod(0o700)
    env = {"HOME": str(home), "XDG_DATA_HOME": str(data), "XDG_RUNTIME_DIR": str(runtime),
           "PATH": "/usr/bin", "LANG": "C.UTF-8"}
    if host_display:
        for key in ("WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY"):
            if os.environ.get(key):
                env[key] = os.environ[key]
    else:
        env["WAYLAND_DISPLAY"] = "managed-test"
    env["LEARNHARNESS_SYNC_MANAGED_QA_LOG"] = "1"
    subprocess.run(["cc", "-Wall", "-Werror", repo/"tests/learnharness/close-window.c",
                    "-lX11", "-o", root/"close"], check=True)
    usr = ["--ro-bind", "/usr", "/usr"] if not weston_root else [
        "--overlay-src", "/usr", "--overlay-src", str(weston_root/"usr"), "--ro-overlay", "/usr"]
    compositor = ["bwrap", "--die-with-parent", "--unshare-all", "--clearenv", *usr,
                  "--symlink", "usr/lib", "/lib", "--symlink", "usr/lib", "/lib64",
                  "--symlink", "usr/bin", "/bin", "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
                  "--bind", str(runtime), "/runtime", "--setenv", "XDG_RUNTIME_DIR", "/runtime",
                  "--setenv", "HOME", "/tmp", "--setenv", "PATH", "/usr/bin", "/usr/bin/weston",
                  "-B", "headless", "--renderer=pixman", "--shell=kiosk-shell.so", "--no-config", "--socket=managed-test"]
    def request(option):
        return subprocess.check_output([package, "--managed-" + option], env=env, text=True, timeout=20).strip()
    def launch():
        return subprocess.Popen([package, "--managed-launch"], env=env, stdout=subprocess.PIPE, text=True)
    slot = data/"io.github.tenkejj.learnharness-sync/managed/slot"
    vault_name = "vault-070" if release == "0.7.0" else "vault"
    def vault(command):
        return subprocess.check_output(["unshare", "--user", "--map-auto", "--map-root-user", "--mount",
            "bwrap", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--ro-bind", "/usr", "/usr", "--symlink", "usr/lib", "/lib64",
            "--symlink", "usr/lib", "/lib", "--proc", "/proc", "--dev", "/dev", "--bind", slot, "/slot",
            "setpriv", "--reuid=1", "--regid=1", "--clear-groups", "/usr/bin/bash", "-ec",
            f'cd /slot/{vault_name}; ' + command], env=env, text=True).strip()
    def saved():
        return vault('gen=$(cat current); [[ $(btrfs property get -ts "$gen" ro) == ro=true ]]; '
                     'test -d "$gen/config/LearnHarness"; echo "$gen"; '
                     'find "$gen" -type f -print0 | sort -z | xargs -0 -r sha256sum | sha256sum')
    def close_window(process, crash=False):
        server = until(lambda: find_child(process, "Xwayland"))
        tmp = Path(f"/proc/{server}/cwd").resolve(strict=True)
        def display_number():
            try:
                value = (tmp/"display").read_text().strip()
                return value if value.isdecimal() else None
            except FileNotFoundError:
                return None
        number = until(display_number)
        xenv = {**env, "DISPLAY": ":" + number, "XAUTHORITY": str(tmp/"display.auth")}
        print(f"Waiting for real LearnHarness window on {xenv['DISPLAY']}", flush=True)
        def window_ready():
            return subprocess.run([root/"close", "--ready"], env=xenv,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
        until(window_ready)
        print("Real LearnHarness window ready", flush=True)
        assert request("status") == "active"
        refused = subprocess.run([package, "--managed-snapshot"], env=env, capture_output=True, text=True, timeout=30)
        assert refused.returncode != 0, "Snapshot must refuse while the LearnHarness writer is active"
        try:
            os.listdir(slot/vault_name)
        except PermissionError as e:
            assert e.errno == errno.EACCES
        else:
            raise RuntimeError("An unmanaged writer reached the vault")
        if crash:
            print("Forcing managed supervisor termination", flush=True)
            refused = subprocess.run([package, "--managed-recover-confirmed"], env=env, capture_output=True, text=True)
            assert refused.returncode and "Close the managed" in refused.stderr
            supervisor = until(lambda: find_child(process, "bwrap"))
            os.kill(supervisor, signal.SIGKILL)
            assert process.wait(timeout=20) != 0
        else:
            print("Closing real LearnHarness window", flush=True)
            subprocess.run([root/"close"], env=xenv, check=True)
            assert process.communicate(timeout=40)[0].strip() == "saved" and process.returncode == 0
    def run_acceptance():
        display = None
        try:
            if not host_display:
                log = (root/"weston.log").open("wb")
                display = subprocess.Popen(compositor, env=env, stdout=log, stderr=log)
                until(lambda: (runtime/"managed-test").is_socket())
            assert request("status") == "new"
            debug_binary = repo/"target/debug/learnharness-sync"
            populate = subprocess.run([debug_binary, "--managed-populate-real-app-test"], env=env,
                                      capture_output=True, text=True, timeout=180)
            assert populate.returncode == 0, populate.stderr + populate.stdout
            oracle_line = next((line for line in populate.stdout.splitlines()
                                if line.startswith("MANAGED_REAL_APP_REPORT=")), None)
            assert oracle_line, populate.stdout
            oracle = __import__("json").loads(oracle_line.split("=", 1)[1])
            assert populate.stdout.splitlines()[-1] == "saved", populate.stdout
            assert request("status") == "saved"
            assert oracle["messageCount"] == 4
            assert len(oracle["propositionIds"]) == 3 and len(oracle["chainIds"]) == 2
            print(f"Real LearnHarness created {oracle['messageCount']} messages, 3 propositions and 2 graph chains for {oracle['chatId']}.", flush=True)
            assert request("status") == "saved"
            captured = subprocess.run([package, "--managed-snapshot"], env=env, capture_output=True, text=True, timeout=30)
            assert captured.returncode == 0, captured.stderr
            snapshot = __import__("json").loads(captured.stdout)
            assert snapshot["format"] == f"learnharness-{release}-disposable-v1"
            assert isinstance(snapshot["digest"], str) and len(snapshot["digest"]) == 64
            assert snapshot["files"] > 0 and snapshot["bytes"] > 0
            assert snapshot["manifest"] == oracle["manifest"], "Packaged helper manifest differs from independent post-exit file hashes"
            assert any(file["path"] == f"chats/{oracle['chatId']}.json" for file in snapshot["manifest"])
            assert sum(file["bytes"] for file in snapshot["manifest"]) == snapshot["bytes"]
            print(f"Subordinate-UID snapshot IPC passed: {snapshot['files']} files, {snapshot['bytes']} bytes, digest {snapshot['digest']}", flush=True)
            smoke = subprocess.run([package, "--smoke-test", "--smoke-managed-snapshot"], env=env,
                                   capture_output=True, text=True, timeout=120)
            if smoke.returncode != 0:
                report_path = next((line.split("=", 1)[1] for line in smoke.stdout.splitlines()
                                    if line.startswith("NATIVE_SMOKE_REPORT=")), None)
                detail = Path(report_path).read_text() if report_path else "no smoke report"
                raise AssertionError(f"Tauri smoke failed: {detail}; managed status: {request('status')}")
            report_line = next((line for line in smoke.stdout.splitlines()
                                if line.startswith("NATIVE_SMOKE_REPORT=")), None)
            assert report_line, smoke.stdout
            report = __import__("json").loads(Path(report_line.split("=", 1)[1]).read_text())
            assert report["ok"] and report["managed_snapshot"]["digest"] == snapshot["digest"], report
            assert report["managed_snapshot"]["manifest"] == oracle["manifest"], "Tauri command manifest differs from independent LearnHarness data"
            print("Tauri WebView command successfully requested the subordinate-UID snapshot.", flush=True)
            original_build = vault('exec 9<.; flock -n 9; cat profile-build')
            assert len(original_build) == 64
            vault('exec 9<.; flock -n 9; rm -- profile-build; '
                  'printf partial > .profile-build.interrupted; sync -f .')
            interrupted_build = subprocess.run([package, "--managed-snapshot"], env=env,
                                               capture_output=True, text=True, timeout=30)
            assert interrupted_build.returncode == 0, interrupted_build.stderr
            assert __import__("json").loads(interrupted_build.stdout)["digest"] == snapshot["digest"]
            assert vault('exec 9<.; flock -n 9; cat profile-build') == original_build
            vault('exec 9<.; flock -n 9; rm -- .profile-build.interrupted')
            print("Interrupted executable-binding write recovered using the original verified digest.", flush=True)
            vault('exec 9<.; flock -n 9; printf "%064d\\n" 0 > profile-build; sync -f .')
            wrong_build = subprocess.run([package, "--managed-snapshot"], env=env,
                                         capture_output=True, text=True, timeout=30)
            assert wrong_build.returncode != 0 and "another verified executable" in wrong_build.stderr, wrong_build.stderr
            vault(f'exec 9<.; flock -n 9; printf "%s\\n" "{original_build}" > profile-build; sync -f .')
            restored_build = subprocess.run([package, "--managed-snapshot"], env=env,
                                            capture_output=True, text=True, timeout=30)
            assert restored_build.returncode == 0, restored_build.stderr
            assert __import__("json").loads(restored_build.stdout)["digest"] == snapshot["digest"]
            print("Changed executable binding refused; restoring the verified build retained the same immutable snapshot.", flush=True)
            installed.rename(installed.with_suffix(".held"))
            unknown = home/"Applications/LearnHarness-0.8.0-x86_64.AppImage"
            unknown.write_bytes(b"Unverified disposable upgrade probe")
            try:
                pinned = subprocess.run([package, "--managed-snapshot"], env=env,
                                        capture_output=True, text=True, timeout=30)
                assert pinned.returncode == 0, pinned.stderr
                assert __import__("json").loads(pinned.stdout)["digest"] == snapshot["digest"]
                update_smoke = subprocess.run([package, "--smoke-test", "--smoke-managed-snapshot"], env=env,
                                               capture_output=True, text=True, timeout=120)
                assert update_smoke.returncode == 0, update_smoke.stderr + update_smoke.stdout
                report_path = next(line.split("=", 1)[1] for line in update_smoke.stdout.splitlines()
                                   if line.startswith("NATIVE_SMOKE_REPORT="))
                update_report = __import__("json").loads(Path(report_path).read_text())
                assert f"{release} verified" in update_report["installation"] and "unverified LearnHarness 0.8.0" in update_report["installation"]
                print(f"Unknown upstream upgrade refused; private pinned {release} image and UI warning passed.", flush=True)
            finally:
                unknown.unlink(missing_ok=True)
                installed.with_suffix(".held").rename(installed)
            original_generation = vault('exec 9<.; flock -n 9; cat current')
            malformed_generation = vault(
                'exec 9<.; flock -n 9; old=$(cat current); '
                'bad=session.$(cat /proc/sys/kernel/random/uuid); '
                'btrfs subvolume snapshot "$old" "$bad" >/dev/null; '
                'printf "{broken\\n" > "$bad/config/LearnHarness/data/chats/unsupported.json"; '
                'btrfs property set -ts "$bad" ro true; '
                'printf "%s\\n" "$bad" > current.next; sync -f .; mv -T current.next current; '
                'printf "%s\\n" "$bad"'
            )
            assert malformed_generation.startswith("session.")
            malformed = subprocess.run([package, "--managed-snapshot"], env=env,
                                        capture_output=True, text=True, timeout=30)
            assert malformed.returncode != 0, "Malformed LearnHarness content was accepted"
            assert request("status") == "recovery", "Rejected malformed capture must retain a recovery marker"
            assert request("recover-confirmed") == "recovered"
            restored = vault(
                f'exec 9<.; flock -n 9; printf "%s\\n" "{original_generation}" > current.next; '
                'sync -f .; mv -T current.next current'
            )
            assert restored == "", restored
            retry = subprocess.run([package, "--managed-snapshot"], env=env,
                                   capture_output=True, text=True, timeout=30)
            assert retry.returncode == 0, retry.stderr
            assert __import__("json").loads(retry.stdout)["digest"] == snapshot["digest"], "Retry did not recover the original verified generation"
            assert request("status") == "saved"
            print("Malformed content refused; recovery restored the prior immutable generation and capture retry matched.", flush=True)
            interrupted = subprocess.Popen([debug_binary, "--managed-snapshot-interruption-test"], env=env,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            helper_pid = None
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline and interrupted.poll() is None:
                helper_pid = find_descendant_command(interrupted, "--test-pause-before-read")
                if helper_pid:
                    break
                time.sleep(.01)
            assert helper_pid, "Timed out locating the deliberately paused subordinate-UID helper"
            os.kill(helper_pid, signal.SIGKILL)
            interrupted_out, interrupted_err = interrupted.communicate(timeout=30)
            assert interrupted.returncode != 0, interrupted_out
            assert request("status") == "recovery", "Killed helper did not retain its recovery marker"
            assert request("recover-confirmed") == "recovered"
            retry = subprocess.run([package, "--managed-snapshot"], env=env,
                                   capture_output=True, text=True, timeout=30)
            assert retry.returncode == 0, retry.stderr
            assert __import__("json").loads(retry.stdout)["digest"] == snapshot["digest"]
            print("Paused helper was killed across the UID boundary; recovery and packaged capture retry passed.", flush=True)
            if debug_binary.is_file():
                unauthorized = subprocess.run([debug_binary, "--managed-bridge-unauthorized-test"], env=env,
                                              capture_output=True, text=True, timeout=30)
                assert unauthorized.returncode == 0 and "not enabled" in unauthorized.stdout, unauthorized.stderr
                assert request("status") == "saved", "Rejected operation must release the capture marker"
                print("Unauthorized cross-UID managed operation was rejected.", flush=True)
            assert request("status") == "saved", "Snapshot bridge must release writer custody after success"
            previous = saved()
            close_window(launch(), crash=True)
            assert request("status") == "recovery" and saved() == previous
            assert request("recover-confirmed") == "recovered" and request("status") == "saved"
            assert saved() == previous
            close_window(launch())
            assert saved() != previous
            print("Real packaged window reopened. Kernel writer denial, busy recovery refusal, supervisor death, retained generation and restart passed.")
        finally:
            if display:
                display.terminate()
                display.wait(timeout=15)
    try:
        run_acceptance()
    finally:
        # Only the mapped test identity may dispose of this invocation's
        # private vault. A held supervisor lock refuses cleanup.
        try:
            vault('exec 9<.; flock -n 9; '
                  'for dir in session.*; do btrfs property set -ts "$dir" ro false; '
                  'if ! btrfs subvolume delete "$dir" >/dev/null; then '
                  'btrfs property set -ts "$dir" ro true; exit 1; fi; done; '
                  'rm -f -- current active apply.pending.json current.next profile-format profile-build interrupted.*; '
                  f'cd /slot; rmdir {vault_name}')
            shutil.rmtree(root)
        except subprocess.CalledProcessError:
            print(f"Disposable Btrfs test vault retained for administrator cleanup: {root}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("upstream_image", type=Path)
    parser.add_argument("--weston-root", type=Path)
    parser.add_argument("--host-display", action="store_true",
                        help="Use the current Wayland/X11 compositor instead of launching Weston")
    parser.add_argument("--release", choices=("0.6.1", "0.7.0"), default="0.6.1")
    args = parser.parse_args()
    exercise(args.package.resolve(strict=True), args.upstream_image.resolve(strict=True),
             args.weston_root.resolve(strict=True) if args.weston_root else None,
             args.host_display, args.release)
