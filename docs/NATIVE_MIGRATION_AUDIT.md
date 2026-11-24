# Native migration audit — 2026-11-17

Initial repository state: all project files untracked, no HEAD commit. Preserved existing source and approved brand assets. No commits, pushes or publications performed.

Baseline: `cargo test` 2 Rust tests; `npm test` 11 frontend tests; `python -m unittest discover -s tests -q` 72 tests, all passed.

The old Tauri command launched `harness-sync-engine`, a frozen Python executable, for every request. Python implemented immutable per-device snapshot directories, causal ancestry, pre-import backups and a recoverable journal, with a Syncthing REST adapter transporting snapshots. The Rust core contained only hashing and ancestry helpers. Build/release scripts required PyInstaller and an externalBin sidecar. Those production dependencies are removed.

The Python supervisor is a separate-process Linux subreaper. It controls only its own isolated descendants, leaves a durable WRITER_ACTIVE quarantine on failure, and explicitly cannot exclude unmanaged production writers. It cannot safely be moved wholesale into a multithreaded desktop process. The native engine retains conservative marker refusal for synthetic workspaces and blocks ALL production operations. It does not scan or signal LearnHarness processes, read the real data directory, or pretend a process scan establishes exclusion. The older supervisor and schema tests remain as reference.

The native format is explicitly synthetic-v2, not a silent migration of Python snapshots. Existing Python profiles and immutable history are untouched and are never adopted as native profiles. Golden SHA-256 and causal behavior are preserved; byte-for-byte compatibility with old manifests is not claimed.

The native app owns a Tokio/Quinn endpoint, DNS-SD discovery thread, identity, private object store and status events in the same OS process. The fixture-peer executable is a test harness only, neither packaged nor spawned by the desktop.

No production LearnHarness fixture, screenshot, credential, application launch, process termination or data mutation is part of this work. Physical two-computer validation remains a separate pending acceptance gate.
