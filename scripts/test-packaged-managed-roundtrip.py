#!/usr/bin/env python3
"""Packaged two-peer LearnHarness round trip using isolated disposable profiles."""
import json
import hashlib
import base64
import os
from pathlib import Path
import queue
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time


REPO = Path(__file__).resolve().parents[1]
RELEASE = sys.argv[2] if len(sys.argv) > 2 else "0.6.1"
assert RELEASE in ("0.6.1", "0.7.0"), "Only pinned LearnHarness releases are qualified"
UPSTREAM = REPO / ("build/qualification/learnharness-070/LearnHarness-0.7.0-x86_64.AppImage"
                   if RELEASE == "0.7.0" else "build/qualification/real-sync/LearnHarness-0.6.1-x86_64.AppImage")
APPIMAGE = Path(sys.argv[1]).resolve(strict=True)
VAULT_NAME = "vault-070" if RELEASE == "0.7.0" else "vault"
QUARANTINE_NAME = "learnharness-0.7.0" if RELEASE == "0.7.0" else "learnharness-0.6.1"


def descendants(pid):
    children = []
    try:
        for task in (Path(f"/proc/{pid}/task")).iterdir():
            try:
                children.extend((task / "children").read_text().split())
            except (FileNotFoundError, PermissionError):
                continue
    except (FileNotFoundError, PermissionError):
        return
    for child in map(int, children):
        yield child
        yield from descendants(child)


class Peer:
    def __init__(self, name, home, data, store, failpoint=""):
        self.name = name
        self.home = home
        self.data = data
        self.store = store
        self.failpoint = failpoint
        self.output = queue.Queue()
        self.lock = threading.Lock()
        self.log = (home.parent / f"{name}.stderr.log").open("wb")
        self.start()

    def start(self):
        env = {
            "HOME": str(self.home),
            "XDG_DATA_HOME": str(self.data),
            "HARNESS_SYNC_TEST_PROFILE": str(self.store),
            "LEARNHARNESS_SYNC_ACCEPTANCE": "1",
            "LEARNHARNESS_SYNC_MANAGED_QA_LOG": "1",
            "PATH": "/usr/bin",
            "LANG": "C.UTF-8",
        }
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "XAUTHORITY"):
            if os.environ.get(key):
                env[key] = os.environ[key]
        if self.failpoint:
            env["LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT"] = self.failpoint
        self.process = subprocess.Popen(
            [str(APPIMAGE), "--managed-acceptance-peer"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.log,
            text=True,
            bufsize=1,
            env=env,
        )
        self.env = env
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"{self.name} packaged peer exited {self.process.returncode}")
            try:
                line = self.output.get(timeout=0.2)
            except queue.Empty:
                continue
            if line == "ACCEPTANCE_READY":
                return
            print(f"[{self.name}] {line}", flush=True)
        raise TimeoutError(f"{self.name} packaged acceptance console did not start")

    def _read(self):
        for line in self.process.stdout:
            self.output.put(line.rstrip("\n"))

    def request(self, command, args=None, timeout=240):
        with self.lock:
            if self.process.poll() is not None:
                raise RuntimeError(f"{self.name} peer is not running")
            self.process.stdin.write(json.dumps({"command": command, "args": args or {}}) + "\n")
            self.process.stdin.flush()
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                try:
                    line = self.output.get(timeout=0.2)
                except queue.Empty:
                    continue
                if line.startswith("ACCEPTANCE_RESULT "):
                    response = json.loads(line[len("ACCEPTANCE_RESULT "):])
                    if not response.get("ok"):
                        raise RuntimeError(f"{self.name} {command}: {response.get('error')}")
                    return response.get("result")
                if line.startswith("ACCEPTANCE_ERROR"):
                    raise RuntimeError(f"{self.name}: {line}")
                print(f"[{self.name}] {line}", flush=True)
            raise TimeoutError(f"{self.name} {command} timed out")

    def stop(self):
        if self.process.poll() is None:
            if self.lock.locked():
                self.process.terminate()
                try:
                    self.process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait()
                return
            try:
                self.request("__shutdown", timeout=5)
            except Exception:
                self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()


