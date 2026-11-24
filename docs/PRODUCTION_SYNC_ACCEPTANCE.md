# Packaged managed-sync acceptance

**Result: PASS on the Linux x86_64 build host, 2026-11-24.** The test used
disposable HOME, XDG, native-store and subordinate-UID Btrfs profiles for both
peers. It did not open or modify the owner’s normal LearnHarness profile.

## Real application round trip

The real LearnHarness 0.6.1 AppImage created a managed profile on A containing
one conversation with four messages, three graph propositions, two chains and
message provenance. Two independent packaged LearnHarness Sync peers used
separate native identities and stores, paired through the authenticated QUIC
protocol, and transferred the captured content into B’s verified quarantine.
The receiver verified the manifest, file inventory and content hashes before
the test activated it.

B reopened the real LearnHarness application and confirmed the conversation,
message IDs, proposition IDs, chain IDs and provenance. LearnHarness on B then
created a Unicode follow-up and exited cleanly. B captured and transferred that
revision back to A. A reopened the real application and verified the complete
six-message conversation and the graph records. Capture requests while either
real LearnHarness window was open were refused.

## Packaged transaction fault tests

The feature-only acceptance AppImage exercised the same Tauri dispatcher and
managed helper used by the ordinary application. Its acceptance console and
fault injection require both a separate Cargo feature and an explicit runtime
environment variable. The ordinary AppImage does not contain that console; its
`--managed-acceptance-peer` invocation exits with code 64.

- ENOSPC injected after the independent backup was flushed: Apply refused, the
  active generation remained unchanged, the backup remained available, and
  retry was possible.
- Subordinate helper sent SIGKILL after the durable intent and generation
  switch: the packaged peer reported recovery required; explicit recovery
  restored the prior generation; the authenticated connection resumed and a
  later Apply/readback succeeded.
- Active LearnHarness writer: capture was refused until the real application
  closed and the managed profile saved.
- Malformed content and failed application transactions: covered by the full
  packaged managed-launcher acceptance suite; prior generation remained
  available and capture retry verified the immutable content.
- Conflicting same-identity edits remain refused. A unique local-only
  conversation absent from the incoming inventory is preserved by the managed
  merge regression test.

ENOSPC is injected at the transaction boundary; this is not a claim that a
physical device was filled to capacity. The helper kill is an actual SIGKILL,
but it is not a cold power-loss test. Both computers were separate packaged
peers on one physical host, not two physical laptops.

## Reproduction

From the repository root, with the pinned LearnHarness 0.6.1 Linux x86_64
AppImage at `build/qualification/real-sync/LearnHarness-0.6.1-x86_64.AppImage`
and a usable local desktop session:

```sh
CARGO_TARGET_DIR=target/acceptance npm run tauri -- build --features managed-acceptance --bundles appimage
python scripts/test-packaged-managed-roundtrip.py \
  'src-tauri/target/acceptance/release/bundle/appimage/LearnHarness Sync_0.2.0-alpha.1_amd64.AppImage'
npm run tauri -- build --bundles appimage
python scripts/test-managed-launcher.py \
  'target/release/bundle/appimage/LearnHarness Sync_0.2.0-alpha.1_amd64.AppImage' \
  'build/qualification/real-sync/LearnHarness-0.6.1-x86_64.AppImage' --host-display
```

The feature-enabled acceptance AppImage is a test artifact and must not be
distributed. The final command qualifies the ordinary no-feature AppImage’s
managed launcher, Tauri snapshot IPC and safety refusals. This work does not
qualify physical two-laptop network conditions, actual full-volume behavior,
cold power loss, every Linux distribution, or redistribution of bundled
third-party libraries. The host refused deletion of generated Btrfs test
subvolumes with `EPERM`; those exact disposable test vaults remain for
administrator cleanup.
