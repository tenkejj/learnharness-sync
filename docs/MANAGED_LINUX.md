# Managed LearnHarness qualification on Linux

The native integration suite runs the upstream Linux x86_64 LearnHarness 0.6.1 executable. The desktop identifies the executable, provides a subordinate-UID offline managed launcher, and supports consented transfer, quarantine, backed-up Apply and application readback for the verified profile contract. It does not open the ordinary LearnHarness profile and provides no live-profile migration button. Tests create every profile they open. The current packaged A→B→A and recovery evidence is in [PRODUCTION_SYNC_ACCEPTANCE.md](PRODUCTION_SYNC_ACCEPTANCE.md).

## Reproduce

Use a Btrfs checkout with enough free space for the disposable migration source, a Linux kernel with unprivileged user namespaces, pre-existing subordinate UID/GID allocations, util-linux with automatic ID mapping, newuidmap/newgidmap, bubblewrap, Btrfs tools, setfacl, Xvfb, curl, Python 3, Node and Rust. The runner needs local namespace support and a writable checkout. It creates no system service. Supply an executable copy of the official distribution.

```sh
npm ci
scripts/test-learnharness-desktop ~/Applications/LearnHarness-0.6.1-x86_64.AppImage
```

A second argument can point to a separate Xvfb executable. Supply the built LearnHarness Sync AppImage as a third argument to run the packaged desktop inspection against the real, generated read-only backup. The runner hashes a private copy of the AppImage before extracting or executing it. The tested upstream asset is 196,784,591 bytes with SHA-256 `b8a033eb7421677893d96e4988b30153fdfe4fc6f0dc456f7947161bba041b98`. [Upstream release](https://github.com/amosblomqvist/learnharness/releases/tag/v0.6.1)

## Inspect a backup with the installed desktop

Open Settings → Advanced → Check a LearnHarness backup. Select the subvolume root by its absolute path. Supported layouts under that root are `data`, `LearnHarness/data`, `config/LearnHarness/data` and `.config/LearnHarness/data`. Exactly one layout may be present. The 16 MiB cap covers the total inspected content. The desktop opens a directory descriptor, checks the Btrfs subvolume read-only flag, and resolves records beneath that descriptor with Linux openat2. It rejects symlinks, mount crossings, nested volumes, hard-linked records, unrecognized inventory and unsupported schemas. Settings are inspected only for file type and link count. Only aggregate counts return to the window. The read-only check neither creates a revision nor starts peer transfer.

## Writer custody and migration

The runner maps an already allocated subordinate UID and GID. That identity creates the mode-0700 vault. The user running an ordinary LearnHarness process cannot traverse it. A host-UID write probe checks for a kernel permission denial. A second attempted transfer during a live Electron session must return the specific busy code. The managed process operates in a private PID, mount, network and IPC namespace with its own display, HOME and XDG directories. Only its own generated application profile is writable. A supervisor obtains the vault lock before entering a phase and waits for the application namespace to exit. Unexpected interruption leaves a durable activity marker.

The test also starts an ordinary-UID LearnHarness in a separate generated home. After this process family exits, the runner takes a read-only Btrfs snapshot. It grants the mapped identity access only to the generated data and test oracles and exposes the source data read-only. Migration validates the inventory, requires an empty destination, preserves the destination settings, and retains an independent backup. A fresh application opens the migrated conversation and graph. The runner compares the original source with the snapshot and removes its disposable profiles at exit.

A snapshot of a live unmanaged home could catch LearnHarness between writes to different stores. The suite knows when its original process family exits because it created that family. Normal migration from an arbitrary writable home cannot establish the same fact. The subordinate UID boundary covers ordinary unmanaged launches. The operating-system administrator and processes deliberately granted the same subordinate identity are trusted.

## Transfer and recovery

The native source peer reads the closed application files, excludes local settings and validates the complete allowlisted inventory. The receiver independently validates immutable objects and manifests. It checks the active checkout against the last confirmed application revision before staging incoming data. A transport fast-forward can happen while the application has offline edits. Those edits retain their application parent on export.

The transaction copies the prior tree into a separate backup. It verifies the backup and staged content by reading them again, flushes the files and directories, and writes a durable intent. A Linux atomic directory exchange activates the generation. If the process dies after the exchange, a fresh process compares both generations with the recorded hashes and reconstructs the old tree from the backup. Unknown or corrupt state stops recovery. The test opens the application, compares its data with an independent oracle, then confirms the received revision. A remote object receipt alone cannot establish application persistence.

## Application-level observations

| Exercise | Observation |
|---|---|
| A → B → A | A created three teach/practice sessions. B reopened all three, created three more, and A reopened all six by ID. Renames and large Unicode titles survived. |
| C → D → C | C authored two user turns and two assistant replies through a local keyless test provider. Extraction produced three propositions, two chains and message provenance. The desktop saved coordinates for the derived multi-premise junction. D reopened the original message IDs and graph, added another turn and renamed the chat. C reopened all six messages and compared their full records, propositions, chains and positions with D’s application-level oracle. |
| Divergence | C and D made offline edits. A new transport head reached a peer with an unpublished edit. Staging refused. Capture preserved the true parent and both peers retained both branches after restart. |
| Damage and faults | Invalid JSON, unsupported schema, malformed JSONL, broken graph endpoints and a real persisted attachment path were refused. Four native committers received SIGKILL immediately after exchange. Their replacements restored and verified the old generations. A 2 MiB private tmpfs generated kernel ENOSPC. A mode-000 input generated EACCES. Both preserved the destination and a later retry succeeded. |
| Migration | A separate ordinary-UID installation created real transcript and graph state. Managed E reopened it from an immutable backup and the original source still compared equal. |

The packaged supervisor saves local generations and offers explicit crash recovery. Peer Apply is available only in a managed profile with explicit confirmation, an independently verified backup and a subsequent real-application readback. The suite controls the display, model endpoint, application readback and clean shutdown. The schema accepts the exercised text-only custom model and refuses other provider records, tools, file paths, imports, unknown durable inventory and unsupported graph histories. Coverage bytes are retained. The validator checks their chat links and fingerprint shape. It has no general semantic replay for arbitrary upstream histories. Cold power loss, actual physical full-disk behavior and dispatch across two physical laptops remain open. [Record contract](LEARNHARNESS_061.md) · [Release status](RELEASE_READINESS.md)


## Installed launcher and upgrade identity

Settings exposes the managed window for the pinned installation. Each session has a nested, authenticated X display. The LearnHarness process receives a single writable Btrfs generation, private network and PID namespaces and its own HOME/XDG directories. Closing the window saves the generation after the namespace exits. The supervisor flushes the files, seals the generation read-only and atomically publishes the pointer. A crash leaves an activity marker. Recovery retains the interrupted tree and resumes the last sealed generation. If the first launch was interrupted, the next session starts fresh. Administrators and a user deliberately entering the same subordinate identity are trusted. Retained generations consume disk space.

The package identifier is `io.github.tenkejj.learnharness-sync`. The desktop opens the established native store at `$XDG_DATA_HOME/community.harnesssync.desktop/native-v2`, using the normal Linux user data directory when XDG_DATA_HOME is unset. No trust files or histories are copied or renamed. A store lock excludes simultaneously running old and new clients. The managed vault is separate from this native store. Close the previous client before launching the new one.

Run `python scripts/test-managed-launcher.py <candidate.AppImage> <LearnHarness-0.6.1.AppImage>` to test the packaged supervisor. It requires Btrfs, a headless Weston test compositor, X11 development headers and the launcher prerequisites described in the README.
