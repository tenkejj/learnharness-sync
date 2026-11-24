#!/usr/bin/env python3
"""Private two-computer acceptance using newly created disposable Btrfs profiles.

This runner never selects the caller's ordinary HOME or XDG data directory.
The feature-enabled AppImage is a private test artifact, never a release asset.
"""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile


UPSTREAM_SHA256 = "b8a033eb7421677893d96e4988b30153fdfe4fc6f0dc456f7947161bba041b98"
MARKER = ".learnharness-sync-disposable-acceptance.json"


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def regular_file(path):
    path = Path(path).absolute()
    mode = path.lstat().st_mode
    if not stat.S_ISREG(mode) or path.is_symlink():
        raise RuntimeError(f"Expected a regular, non-symlink file: {path}")
    return path


def private_root(path):
    path = Path(path).absolute()
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or path.is_symlink():
        raise RuntimeError("Acceptance root must be a real directory")
    if metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise RuntimeError("Acceptance root must be owned by this user and mode 0700")
    return path


def environment(root, test=False):
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    if not runtime:
        raise RuntimeError("A graphical desktop XDG_RUNTIME_DIR is required")
    env = {
        "HOME": str(root / "home"),
        "XDG_DATA_HOME": str(root / "data"),
        "XDG_RUNTIME_DIR": runtime,
        "PATH": "/usr/bin",
        "LANG": "C.UTF-8",
    }
    for key in ("DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"):
        if os.environ.get(key):
            env[key] = os.environ[key]
    if test:
        env["LEARNHARNESS_SYNC_ACCEPTANCE"] = "1"
        env["LEARNHARNESS_SYNC_MANAGED_QA_LOG"] = "1"
    return env


def write_state(root, state):
    temporary = root / (MARKER + ".next")
    with temporary.open("x", encoding="utf-8") as target:
        os.chmod(temporary, 0o600)
        json.dump(state, target, sort_keys=True)
        target.write("\n")
        target.flush()
        os.fsync(target.fileno())
    temporary.replace(root / MARKER)
    directory = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def read_state(root):
    root = private_root(root)
    if not root.name.startswith("learnharness-sync-physical-"):
        raise RuntimeError("Refusing a root outside the disposable acceptance naming contract")
    marker = regular_file(root / MARKER)
    state = json.loads(marker.read_text(encoding="utf-8"))
    if state.get("format") != "learnharness-sync-disposable-v1":
        raise RuntimeError("This is not a prepared disposable acceptance root")
    for name in ("home", "data"):
        path = root / name
        if path.is_symlink() or not path.is_dir() or path.stat().st_uid != os.getuid():
            raise RuntimeError("Disposable acceptance directories changed")
    for name in ("ordinary", "acceptance"):
        artifact = regular_file(state[name]["path"])
        if digest(artifact) != state[name]["sha256"]:
            raise RuntimeError(f"{name} AppImage changed since preparation")
    return root, state


def prepare(args):
    parent = private_root(args.parent)
    filesystem = subprocess.check_output(
        ["/usr/bin/stat", "-f", "-c", "%T", str(parent)], text=True
    ).strip()
    if filesystem != "btrfs":
        raise RuntimeError("Prepare inside a private directory on Btrfs")
    ordinary = regular_file(args.ordinary)
    acceptance = regular_file(args.acceptance)
    upstream = regular_file(args.learnharness)
    if digest(upstream) != UPSTREAM_SHA256:
        raise RuntimeError("LearnHarness image differs from verified 0.6.1 x86_64 build")
    root = Path(tempfile.mkdtemp(prefix="learnharness-sync-physical-", dir=parent))
    try:
        for name in ("home", "data"):
            (root / name).mkdir(mode=0o700)
        applications = root / "home/Applications"
        applications.mkdir(mode=0o700)
        private_app = applications / "LearnHarness-0.6.1-x86_64.AppImage"
        with upstream.open("rb") as source, private_app.open("xb") as target:
            os.chmod(private_app, 0o700)
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                target.write(chunk)
            target.flush()
            os.fsync(target.fileno())
        if digest(private_app) != UPSTREAM_SHA256:
            raise RuntimeError("Private LearnHarness copy failed integrity check")
        ordinary_check = subprocess.run(
            [str(ordinary), "--managed-acceptance-peer"],
            env=environment(root), capture_output=True, text=True, timeout=20,
        )
        if ordinary_check.returncode != 64 or "absent from production builds" not in ordinary_check.stderr:
            raise RuntimeError("Ordinary AppImage unexpectedly contains the acceptance console")
        write_state(root, {
            "format": "learnharness-sync-disposable-v1", "state": "prepared",
            "ordinary": {"path": str(ordinary), "sha256": digest(ordinary)},
            "acceptance": {"path": str(acceptance), "sha256": digest(acceptance)},
        })
    except BaseException:
        # A private LearnHarness copy is the only file created before the
        # marker. Never recursively delete a root after a vault may exist.
        print(f"Preparation stopped; inspect this new disposable directory: {root}", file=sys.stderr)
        raise
    print(root)


