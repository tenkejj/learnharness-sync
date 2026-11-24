> Historical reference evidence from the Python/Syncthing implementation. It does not certify the current native engine. See NATIVE_MIGRATION_AUDIT.md and NATIVE_TEST_EVIDENCE.md. Production access remains disabled.

# Audit — 2026-11-16

This is an independent implementation. No AGPL source was copied into the product.

## Public upstream (main, not pinned to an immutable commit)

Read the public repository and requested store/lifecycle paths through browser fetches. Network DNS is unavailable to shell tools, so no upstream checkout was made. Findings are provisional across future upstream changes.

- [config.ts](https://github.com/amosblomqvist/learnharness/blob/main/config.ts): LH_DATA_DIR selects mutable records; auth is separate. Settings, chat JSON, graph JSONL and format stamp require coordinated treatment. Attachments and source paths are machine-local references, not portable content.
- [electron/main.cjs](https://github.com/amosblomqvist/learnharness/blob/main/electron/main.cjs): Electron starts a standalone Next server with a distinct data directory. The main process signals shutdown; child and worker termination needs separate proof. Single-instance behavior alone cannot protect against arbitrary file writers.
- [stores.ts](https://github.com/amosblomqvist/learnharness/blob/main/src/server/stores.ts) wires chat, graph, jobs and settings stores. [chats.ts](https://github.com/amosblomqvist/learnharness/blob/main/src/store/chats.ts), [jobs.ts](https://github.com/amosblomqvist/learnharness/blob/main/src/store/jobs.ts), [settings.ts](https://github.com/amosblomqvist/learnharness/blob/main/src/store/settings.ts) persist independent JSON records. The reference engine does not claim to validate their complete domain schemas.
- [graph.ts](https://github.com/amosblomqvist/learnharness/blob/main/src/store/graph.ts): graph ledgers encode domain operations. Arbitrary concatenation or union is unsafe, especially with deletions.
- [atomic-write.ts](https://github.com/amosblomqvist/learnharness/blob/main/src/store/atomic-write.ts): individual replacement is not a multi-file consistency boundary.
- [graph-format.ts](https://github.com/amosblomqvist/learnharness/blob/main/src/store/graph-format.ts): format guards matter on downgrade. LearnHarness Sync's fixture stamp is deliberately distinct; production versions are never assumed compatible.
- [architecture](https://github.com/amosblomqvist/learnharness/blob/main/docs/architecture.md) and [development](https://github.com/amosblomqvist/learnharness/blob/main/docs/development.md) distinguish server persistence, Electron and plugins. Durable plugin state, prompt overrides, themes, migrations and absolute attachments need a versioned production adapter; the fixture engine rejects unknown paths instead of silently omitting them.

## Alternatives and discussions

[LearnHarness Git](https://github.com/heohlee/learnharness-git) already offers plugin UI, automatic Git backup/sync, history and conflict handling. LearnHarness Sync aims at a separate desktop control center with immutable multi-file snapshots transported over Syncthing, without a user-owned remote Git repository. Only the local synthetic protocol and monitor are implemented here; real transport exchange is not claimed verified.

[eliasmatczak/learnharnessSync](https://github.com/eliasmatczak/learnharnessSync) could not be retrieved through available public fetches. No conclusion about its implementation or license is asserted. Upstream syncing issue search was also unavailable; no issue consensus is invented.

## Local installation, read-only audit

Detected user desktop entry points to a shell launcher selecting the latest LearnHarness AppImage in the Applications directory. File present: LearnHarness-0.6.1-x86_64.AppImage. It was extracted with AppImage's extraction mode into /tmp solely to inspect public packaged code; it was not launched. See local audit results in RELEASE_READINESS.md. Existing launcher and services were not modified.

Default XDG LearnHarness directory exists. Only names at its top level were inspected, never session payloads or auth contents. Separate credential files were observed outside data. No private data was copied to fixtures or source.

## Build environment

Node 26, Python 3.14, GTK 3 and WebKitGTK 4.1 available. Rust/Cargo and Syncthing missing. npm registry DNS unavailable. Cached tarballs were read without network or lifecycle scripts for frontend validation; actual versions are recorded in local-dependency-versions.json. Local HTTP socket creation is denied (EPERM). The browser can render file-based synthetic projections, but screenshots require unavailable tool approval.


## Continuation environment verification

Rechecked tools and socket permissions: Rust/Cargo and Syncthing remain absent, PyInstaller is absent, and socket construction itself is denied (EPERM). Git cannot create index.lock because .git is read-only. These are current observed blockers, not assumptions copied from the previous report. The owner supplied evidence that the preceding GTK UI actually ran and was accepted on Omarchy; this does not establish current native Tauri or P2P behavior.

The continuation uses official Syncthing config/defaults/device/folder and db completion/status documentation, with a no-redirect/proxy loopback client. Changes preserve the existing frontend and separate transport from learning files. Production format and lifecycle compatibility remain intentionally unimplemented rather than inferred from filename/process-name observations.
