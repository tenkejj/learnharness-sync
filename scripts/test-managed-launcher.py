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
    except (FileNotFoundError, PermissionError):
        return
    for child in map(int, children):
        yield child
        yield from descendants(child)


def find_child(process, name):
    for pid in descendants(process.pid):
        try:
            if Path(f"/proc/{pid}/comm").read_text().strip() == name:
                return pid
        except (FileNotFoundError, PermissionError):
            pass
    if process.poll() is not None:
        raise RuntimeError("Managed launcher exited before the window appeared")


def exercise(package, image, weston_root):
    repo = Path(__file__).resolve().parents[1]
    root = Path(tempfile.mkdtemp(prefix="managed-accept-", dir=repo/"build"))
    home, runtime, data = (root/p for p in ("home", "runtime", "data"))
    for folder in (home, runtime, data):
        folder.mkdir(mode=0o700)
    (home/"Applications").mkdir()
    shutil.copy2(image, home/"Applications/LearnHarness-0.6.1-x86_64.AppImage")
    env = {"HOME": str(home), "XDG_DATA_HOME": str(data), "XDG_RUNTIME_DIR": str(runtime),
           "WAYLAND_DISPLAY": "managed-test", "PATH": "/usr/bin", "LANG": "C.UTF-8"}
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
    def vault(command):
        return subprocess.check_output(["unshare", "--user", "--map-auto", "--map-root-user", "--mount",
            "bwrap", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--ro-bind", "/usr", "/usr", "--symlink", "usr/lib", "/lib64",
            "--symlink", "usr/lib", "/lib", "--proc", "/proc", "--dev", "/dev", "--bind", slot, "/slot",
            "setpriv", "--reuid=1", "--regid=1", "--clear-groups", "/usr/bin/bash", "-ec",
            'cd /slot/vault; ' + command], env=env, text=True).strip()
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
        try:
            os.listdir(slot/"vault")
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
    with (root/"weston.log").open("wb") as log:
        display = subprocess.Popen(compositor, env=env, stdout=log, stderr=log)
        try:
            until(lambda: (runtime/"managed-test").is_socket())
            assert request("status") == "new"
            close_window(launch())
            assert request("status") == "saved"
            previous = saved()
            close_window(launch(), crash=True)
            assert request("status") == "recovery" and saved() == previous
            assert request("recover-confirmed") == "recovered" and request("status") == "saved"
            assert saved() == previous
            close_window(launch())
            assert saved() != previous
            print("Real packaged window reopened. Kernel writer denial, busy recovery refusal, supervisor death, retained generation and restart passed.")
        finally:
            display.terminate()
            display.wait(timeout=15)
    # Only the mapped test identity can dispose of subvolumes it created.
    try:
        vault('for dir in session.*; do btrfs property set -ts "$dir" ro false; '
              'btrfs subvolume delete "$dir" >/dev/null; done; '
              'rm -- current interrupted.*; cd /slot; rmdir vault')
        shutil.rmtree(root)
    except subprocess.CalledProcessError:
        print(f"Disposable Btrfs test vault retained for administrator cleanup: {root}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("upstream_image", type=Path)
    parser.add_argument("--weston-root", type=Path)
    args = parser.parse_args()
    exercise(args.package.resolve(strict=True), args.upstream_image.resolve(strict=True),
             args.weston_root.resolve(strict=True) if args.weston_root else None)