def console_request(root, state, command):
    request = json.dumps({"command": command, "args": {}}) + "\n"
    shutdown = json.dumps({"command": "__shutdown", "args": {}}) + "\n"
    result = subprocess.run(
        [state["acceptance"]["path"], "--managed-acceptance-peer"],
        input=request + shutdown, env=environment(root, test=True),
        capture_output=True, text=True, timeout=300,
    )
    responses = []
    for line in result.stdout.splitlines():
        if line.startswith("ACCEPTANCE_RESULT "):
            responses.append(json.loads(line.removeprefix("ACCEPTANCE_RESULT ")))
    if result.returncode or len(responses) < 2 or not responses[0].get("ok") or not responses[1].get("ok"):
        reason = next((row.get("error") for row in responses if not row.get("ok")), None)
        raise RuntimeError(
            f"Private acceptance operation failed (exit {result.returncode}): "
            f"{str(reason or result.stderr[-800:] or 'no result')[:800]}; "
            "disposable profile retained for inspection"
        )
    return responses[0]["result"]


def test_operation(root, state, command):
    if state["state"] not in ("prepared", "seeded", "extended"):
        raise RuntimeError("Prior acceptance operation has an uncertain outcome; inspect the disposable profile")
    if command == "__qa_populate_profile" and state["state"] != "prepared":
        raise RuntimeError("Initial test conversation may only be created once")
    state["state"] = "operation-in-progress"
    write_state(root, state)
    output = console_request(root, state, command)
    report = next(
        (line for line in output.splitlines() if line.startswith("MANAGED_REAL_APP_REPORT=")),
        None,
    )
    if not report:
        raise RuntimeError("Real LearnHarness report was absent; profile retained for inspection")
    content = json.loads(report.removeprefix("MANAGED_REAL_APP_REPORT="))
    state["state"] = "seeded" if command == "__qa_populate_profile" else "extended"
    write_state(root, state)
    print(json.dumps({
        "state": state["state"], "chatId": content["chatId"],
        "messageCount": content["messageCount"],
        "propositionCount": len(content["propositionIds"]),
        "chainCount": len(content["chainIds"]),
    }, sort_keys=True))


def verify(root, state, chat_id, expected_messages):
    if state["state"] == "operation-in-progress":
        raise RuntimeError("A previous acceptance operation has an uncertain outcome")
    bundle = console_request(root, state, "__managed_capture_for_test")
    manifest = bundle["manifest"]
    files = manifest["files"]
    records = {row["path"]: base64.b64decode(row["data"], validate=True) for row in bundle["records"]}
    if len(records) != len(files) or set(records) != {row["path"] for row in files}:
        raise RuntimeError("Managed capture inventory differs from its manifest")
    for row in files:
        data = records[row["path"]]
        if len(data) != row["size"] or hashlib.sha256(data).hexdigest() != row["hash"]:
            raise RuntimeError("Managed capture content failed an independent hash check")
    chat = json.loads(records[f"chats/{chat_id}.json"])
    messages = chat["messages"]
    if chat["id"] != chat_id or len(messages) != expected_messages:
        raise RuntimeError("Conversation identity or message count differs")
    message_ids = {row["id"] for row in messages}
    nodes = [json.loads(line) for line in records["graph/nodes.jsonl"].splitlines() if line]
    chains = [json.loads(line) for line in records["graph/chains.jsonl"].splitlines() if line]
    node_ids = {row["id"] for row in nodes}
    if len(nodes) != 3 or len(chains) != 2 or len(node_ids) != 3:
        raise RuntimeError("Knowledge graph proposition or chain count differs")
    if not all(any(anchor["chatId"] == chat_id and anchor["messageId"] in message_ids
                   for anchor in row["provenance"]) for row in nodes):
        raise RuntimeError("Knowledge graph message provenance differs")
    if not all(set(chain["premises"]).issubset(node_ids) and chain["conclusion"] in node_ids
               for chain in chains):
        raise RuntimeError("Knowledge graph relationships differ")
    positions = json.loads(records["graph/positions.json"])["positions"]
    if len(positions) != 4 or not node_ids.issubset(positions) or not any(chain["id"] in positions for chain in chains):
        raise RuntimeError("Saved graph layout differs")
    print(json.dumps({
        "verified": True, "chatId": chat_id,
        "messageIds": sorted(message_ids),
        "propositionIds": sorted(node_ids),
        "chainIds": sorted(chain["id"] for chain in chains),
        "files": len(files), "bytes": sum(row["size"] for row in files),
        "revision": manifest["revision"],
    }, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    choices = parser.add_subparsers(dest="action", required=True)
    create = choices.add_parser("prepare", help="Create a new isolated acceptance HOME on Btrfs")
    for name in ("parent", "ordinary", "acceptance", "learnharness"):
        create.add_argument("--" + name, type=Path, required=True)
    for name in ("seed", "extend", "app", "learnharness", "snapshot", "verify"):
        action = choices.add_parser(name)
        action.add_argument("root", type=Path)
        if name == "verify":
            action.add_argument("--chat-id", required=True)
            action.add_argument("--messages", type=int, required=True)
    args = parser.parse_args()
    if args.action == "prepare":
        prepare(args)
        return
    root, state = read_state(args.root)
    if args.action in ("seed", "extend"):
        test_operation(root, state, "__qa_populate_profile" if args.action == "seed" else "__qa_extend_profile")
        return
    if args.action == "verify":
        verify(root, state, args.chat_id, args.messages)
        return
    if state["state"] == "operation-in-progress":
        raise RuntimeError("A previous acceptance operation has an uncertain outcome")
    option = {"app": None, "learnharness": "--managed-launch", "snapshot": "--managed-snapshot"}[args.action]
    command = [state["ordinary"]["path"]] + ([option] if option else [])
    raise SystemExit(subprocess.run(command, env=environment(root), check=False).returncode)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"Acceptance refused: {error}", file=sys.stderr)
        raise SystemExit(1) from None
