# Redesigned desktop candidate — verification evidence

Date: 2026-11-17. Linux x86_64 development host. The application remains an
experimental synthetic-only candidate; public release and production access are
not authorized. This report describes checks actually performed in this session.

## Required checks

| Command / check | Result |
| --- | --- |
| `npm run build` | PASS: TypeScript and Vite production bundle. |
| `npm run lint` | PASS: strict unused-local/parameter TypeScript checks. |
| `npm run format:check` | PASS: Prettier across source and frontend/browser tests. |
| `npm test` | PASS: 21 native-bridge, state, malformed-response, event and safety tests. |
| `cargo test --locked -p harness-sync-core -p harness-sync-native` | PASS: 7 core + 5 protocol + 13 native integration/storage tests; doc tests passed. |
| `node scripts/native-peer-e2e.mjs` | PASS: two independent real QUIC processes; mutual pairing, verified one-click test, bidirectional transfer, persistence, reconnect, offline continuation and divergence. |
| `node scripts/native-discovery-e2e.mjs` | PASS: actual multicast discovery between two isolated native processes on this host. |
| `npm run test:ui` | PASS: 8 actual-native-backed browser journeys. |
| `python -m unittest discover -s tests -q` | PASS: all 72 preserved reference tests. Python is not a runtime dependency. |
| `npm run tauri -- build --bundles appimage,deb,rpm` | PASS: fresh AppImage, DEB and RPM private candidates. |
| Packaged AppImage `--smoke-test` | PASS: actual WebKit UI, native commands, font, navigation, production/offline refusal, readback, pairing dismissal and Settings. |
| Packaged AppImage `--visual-test` | Visually inspected actual isolated Tauri window on Wayland; screenshot retained. |
| `python scripts/audit_repository.py` | PASS: targeted source scan, no findings; repository has no commits/index entries. |
| `python scripts/check_release.py` | EXPECTED BLOCK: production adapter/writer exclusion, physical candidate acceptance, installer qualification, artwork rights and publication authorization stay closed. |

## Real one-click evidence

A test starts with no synthetic initialization on either native peer. It generates
a unique harmless note, commits it, transfers through real QUIC, and succeeds only
when the authenticated remote receipt reports both verification and adoption of
that exact head. Native tests assert the receiver's active head and received
objects, the sender's matching receipt and persistent result. A simultaneous second
request is rejected. A receiver's WRITER_ACTIVE marker cannot produce success.
Separate offline branches stay preserved and refuse another test.

The browser suite uses two real fixture-engine processes behind the same typed
bridge as the desktop. It verifies discovery and approval on both screens,
automatic Home return, exactly one native test call for a double click, actual
remote head adoption, refusal/reconnection, offline state, read-only conflict-note
review, named devices, management confirmation, keyboard dialog containment,
Escape, focus restoration, hidden diagnostics and reduced motion. No seeded peer,
fabricated transfer, fake acknowledgment or test-success response is used.

Connection liveness and receipts are separate. Saved receipts are withheld from a
new live connection until its own authenticated exchange completes. Unchanged
heartbeats retain the last meaningful exchange time instead of rewriting it every
three seconds. Late matching acknowledgments can finish a pending result.

## Visual inspection

Browser screenshots inspected at 1180×800, 900×680, 720×600 and 420×740. Additional
720px checks used 200% text scaling. Inspected states: no devices, actual nearby
discovery, full eight-group pairing, connected, running/success/failed test,
offline, preserved conflict and actual note review, Devices, Settings/Advanced.

Refinements from inspection: responsive top navigation, fixed safety footer,
independent content scrolling, smaller topology spacing at laptop sizes, permanent
modal title/close control, explicit keyboard focus containment, readable full code,
wrapped identities/paths and self-hosted font. All eight verification groups and
approval/rejection controls fit the 720×600 pairing window. At 200% text, content
scrolls while navigation and the production-disabled footer remain reachable.

Screenshots are in `docs/evidence/ux/`. Browser captures are actual React UI driven
by native peers. `tauri-appimage-wayland.png` is the actual packaged WebKit window,
using a new isolated profile. No other desktop window is included in that capture.

The first unmodified Wayland AppImage smoke failed with GDK protocol error 71.
`WEBKIT_DISABLE_DMABUF_RENDERER=1` fixed it. The application now sets that variable
for its own process before GTK starts, only when no explicit override exists. The
fresh packaged smoke then passed without external environment settings. No system
configuration or services were modified. This is not cross-driver certification.

## Tool findings and practical limits

React Doctor's changed-scope mode could not run because this repository has no
HEAD commit. Full scans were run. Its security error points into the pre-existing
local `.venv-build` copy of pip, outside desktop source/runtime. Its remaining
React observations concern pairing state reconciliation after native events,
static code-group keys and component complexity; they are not a clean accessibility
certification. No findings were hidden through tool configuration. Existing npm
audit findings remain in the Vite/picomatch development chain (4 high-severity
entries); no forced downgrade was applied. Dependency/license review remains a
release gate.

There was no physical second machine available to this agent. The owner reported
prior physical native success; the new HSQ/2 candidate must still be accepted on
both physical computers. DEB/RPM builds were not installed. No Windows/macOS test,
power-cut qualification, formal screen-reader audit, independent security review
or real LearnHarness compatibility test is claimed. Conflict merging, Internet
relays and a background lifecycle remain unsupported.

## Delivery

The fresh packaged AppImage is under
`target/release/bundle/appimage/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage`.
A ready-to-copy folder with that AppImage, DEB/RPM alternatives, clean source and
checksums is under `release/harnesssync-ux-linux/`. See SECOND_COMPUTER.md.
No commit, push, remote release or public artifact publication was performed.
