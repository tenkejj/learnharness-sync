# Run this candidate on your other Linux computer

Use the same build on **both** computers. This candidate uses HSQ/2 remote-head
verification; an older HSQ/1 AppImage cannot connect to it. Existing native device
identities and trust pins are kept when you update the application.

## Portable AppImage

The current private, ready-to-copy folder on the build machine is `release/learnharness-sync-experimental/`. It contains the AppImage, DEB, RPM, SHA256SUMS and the source commit record. Copy this folder intact to Downloads on the second computer. Run `sha256sum -c SHA256SUMS` from inside the copied folder.

Copy the AppImage to a folder you control on each computer. From that folder:

```sh
chmod +x LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage
./LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage
```

No account, Python runtime, external sync service or new background service is
needed. This x86_64 candidate was built and smoke-tested on the development Linux
host. Other distributions, GPU drivers and architectures are not certified.

Verify the copied files with `sha256sum -c SHA256SUMS` if you copy the whole folder.
If a host cannot mount AppImages through FUSE, use the AppImage runtime’s extraction
mode and run `squashfs-root/AppRun` from the extracted bundle. Ubuntu DEB and Fedora RPM verification is recorded in [the qualification report](LEARNHARNESS_061.md).

## Pair and test

1. Put both computers on the same Wi-Fi or LAN and open LearnHarness Sync on each.
2. Click **Add Device** on both. On one, click **Pair** beside the other computer.
3. Compare **all eight groups** on both screens through a trusted channel. Click
   **Strings match — approve** on both only if the entire string matches.
4. The app returns to Home. Click **Run sync test** once on either computer.
5. Wait for the verified result. It requires actual remote receipt, integrity
   verification and adoption of the expected test revision. A connection alone
   does not count as success.

There is no setup page or manual test-note preparation. Keep both apps open.
Closing an app stops its engine. If discovery is unavailable, choose **Can’t find
your computer?** inside Add Device to use the invitation fallback. Mutual approval
is still mandatory. Invitations and diagnostics are unnecessary on the happy path.

Do **not** copy a native profile, `identity.json`, or `devices.json` to the other
computer: each machine needs its own independent identity. Copy only the app or
source archive. No LearnHarness data directory is needed or accepted.

Real LearnHarness synchronization remains disabled. This is a functioning
synthetic LAN test candidate, not a production chat/knowledge-graph sync release.

## Build from source

Enter `learnharness-sync`, install the normal Tauri 2 Linux
GTK/WebKit development and packaging prerequisites, and use Node.js 22+ and Rust:

```sh
npm ci
npm run build
npm test
cargo test --locked -p harness-sync-core -p harness-sync-native
scripts/build-linux-candidates
```

The AppImage is written under `target/release/bundle/appimage/`. Development browser
tests are available through `npm run test:ui` with Chromium installed; see TESTING.md.
Python is only an optional reference-test dependency.

## When something needs attention

- Offline: open both apps on the same LAN; reconnection is automatic.
- After an update: use this same AppImage on both computers.
- Strings differ: reject on both. A partial match is insufficient.
- An interrupted pairing leaves one-sided trust: remove the relationship on both
  computers using Devices → Manage → Remove Device, then pair again.
- Two versions need attention: choose Review conflict. Read the actual preserved
  notes in History. Both branches remain intact; automatic merging is unsupported.
- A test fails: read its result and allow reconnection. Settings → Advanced →
  Diagnostics contains connection and storage details. Never delete a profile to
  make a warning disappear.
- Renderer problems: the app disables WebKit DMA-BUF rendering for itself by
  default. This fixed the reproduced Wayland launch failure on the verified host.
  Explicit environment overrides are respected. No system config is modified.
