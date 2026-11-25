# Copy LearnHarness Sync to a second Omarchy computer

Use the **same checksummed candidate** on the PC and laptop. This is a private physical-test package, not a public release. The transfer folder is `build/physical-070/` in the development checkout. Its six files are:

| File | Purpose |
|---|---|
| `LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage` | Ordinary desktop candidate; no test console |
| `LearnHarness-Sync_0.2.0-alpha.1_PRIVATE-ACCEPTANCE.AppImage` | Private offline acceptance commands; never publish |
| `LearnHarness-0.7.0-x86_64.AppImage` | Exact reviewed upstream build for disposable profiles |
| `physical-managed-acceptance.py` | Creates and checks isolated test HOME/XDG roots |
| `learnharness-builds.json` | Reviewed 0.6.1/0.7.0 build and schema matrix |
| `SHA256SUMS` | Hashes of the five files above |

Copy the **entire folder** by trusted USB storage or, if SSH is already available, from the PC with:

```sh
scp -r "$HOME/Projects/harness-sync/build/physical-070" USER@LAPTOP_IP:~/Downloads/learnharness-sync-physical-070
```

Replace `USER@LAPTOP_IP` with the laptop's actual SSH destination. Do not enable a new system service just for this test. The PC can run from its checkout's `build/physical-070/`; the laptop uses `~/Downloads/learnharness-sync-physical-070/`. On **both** computers:

```sh
cd /path/to/learnharness-sync-physical-070
sha256sum -c SHA256SUMS
chmod +x LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage
chmod +x LearnHarness-Sync_0.2.0-alpha.1_PRIVATE-ACCEPTANCE.AppImage
```

Compare the SHA-256 of `SHA256SUMS` with the value in the owner's handoff before trusting the copied folder. Keep this folder private: the acceptance AppImage is feature-enabled and must never be uploaded to GitHub Releases. It can only create test content inside a fresh disposable HOME through the runner. Do not copy application profiles, pairing keys, native stores or backups between computers.

For the real 0.7.0 A → B → A procedure, follow [TWO_DEVICE_ACCEPTANCE.md](TWO_DEVICE_ACCEPTANCE.md). It invokes both AppImages with separate disposable managed profiles and verifies the exact upstream AppImage against [the reviewed matrix](UPSTREAM_COMPATIBILITY.md). The ordinary personal LearnHarness profile is never selected. To test 0.6.1 separately, obtain its exact reviewed AppImage and choose `--version 0.6.1`; never mix the two versions in one acceptance run.

## Ordinary desktop prerequisites

The x86_64 AppImage needs glibc 2.39 or newer. GTK/WebKit libraries are bundled. If FUSE 2 is unavailable, use `--appimage-extract-and-run`. Managed LearnHarness operation additionally needs Btrfs, unprivileged user namespaces with subordinate UID/GID allocations, `newuidmap`/`newgidmap`, bubblewrap, util-linux, btrfs-progs, and display support (`xauth` plus rootful Xwayland on Wayland, or Xephyr on X11). The runner checks Btrfs and keeps its HOME/XDG paths under a new private root. It does not modify the normal LearnHarness installation or system services.

For a quick **synthetic** LAN check only, open the ordinary AppImage on both devices, use **Add Device**, compare all eight pairing-string groups, approve on both sides, then click **Run sync test**. A connection without a verified remote receipt is not success. The owner has already reported this synthetic bidirectional test as passed on the two physical machines; real LearnHarness content still needs the separate acceptance procedure.

If discovery fails, use **Can't find your computer?** in Add Device. If pairing strings differ, reject on both sides. If a transfer fails, keep the disposable roots and record the exact message; never delete a profile to clear a warning. Settings → Advanced → Diagnostics contains technical details. Other distributions, architectures, full-volume exhaustion and cold power loss remain unqualified.
