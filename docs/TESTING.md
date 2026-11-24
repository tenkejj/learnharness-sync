# Test the native migration

The real LearnHarness 0.6.1 qualification suite and its packaged backup inspection check are documented in the [managed Linux report](MANAGED_LINUX.md).

```sh
cargo test --locked -p harness-sync-core -p harness-sync-native
npm test
npm run lint
npm run format:check
npm run build
npm run test:peers
node scripts/native-discovery-e2e.mjs
npm run test:ui
python -m unittest discover -s tests -q
python scripts/audit_repository.py
python scripts/audit_native_packages.py
python scripts/check_release.py
```

The last release-gate command is EXPECTED to fail while production, physical-device, installation and rights gates remain unverified. Python is required only for preserved behavioral-reference tests and optional audit scripts, never for building or running LearnHarness Sync.

Rust tests use private temporary profiles and actual local QUIC endpoints. They verify mutual consent, wrong/replayed identity proof, untrusted/rejected devices, protocol mismatch, oversized frames, corruption, interruption, bidirectional transfer, causal fast-forward, offline branches, repeated reconciliation, missing/cyclic ancestry, backups, restore, failed disk writes, unsafe links, profile locking and identity persistence. A separate Node harness starts TWO independent Rust processes and tests persisted identity/trust, transfer, restart/reconnection, offline continuation and divergence. It prints an actual temporary JSON report path. Neither harness is included in installers.

`target/debug/harness-sync --smoke-test` launches the actual Tauri/WebKit window with a fresh temporary profile, invokes native commands from JavaScript and exercises Devices navigation. It writes a report and exits. The test can use `GDK_BACKEND=x11` on hosts with a Wayland renderer protocol failure; it does not modify system configuration. Headless tests do not substitute for this native check.

The old 72 Python tests remain intact. Original HTTP frontend tests are preserved as `tests/legacy_frontend.test.mjs`; current frontend tests retain applicable validation/failure properties using the native bridge. HTTP/session-token behavior is intentionally retired and no longer tested as production functionality.

See NATIVE_TEST_EVIDENCE.md for actual commands/outcomes, TWO_DEVICE_ACCEPTANCE.md for the pending physical test, and RELEASE_READINESS.md for remaining blockers. No real LearnHarness data is accessed by these tests. ENOSPC, power-loss, cross-platform private ACLs, hostile network review and real LearnHarness schema qualification remain unproven.


## Redesigned desktop journeys

`npm run test:ui` starts Vite and two independent native fixture engines in new
temporary profiles. Playwright drives the actual React interface through a bridge
to those engines; status and transfer results are real, not seeded demo fixtures.
It checks all three destinations, first launch at 1180/900/720/420px, full mutual
pairing, automatic Home return, one-click remote adoption, duplicate-click guards,
write refusal, offline state, conflict preservation and review, settings, Tab
containment, Escape, focus restoration and motion preferences. Screenshots go to
`/tmp/harnesssync-ux-evidence` (override HARNESS_SYNC_UI_EVIDENCE).

Chromium is required for this development-only suite. It uses `/usr/bin/chromium`
when available, HARNESS_SYNC_CHROMIUM if supplied, or Playwright’s installed
Chromium (`npx playwright install chromium`). No browser automation dependency is
included in the desktop runtime. `npm test` remains the fast bridge/state unit suite.

The current AppImage `--smoke-test` checks the real packaged WebKit renderer,
self-hosted font, three navigation destinations, native fixture creation/readback,
production/offline refusal, discovery opening/dismissal and Settings. It writes a
new isolated report and exits. The Linux application sets a conservative renderer
variable for itself before GTK starts; no shell override or system edit is needed
on the verified host. See UX_TEST_EVIDENCE.md for actual outcomes.
