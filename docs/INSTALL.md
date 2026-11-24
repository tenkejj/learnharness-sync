# Install and build

Linux x86_64 is the initial target. Candidate package build and smoke evidence is recorded in LEARNHARNESS_061.md. No Windows/macOS installer is claimed. Nothing has been published.

Build with stable Rust, Node.js 22+, npm and normal Tauri 2 GTK/WebKit/system packaging dependencies already available on the build host:

```sh
npm ci
npm run build
scripts/build-linux-candidates
```

The command builds only native Rust application code and the frontend. Python, PyInstaller, Syncthing and Iroh are not involved. Packages do not bundle the development fixture-peer executable. Install the appropriate locally reviewed package using your usual desktop package workflow. Installation on a second physical machine remains pending owner verification.

Launch the same candidate on both computers, click Add Device on Home on each, select the nearby computer and compare all eight verification groups. Approve on both screens. Home offers Run sync test; all isolated preparation and invented data are automatic. Do not copy real LearnHarness data into it. The default profile is the OS app-data directory `community.harnesssync.desktop/native-v2` (Linux XDG data home, typically `~/.local/share`). The old Python configuration is neither migrated nor read.

For completely separate test instances, start the binary with `HARNESS_SYNC_TEST_PROFILE=/absolute/new/private/path`. The chosen directory must be new/empty or already have the native synthetic marker. It cannot adopt a nonempty ordinary data directory. Each profile generates its own identity and persistent UDP port. Do not copy one profile between devices: that would duplicate identity.

## Troubleshooting

- **No nearby devices:** open Add Device on both, verify the same LAN, and try a pairing invitation. Wi-Fi client isolation or blocked UDP multicast can prevent discovery. Do not disable your firewall; review a specific application rule if needed. No internet relay is used.
- **Invitation does not connect:** both apps must be open, the selected numeric address must be reachable, and the app's UDP port must be allowed by your network policy. Multiple VPN/interfaces can advertise an unsuitable first address; use a common LAN. Port/address editing and automatic multi-address fallback are not implemented.
- **Strings differ:** reject on both. Do not approve a partial match. Try again on a trusted network.
- **One side paired after an interrupted final acknowledgment:** remove the relationship on both and pair again.
- **Offline:** keep both applications open. Trusted peers reconnect automatically. A Connected label means an authenticated connection, while last verified exchange reports completed synchronization separately.
- **Conflict:** both branches are retained. Do not delete object files or select a branch by timestamps. Review History → preserved versions and RECOVERY.md. Automatic merge/production import is unavailable.
- **Profile busy:** another app instance owns this profile. Use a separate test profile or close the other LearnHarness Sync instance yourself.
- **Storage error/quota:** no successful import is claimed. Retain the whole profile; check free disk space and diagnostics. Automatic history deletion is intentionally absent.
- **Linux renderer fails:** this candidate defaults `WEBKIT_DISABLE_DMABUF_RENDERER=1` for its own process, avoiding a reproduced WebKit Wayland protocol error on the development host. An explicit existing value is respected. If your driver still fails, try `GDK_BACKEND=x11` for this app under XWayland; no system configuration change is needed.

Closing the window ends the application and stops sync. There is no system service, tray or background daemon to configure. Historical Python installers under scripts/legacy-reference are not installation instructions for this build.

Earlier migration smoke required external renderer environment settings. The redesigned candidate is tested separately with its built-in per-process default; see UX_TEST_EVIDENCE.md. Other Linux distributions and drivers still require acceptance. See SECOND_COMPUTER.md for portable AppImage instructions.
