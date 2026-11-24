# Packaged real LearnHarness acceptance, 2026-11-25

Source: `5e413e2460bcecf54c7d827c42517302c8d9aae4` plus a test-harness
change made after this application build. The feature-only acceptance AppImage
SHA-256 was `d25ef22486b8f0f5fc3e5cc93da8b95824ebd44a40f4e0ed3bb9ef51939fd964`.

Command:

```sh
python scripts/test-packaged-managed-roundtrip.py 'src-tauri/target/acceptance/release/bundle/appimage/LearnHarness Sync_0.2.0-alpha.1_amd64.AppImage'
```

**PASS, exit 0.** Two independent packaged Tauri/native peers on one Linux
host used separate disposable HOME, XDG data, native identity and managed
profile stores. The real LearnHarness 0.6.1 application created four messages,
three propositions and two chains on A. Authenticated QUIC delivered six
content records (4,932 bytes) into B's validated quarantine. B's managed apply
was backed up and journaled. The real application reopened B and the test
checked message IDs, propositions, chains and provenance. It then authored a
Unicode follow-up on B. The reverse QUIC transfer and managed apply reopened
the real application on A and verified six messages plus the graph content.

The same run verified active-writer capture refusal, injected ENOSPC after a
durable backup with the old generation retained, helper SIGKILL after the
durable apply switch, explicit recovery to the old generation, process restart,
reconnect and successful retry. It did not simulate physical disk exhaustion
or cold power loss. It did not use a second physical computer. The acceptance
feature is absent from the ordinary AppImage and the feature-enabled artifact
is not a release asset.

Cleanup of the two exact disposable Btrfs vaults from this run was refused by
the kernel with `EPERM`. They remain at
`build/packaged-managed-roundtrip-yrgf2u6o/{a,b}-data/…/slot/vault` and must
only be removed after a separate ownership and subvolume check. No owner
profile or credentials were used.

The ordinary no-acceptance-feature AppImage separately passed the complete
`scripts/test-managed-launcher.py … --host-display` suite after a `/proc`
process-exit race was fixed in the test harness: real LearnHarness generated
four messages, three propositions and two chains; packaged snapshot IPC and
the Tauri WebView command matched independent hashes; malformed content was
refused and recovered; a killed helper recovered and retried; unauthorized
cross-UID access was rejected; active-writer refusal, supervisor termination
and restart passed. Its cleanup also received `EPERM`, retaining the exact
disposable vault at `build/managed-accept-69qulpv6` for separate inspection.

A subsequent **portable** private acceptance AppImage, SHA-256
`2bb7ab89f103e58fe435bf602dfc791642f28cfd73ca4ca257533b395277f70e`,
bundled its test assets and staged them outside the AppImage FUSE mount for
subordinate-UID access. `scripts/physical-managed-acceptance.py prepare`
created a fresh Btrfs-backed disposable HOME and verified that the ordinary
AppImage's test console exited 64. `seed` used the real application to create
four messages, three propositions and two chains; ordinary packaged
`--managed-snapshot` returned six files and 4,932 bytes. `extend` used the real
application again and reported six messages. No repository QA files were
resolved at runtime by these packaged operations. The full
`scripts/test-packaged-managed-roundtrip.py` run against that portable package
also exited 0 with authenticated QUIC A→B→A, application reopen, writer
refusal, injected ENOSPC, SIGKILL recovery and retry. Its two disposable Btrfs
vaults remain at `build/packaged-managed-roundtrip-gg4oanos/{a,b}-data/…/slot/vault`
after cleanup was refused with `EPERM`.

After rebuilding the ordinary AppImage from the updated source, the portable
runner again created a fresh disposable profile using the private acceptance
package. Its independent `verify` command checked every manifest file hash,
conversation and message IDs, three propositions, two chain relationships,
message provenance and four saved layout positions: four messages after
`seed`, then six after `extend`. The ordinary package's launcher acceptance
again exited 0 against this rebuilt artifact, including real application
window reopen and the helper-crash recovery path. No physical second-device
real-data run was performed.