def mapped_vault(env, command):
    slot = Path(env["XDG_DATA_HOME"]) / "io.github.tenkejj.learnharness-sync/managed/slot"
    return subprocess.check_output(
        [
            "/usr/bin/unshare", "--user", "--map-auto", "--map-root-user", "--mount",
            "/usr/bin/bwrap", "--unshare-pid", "--unshare-net", "--unshare-ipc",
            "--ro-bind", "/usr", "/usr", "--symlink", "usr/lib", "/lib64",
            "--symlink", "usr/lib", "/lib", "--proc", "/proc", "--dev", "/dev",
            "--bind", str(slot), "/slot", "/usr/bin/setpriv", "--reuid=1", "--regid=1",
            "--clear-groups", "/usr/bin/bash", "-ec", f"cd /slot/{VAULT_NAME}; " + command,
        ],
        env=env,
        text=True,
        stderr=subprocess.PIPE,
        timeout=30,
    ).strip()


def launch_and_close(appimage, env, close_window, expect_verified=False, peer=None):
    launched = subprocess.Popen(
        [str(appimage), "--managed-launch"], env=env,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )

    def walk():
        return list(descendants(launched.pid))

    deadline = time.monotonic() + 90
    display = None
    while time.monotonic() < deadline and launched.poll() is None:
        for pid in walk():
            try:
                if Path(f"/proc/{pid}/comm").read_text().strip() == "Xwayland":
                    work = Path(f"/proc/{pid}/cwd").resolve(strict=True)
                    number_file = work / "display"
                    if number_file.exists():
                        display = number_file.read_text().strip()
                        if display.isdecimal():
                            auth = work / "display.auth"
                            xenv = dict(env, DISPLAY=f":{display}", XAUTHORITY=str(auth))
                            ready = subprocess.run(
                                [str(close_window), "--ready"], env=xenv,
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                            ).returncode == 0
                            if ready:
                                if peer:
                                    try:
                                        peer.request("__managed_capture_for_test", timeout=30)
                                    except RuntimeError as error:
                                        assert "managed" in str(error).lower(), str(error)
                                        print("Active managed writer refused capture.", flush=True)
                                    else:
                                        raise AssertionError("Capture did not refuse an active LearnHarness writer")
                                subprocess.run([str(close_window)], env=xenv, check=True, timeout=30)
                                stdout, stderr = launched.communicate(timeout=90)
                                result = stdout.strip()
                                assert launched.returncode == 0, stderr
                                if expect_verified:
                                    assert result == "verified", f"Application readback not confirmed: {result}"
                                return result
            except (FileNotFoundError, ProcessLookupError):
                continue
        time.sleep(0.1)
    launched.kill()
    stdout, stderr = launched.communicate()
    raise TimeoutError(f"Managed LearnHarness window did not open: {stdout} {stderr}")


def read_bundle(peer):
    bundle = peer.request("__managed_capture_for_test", timeout=60)
    manifest = bundle["manifest"]
    import base64
    records = {r["path"]: base64.b64decode(r["data"]) for r in bundle["records"]}
    assert len(records) == len(manifest["files"]) > 0
    assert sum(len(b) for b in records.values()) > 0
    return manifest, records


def read_quarantine(store, revision):
    target = store / "quarantine" / QUARANTINE_NAME / revision
    manifest = json.loads((target / "manifest.json").read_text())
    records = {}
    for row in manifest["files"]:
        path = row["path"]
        data = (target / path).read_bytes()
        assert len(data) == row["size"] and hashlib.sha256(data).hexdigest() == row["hash"]
        records[path] = data
    assert len(records) == len(manifest["files"]) > 0
    return manifest, records


def validate_application_content(peer, expected_id, extra_text=None):
    manifest, records = read_bundle(peer)
    chat_path = f"chats/{expected_id}.json"
    chat = json.loads(records[chat_path])
    assert chat["id"] == expected_id
    assert len(chat["messages"]) >= (6 if extra_text else 4)
    text = json.dumps(chat["messages"], ensure_ascii=False)
    for phrase in ("What is a triangle?", "Czy ma trzy boki?", "A triangle has three sides."):
        assert phrase in text, phrase
    if extra_text:
        assert extra_text in text
    nodes = [json.loads(x) for x in records["graph/nodes.jsonl"].splitlines() if x]
    chains = [json.loads(x) for x in records["graph/chains.jsonl"].splitlines() if x]
    assert len(nodes) == 3 and len(chains) == 2
    assert all(any(p["chatId"] == expected_id and any(m["id"] == p["messageId"] for m in chat["messages"]) for p in node["provenance"]) for node in nodes)
    if RELEASE == "0.7.0":
        organization = json.loads(records["graph/source-organization.json"])
        assert organization["version"] == 1 and len(organization["folders"]) == 1
        folder = organization["folders"][0]
        assert folder["name"] == "Disposable 0.7 folder"
        assert organization["placements"][expected_id]["folderId"] == folder["id"]
    return manifest, records, chat, nodes, chains


