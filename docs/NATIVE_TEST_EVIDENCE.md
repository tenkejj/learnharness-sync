# Native migration verification — 2026-11-17

All results below concern invented fixtures and isolated profiles on this Linux x86_64 host. No real LearnHarness application, configuration, session, token, launcher, process or data tree was accessed or changed. No system service was stopped or modified. No commit, push, installer publication or public release occurred. Initial repository state had no HEAD and all source untracked; that remains unchanged.

## Executed successfully

| Command | Actual result |
|---|---|
| `cargo test --locked -p harness-sync-core -p harness-sync-native` | 23 tests passed: 7 pure core, 5 hostile-protocol, 11 native integration/storage tests |
| `cargo clippy -p harness-sync-core -p harness-sync-native --all-targets -- -D warnings` | Passed |
| `cargo clippy -p harness-sync -- -D warnings` | Passed |
| `npm test` | 14 frontend tests passed (11 original tests preserved as reference; current boundary uses native IPC) |
| `npm run lint` / `npm run format:check` / `npm run build` | Passed |
| `python -m unittest discover -s tests -q` | All original 72 Python reference tests passed |
| `cargo build -p harness-sync-native --bin harness-sync-fixture-peer && node scripts/native-peer-e2e.mjs` | TWO OS processes, actual QUIC: mutual pairing, bidirectional transfer, persistent identity/trust, restart/reconnection, offline fast-forward, divergent histories retained |
| `node scripts/native-discovery-e2e.mjs` | Two isolated native processes discovered each other through actual mDNS on this host; no physical second computer |
| `GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 target/debug/harness-sync --smoke-test` | Actual Tauri/WebKit UI, native status/identity, synthetic snapshot, production refusal, Devices navigation and GUI Add Device command passed |
| Same smoke command against `target/release/harness-sync` | Passed |
| `GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 APPIMAGE_EXTRACT_AND_RUN=1 target/release/bundle/appimage/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage --smoke-test` | Packaged application passed; no external engine/Python child |
| `npm run tauri -- build --bundles appimage,deb,rpm` | Successfully built all three Linux candidate packages |
| `python scripts/audit_repository.py` | Targeted scan passed; zero findings; zero staged files/commits in unborn repository |
| `python scripts/audit_native_packages.py` | All three actual packages extracted/scanned: 420 AppImage, 7 DEB, 6 RPM regular files; 8 public-library pattern matches manually verified; no private state/external engine files |
| `npm audit --omit=dev --audit-level=high` | Zero vulnerabilities |
| `cargo audit --json` | Zero known vulnerabilities; THREE informational advisories requiring review, below |
| Impeccable detector on changed UI files | Empty finding list; native window visually inspected without unrelated desktop content |

Test assertions include invalid IDs, wrong pinned certificate, wrong identity, replayed proof across different TLS sessions, refused/rejected peers, unsupported protocol, oversized framing before allocation, corrupted and truncated payloads, no partial manifest commit, repeated delivery/reconciliation, missing/cyclic ancestry, quarantine markers, malformed fixture schemas, unsafe links, exclusive profile lock, storage quota refusal and failed ledger writes. A failed ledger commit leaves the original head and independently verified backup intact. A fixed golden manifest hash verifies deterministic v2 encoding; byte compatibility with Python manifests is not claimed.

Machine-readable local evidence is under `docs/evidence/native-*.json`. It contains synthetic/public identifiers only, no private keys or profiles. Two-process test output also identifies its temporary full report path. The AppImage smoke generates its own fresh profile and exits.

## Failures found and corrected / honest limitations

- The first pairing test exposed premature QUIC close before final ACK delivery. Stream FIN and delivery are now awaited; pairing and subsequent transfer tests pass.
- A test timeout originally matched the five-second reconnect interval; bounded integration waits now cover reconnection. No synchronization decision was changed to force a pass.
- The default Wayland launch failed with GDK Error 71 on this host. XWayland smoke passed. An XWayland run also printed GBM buffer warnings; setting `WEBKIT_DISABLE_DMABUF_RENDERER=1` produced a clean smoke. No firewall, sandbox or system security setting was disabled. Native Wayland renderer compatibility remains unqualified.
- Clippy initially reported two style warnings; both were fixed and warnings-denied checks pass.
- `react-doctor --scope changed` cannot compare an unborn repository with no HEAD. A full scan accidentally included the pre-existing ignored Python build environment and reported an unrelated pip command-execution diagnostic. A focused scan of the actual three UI source files reports only three remaining warnings in the incumbent large App component: complexity, size and an append-only timeline key containing an index. No security error remains in that focused scan. No rule was suppressed and no agent-created GitHub issue was posted.
- Cargo audit reports RUSTSEC-2024-0429 (GLib 0.18.5 VariantStrIter unsoundness), RUSTSEC-2026-0543 (chrono unmaintained) and RUSTSEC-2024-0370 (proc-macro-error unmaintained). GLib and proc-macro-error are in the active Tauri/GTK dependency graph; chrono is an optional serde_with dependency present in the lockfile and absent from the active Linux tree. The application does not directly use VariantStrIter, but reachability/mitigation is NOT certified. No advisory was ignored or silently marked resolved.
- The first extracted-package scan flagged an AWS-prefix pattern in libunistring's public Unicode-name dictionary. Manual inspection confirmed concatenated dictionary words at offset 972850, also present in the host library. The scanner permits only that exact bundled library SHA-256/offset/context and records the reviewed false positive. Seven other PEM-pattern findings in libgnutls were byte-for-byte confirmed as [public upstream cryptographic self-test vectors](https://github.com/gnutls/gnutls/blob/master/lib/crypto-selftests-pk.c). The scanner permits only those exact reviewed public match hashes in the exact library build and records every finding. Other credential matches still fail.
- `python scripts/check_release.py` intentionally exits 1. Production writer/schema, installer installation, physical-device, artwork and publication gates remain false. Only the genuine native two-process gate is now true.

## Package scope

Candidate paths (not published):

- `target/release/bundle/appimage/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage`
- `target/release/bundle/deb/LearnHarness-Sync_0.2.0-alpha.1_amd64.deb`
- `target/release/bundle/rpm/LearnHarness-Sync-0.2.0-alpha.1-1.x86_64.rpm`

The standalone release executable is `target/release/harness-sync`. These packages are development candidates, not signed or independently installed releases. Extracted package scan results/checksums are in `docs/evidence/native-package-audit.json`. No Python, Syncthing, sidecar executable, fixture-peer executable, private identity or engine profile may be included. The artifact scan is targeted, not exhaustive certification.

## Still pending

Two physical computers and actual installer installation; full GUI pairing between physical devices; production writer exclusion and real schema/rollback qualification; forced power-loss/real ENOSPC tests; independent hostile-network review; Windows/macOS ACL/durability/installer tests; multi-interface invitation fallback; dependency advisory/license and brand redistribution review. Internet NAT traversal, relays, QR pairing, conflict merging, tray and background daemon behavior are not implemented.

**Public beta is not ready.** The repository is a working native synthetic LAN MVP with explicit production safety gates, not a completed production LearnHarness integration.
