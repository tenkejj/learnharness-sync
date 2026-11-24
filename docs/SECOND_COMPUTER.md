# Run this candidate on your other Linux computer

Use the same build on **both** computers. This candidate uses HSQ/2 remote-head
verification; an older HSQ/1 AppImage cannot connect to it. Existing native device
identities and trust pins are kept when you update the application.

## Portable AppImage

The ordinary, no-acceptance-feature candidate is
`target/release/bundle/appimage/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage`
in the repository checkout. Its SHA-256 is
`eb8a4c89369c19ade0761a28d503bad6a4aff0d89331937f7b8282830b924c0c`.
This is a private physical-test candidate, not a published release. Copy only
the AppImage to Downloads on both computers using USB or another trusted
channel. Do not copy application profiles, native stores or backups.

Copy the AppImage to a folder you control on each computer. From that folder:

```sh
chmod +x LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage
printf '%s  %s\n' 'eb8a4c89369c19ade0761a28d503bad6a4aff0d89331937f7b8282830b924c0c' 'LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage' | sha256sum -c -
./LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage
```

No account, Python runtime, external sync service or new background service is
needed. This x86_64 candidate was built and tested on the development Linux
host. It requires glibc 2.39 or newer. GTK/WebKit libraries are bundled; FUSE 2
is optional if you use `--appimage-extract-and-run`. Other distributions, GPU
drivers and architectures are not certified.

For managed LearnHarness synchronization, the host must also provide a Btrfs
filesystem, unprivileged user namespaces with existing subordinate UID/GID
allocations, `newuidmap`/`newgidmap`, bubblewrap, util-linux, btrfs-progs, and
the supported display support (`xauth` plus rootful Xwayland on Wayland, or
Xephyr on X11). Install the pinned LearnHarness 0.6.1 AppImage at
`~/Applications/LearnHarness-0.6.1-x86_64.AppImage`; its expected size and SHA
are recorded in [MANAGED_LINUX.md](MANAGED_LINUX.md). The LearnHarness Sync
managed launcher executes a verified private copy and uses a separate profile.

Verify the copied files with `sha256sum -c SHA256SUMS` if you copy the whole folder.
If a host cannot mount AppImages through FUSE, use the AppImage runtime’s extraction
mode and run `squashfs-root/AppRun` from the extracted bundle. Ubuntu DEB and Fedora RPM verification is recorded in [the qualification report](LEARNHARNESS_061.md).

## Optional synthetic LAN smoke

This checks pairing and the native test engine only; it is not real LearnHarness
data acceptance.

1. Put both computers on the same Wi-Fi or LAN and open LearnHarness Sync on each.
2. Click **Add Device** on both. On one, click **Pair** beside the other computer.
3. Compare **all eight groups** on both screens through a trusted channel. Click
   **Strings match — approve** on both only if the entire string matches.
4. The app returns to Home. Click **Run sync test** once on either computer.
5. Wait for the verified result. It requires actual remote receipt, integrity
   verification and adoption of the expected test revision. A connection alone
   does not count as success.

Keep both apps open during transfer. If discovery is unavailable, choose **Can’t
find your computer?** inside Add Device to use the invitation fallback. Mutual
approval is still mandatory.

Do **not** copy a native profile, `identity.json`, or `devices.json` to the other
computer: each machine needs its own independent identity. Copy only the app or
source archive. No LearnHarness data directory is needed or accepted.

For physical real-data acceptance, follow [TWO_DEVICE_ACCEPTANCE.md](TWO_DEVICE_ACCEPTANCE.md).
Use new managed profiles only. The ordinary LearnHarness profile is not opened
or modified. Do not mark physical acceptance complete until both computers have
reopened LearnHarness and verified A → B → A content and graph readback.

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