def main():
    assert UPSTREAM.is_file(), f"Verified LearnHarness {RELEASE} image missing: {UPSTREAM}"
    expected_sha = ("772b5e04d380cbe8a72a75dbff269aeb6a47f4d4452711b1da8059a872067613"
                    if RELEASE == "0.7.0" else "b8a033eb7421677893d96e4988b30153fdfe4fc6f0dc456f7947161bba041b98")
    assert hashlib.sha256(UPSTREAM.read_bytes()).hexdigest() == expected_sha, "Upstream AppImage digest mismatch"
    assert os.environ.get("XDG_RUNTIME_DIR"), "A desktop runtime directory is required for real application readback"
    root = Path(tempfile.mkdtemp(prefix="packaged-managed-roundtrip-", dir=REPO / "build"))
    close_window = root / "close-window"
    subprocess.run(
        ["cc", "-Wall", "-Werror", REPO / "tests/learnharness/close-window.c", "-lX11", "-o", close_window],
        check=True,
    )
    peers = []
    try:
        envs = {}
        for name in ("a", "b"):
            home, data, store = (root / f"{name}-{part}" for part in ("home", "data", "native"))
            for path in (home, data, store):
                path.mkdir(mode=0o700)
            (home / "Applications").mkdir()
            app = home / f"Applications/LearnHarness-{RELEASE}-x86_64.AppImage"
            shutil.copy2(UPSTREAM, app)
            app.chmod(0o700)
            env = {
                "HOME": str(home), "XDG_DATA_HOME": str(data), "XDG_RUNTIME_DIR": os.environ["XDG_RUNTIME_DIR"],
                "PATH": "/usr/bin", "LANG": "C.UTF-8", "LEARNHARNESS_SYNC_MANAGED_QA_LOG": "1",
            }
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"):
                if os.environ.get(key):
                    env[key] = os.environ[key]
            envs[name] = (env, store)

        a = Peer("peer-a", root / "a-home", root / "a-data", root / "a-native")
        b = Peer("peer-b", root / "b-home", root / "b-data", root / "b-native")
        peers += [a, b]
        for peer in peers:
            status = peer.request("status")
            assert status["transport"]["device_id"]
        a_report = a.request("__qa_populate_profile", timeout=300)
        report_line = next(line for line in a_report.splitlines() if line.startswith("MANAGED_REAL_APP_REPORT="))
        expected = json.loads(report_line.split("=", 1)[1])
        chat_id = expected["chatId"]
        assert expected["messageCount"] == 4 and len(expected["propositionIds"]) == 3 and len(expected["chainIds"]) == 2
        print(f"Peer A real LearnHarness profile created: {chat_id}; 4 messages, 3 propositions, 2 chains.", flush=True)

        b_env, b_store = envs["b"]
        assert launch_and_close(APPIMAGE, b_env, close_window) == "saved", "B managed profile initialization failed"

        # Pair through the production QUIC and mutual trust protocol.
        a.request("open_pairing")
        b.request("open_pairing")
        invitation = b.request("invitation")["invitation"]
        invitation_data = json.loads(base64.b64decode(invitation))
        invitation_data["addresses"] = [f"127.0.0.1:{b.request('status')['listen_port']}"]
        invitation = base64.b64encode(json.dumps(invitation_data, separators=(",", ":")).encode()).decode()
        a.request("start_pairing", {"invitation": invitation})
        pending_a = pending_b = None
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            rows_a, rows_b = a.request("status")["pending"], b.request("status")["pending"]
            if rows_a and rows_b:
                pending_a, pending_b = rows_a[0], rows_b[0]
                break
            time.sleep(0.2)
        assert pending_a and pending_b and pending_a["code"] == pending_b["code"], "Mutual pairing confirmation codes did not match"
        a.request("pairing_decision", {"id": pending_a["id"], "accept": True})
        b.request("pairing_decision", {"id": pending_b["id"], "accept": True})
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            a_status, b_status = a.request("status"), b.request("status")
            if any(p["connected"] for p in a_status["transport"]["peers"]) and any(p["connected"] for p in b_status["transport"]["peers"]):
                break
            time.sleep(0.25)
        else:
            raise TimeoutError("Paired QUIC peers did not establish an authenticated session")
        aid = a_status["transport"]["device_id"]
        bid = b_status["transport"]["device_id"]
        assert aid != bid
        print("Two independent packaged native peers paired and connected over authenticated QUIC.", flush=True)

        export = a.request("managed_export", {"device": bid, "consent": True}, timeout=180)
        assert export, "Managed export returned no authenticated receipt"
        b_status = b.request("status")
        staged = b_status["managed_exports"]
        assert len(staged) == 1
        revision = staged[0]["revision"]
        staged_manifest, staged_payload = read_quarantine(b_store, revision)
        assert staged_manifest["revision"] == revision
        assert staged_manifest["format"] == f"learnharness-{RELEASE}-disposable-v1"
        assert f"chats/{chat_id}.json" in staged_payload
        print(f"QUIC delivered and verified {staged[0]['files']} content records ({staged[0]['bytes']} bytes) into B quarantine.", flush=True)

        # Inject ENOSPC after the independent backup is durable but before a new
        # generation is built. The profile must remain current and retryable.
        b.stop()
        b = Peer(
            "peer-b-enospc", root / "b-home", root / "b-data", root / "b-native",
            failpoint="enospc-after-backup",
        )
        peers[-1] = b
        old_generation = mapped_vault(b.env, "cat current")
        try:
            b.request("managed_apply", {"revision": revision, "confirmed": True})
        except RuntimeError as error:
            assert "space" in str(error).lower(), f"ENOSPC was not reported: {error}"
        else:
            raise AssertionError("Managed apply did not fail at the injected ENOSPC point")
        assert mapped_vault(b.env, "test \"$(cat current)\" = \"" + old_generation + "\" && test ! -e active && test ! -e apply.pending.json && echo unchanged") == "unchanged"
        assert mapped_vault(b.env, "find backups -mindepth 1 -maxdepth 1 -type d -name 'backup-*' | wc -l") == "1"
        print("Injected ENOSPC after durable backup retained B's prior generation and allowed retry.", flush=True)

        # Inject process death only after the durable apply journal and current
        # generation switch. The feature-only helper sleeps at that precise point.
        b.stop()
        b = Peer(
            "peer-b-failpoint", root / "b-home", root / "b-data", root / "b-native",
            failpoint="pause-after-apply-switch",
        )
        peers[-1] = b
        apply_args = {"revision": revision, "confirmed": True}
        outcome = []
        failure = []
        t = threading.Thread(target=lambda: _attempt(b, apply_args, outcome, failure), daemon=True)
        old_generation = mapped_vault(b.env, "cat current")
        t.start()
        deadline = time.monotonic() + 45
        new_generation = None
        while time.monotonic() < deadline:
            try:
                state = mapped_vault(b.env, "test -f apply.pending.json && cat current")
                if state and state != old_generation:
                    new_generation = state
                    break
            except subprocess.CalledProcessError:
                pass
            time.sleep(0.1)
        assert new_generation, "Apply did not reach its journaled generation switch failpoint"
        helper_pid = None
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and helper_pid is None:
            for pid in descendants(b.process.pid):
                try:
                    comm = Path(f"/proc/{pid}/comm").read_text().strip()
                    cmdline = Path(f"/proc/{pid}/cmdline").read_bytes().decode(errors="ignore")
                    if comm == "helper" and "--managed-bridge-stdio" in cmdline:
                        helper_pid = pid
                        break
                except (FileNotFoundError, PermissionError):
                    pass
            time.sleep(0.05)
        assert helper_pid, "Could not locate the subordinate-UID apply helper for SIGKILL injection"
        os.kill(helper_pid, signal.SIGKILL)
        t.join(30)
        assert not t.is_alive(), "Killed apply helper did not release the packaged Tauri request"
        assert failure and "recovery" in failure[0].lower(), f"Interrupted apply did not report recovery: {failure}"
        assert mapped_vault(b.env, "test -f active && test -f apply.pending.json && echo retained") == "retained"
        print("SIGKILL after durable apply switch retained the transaction for explicit recovery.", flush=True)

        recovered = b.request("managed_recover", {"confirmed": True})
        assert recovered == "recovered"
        assert mapped_vault(b.env, "test \"$(cat current)\" = \"" + old_generation + "\" && test ! -e apply.pending.json && echo restored") == "restored"
        b.stop()
        peers.remove(b)
        b = Peer("peer-b-retry", root / "b-home", root / "b-data", root / "b-native")
        peers.append(b)
        retry_revision = revision
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            status = b.request("status")
            if any(p["connected"] for p in status["transport"]["peers"]):
                break
            time.sleep(0.25)
        else:
            raise TimeoutError("Trusted QUIC connection did not resume after B process restart")
        retry = b.request("managed_apply", {"revision": retry_revision, "confirmed": True})
        assert retry["state"] == "awaiting_readback" and retry["applied"] is True
        assert launch_and_close(APPIMAGE, b_env, close_window, expect_verified=True, peer=b) == "verified"
        manifest_b, records_b, chat_b, nodes_b, chains_b = validate_application_content(b, chat_id)
        assert manifest_b["revision"] == retry["merged_revision"]
        assert [row["id"] for row in chat_b["messages"]] == expected["messageIds"]
        assert sorted(row["id"] for row in nodes_b) == expected["propositionIds"]
        assert sorted(row["id"] for row in chains_b) == expected["chainIds"]
        print("B's real LearnHarness reopened, persisted IDs/messages/graph/provenance, and confirmed readback.", flush=True)

        # Create genuinely new conversation content inside B's managed profile
        # with the real application, then close and snapshot it again.
        report_b = b.request("__qa_extend_profile", timeout=300)
        extended_line = next(line for line in report_b.splitlines() if line.startswith("MANAGED_REAL_APP_REPORT="))
        extended = json.loads(extended_line.split("=", 1)[1])
        assert extended["chatId"] == chat_id and extended["messageCount"] == 6
        marker = "Follow-up created on the second packaged peer – 続き ✓"
        print("Real LearnHarness on B added a Unicode follow-up and exited cleanly.", flush=True)

        # Reverse transfer and apply to A through A's own managed helper.
        export_b = b.request("managed_export", {"device": aid, "consent": True}, timeout=180)
        assert export_b
        a_status = a.request("status")
        incoming = next((x for x in a_status["managed_exports"] if x["revision"] != expected.get("revision")), None)
        assert incoming, "A did not quarantine B's new revision"
        applied_a = a.request("managed_apply", {"revision": incoming["revision"], "confirmed": True})
        assert applied_a["state"] == "awaiting_readback" and applied_a["applied"]
        a_env, _ = envs["a"]
        assert launch_and_close(APPIMAGE, a_env, close_window, expect_verified=True, peer=a) == "verified"
        manifest_a, records_a, chat_a, nodes_a, chains_a = validate_application_content(a, chat_id, marker)
        assert manifest_a["revision"] == applied_a["merged_revision"]
        assert len(chat_a["messages"]) == 6
        assert sorted(row["id"] for row in nodes_a) == expected["propositionIds"]
        assert sorted(row["id"] for row in chains_a) == expected["chainIds"]
        print("PASS: packaged real LearnHarness A→B→A, including B-authored follow-up and graph readback.", flush=True)
    finally:
        for peer in reversed(peers):
            peer.stop()
        cleaned = True
        for name in ("a", "b"):
            env = {
                "HOME": str(root / f"{name}-home"),
                "XDG_DATA_HOME": str(root / f"{name}-data"),
                "PATH": "/usr/bin",
                "LANG": "C.UTF-8",
            }
            try:
                mapped_vault(
                    env,
                    'exec 9<.; flock -n 9; '
                    'for g in session.* backups/backup-*; do '
                    '[[ -d "$g" && ! -L "$g" ]] || continue; '
                    '[[ $(btrfs property get -ts "$g" ro) == ro=true ]] && btrfs property set -ts "$g" ro false; '
                    'if ! btrfs subvolume delete "$g"; then btrfs property set -ts "$g" ro true; exit 1; fi; done; '
                    'for f in transactions/apply-*.json; do [[ -f "$f" ]] && rm -- "$f"; done; '
                    'rm -f -- active current current.next apply.pending.json profile-format profile-build; '
                    f'rmdir transactions backups 2>/dev/null || true; rmdir {VAULT_NAME}'
                )
            except (OSError, subprocess.CalledProcessError) as exc:
                cleaned = False
                print(f"Disposable Btrfs test vault retained; exact cleanup was refused: {root}/{name}-data: {exc}", flush=True)
        if cleaned:
            shutil.rmtree(root)


def _attempt(peer, args, outcome, failure):
    try:
        outcome.append(peer.request("managed_apply", args, timeout=230))
    except Exception as error:
        failure.append(str(error))


if __name__ == "__main__":
    main()
