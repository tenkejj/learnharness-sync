# LearnHarness build and data compatibility

LearnHarness Sync treats executable identity, durable data shape and peer wire format as separate decisions. The reviewed build catalog is [`support/learnharness-builds.json`](../support/learnharness-builds.json). Its SHA-256 and size entries identify exact upstream AppImages. The schema source hashes cover the chat, graph and source-organization contracts. They are evidence for review, not permission to run an unknown binary. The native validator still examines every durable record and refuses unknown inventory.

| Upstream AppImage | Exact SHA-256 | Supported managed data | Wire format | Vault | Evidence |
|---|---|---|---|---|---|
| 0.6.1 Linux x86_64 | `b8a033eb7421677893d96e4988b30153fdfe4fc6f0dc456f7947161bba041b98` | Text-only `custom:qa-model` conversations, supported graph records, provenance and layout | `learnharness-0.6.1-disposable-v1` | `vault` | [Packaged real-data acceptance](PRODUCTION_SYNC_ACCEPTANCE.md) |
| 0.7.0 Linux x86_64 | `772b5e04d380cbe8a72a75dbff269aeb6a47f4d4452711b1da8059a872067613` | Same strict subset, plus verified chat folders and placements | `learnharness-0.7.0-disposable-v1` | `vault-070` | [0.7.0 packaged qualification](evidence/learnharness-070-compatibility-2026-11-25.md) |

Both builds use the same upstream chat version 2, graph version 1 and source-organization version 1 for this subset. They deliberately use different peer formats and vaults. An incoming revision for one cannot be applied to the other. The original 0.6.1 vault, trusted peer identities and quarantined revisions remain at their established paths. Other providers, attachments, imported notes, plugin/job records, historical graph compaction and arbitrary existing profiles remain unsupported.

## Detection and pinned execution

At startup, when Settings opens, and before every managed operation, the desktop checks `~/Applications` and its private pinned-image cache for a catalogued executable. A version label or a matching data schema alone is insufficient: the size and full SHA-256 must match. The highest verified version is selected; an unverified newer AppImage is reported but never executed in the managed profile. If the user retains an older verified build, LearnHarness Sync keeps a private, hash-checked copy in its own application data so a later upstream install does not make that build unavailable. The copy stays outside the subordinate-UID vault. It is not sent to peers or bundled with LearnHarness Sync.

The supervisor copies the pinned executable to a temporary extraction directory, hashes the private copy again, and mounts it read-only at `/app` inside the offline LearnHarness namespace. The namespace has no network access, so an in-app update cannot fetch or replace its executable. The managed vault records the executable digest under subordinate-UID custody. If a future reviewed build has a different digest, the existing vault refuses it until a separate migration qualification and explicit upgrade flow have been implemented. Legacy vaults without a digest marker bind only to their original 0.6.1 or 0.7.0 digests. A failed check leaves the profile, backups, trusted peers and received revisions in place.

The upstream 0.6.1 and 0.7.0 source packages identify their license as AGPL-3.0-only. LearnHarness Sync retains only the user's own verified AppImage locally; it does not redistribute or bundle that executable. This private copy keeps a managed profile on its qualified build.

**There is no production cross-version profile migration.** A 0.6.1 vault is not opened by 0.7.0. Retaining both builds gives two independent managed profiles, not an automatic upgrade. Testing a disposable 0.6.1 clone under 0.7.0 established a read-only compatibility observation, not permission to migrate a user's profile. This limitation protects existing data while the upgrade transaction remains unqualified.

## Future release qualification

The [upstream monitor workflow](../.github/workflows/learnharness-upstream-monitor.yml) runs weekly and on manual dispatch with read-only repository permissions. It reads the official GitHub release asset digest and tagged source, compares the focused schema hashes, reruns the strict native tests, and performs an offline, disposable AppImage extraction probe for a new asset. It uploads a report. It does not change the catalog, publish a binary, open a production vault or mark a candidate supported. GitHub can delay scheduled runs and can disable them after prolonged repository inactivity; manual dispatch remains available. The extraction probe can fail if the runner does not permit user namespaces, which must be investigated rather than bypassed.

To qualify a future release:

1. Review the upstream tag, artifact provenance and complete diff of durable stores, message/graph contracts, provider serialization and migration logic. A matching hash of the focused files is a useful signal, not a full proof.
2. Download the upstream AppImage to an ignored disposable directory; verify its published digest and version. Run it with disposable HOME/XDG data inside a network-disabled namespace. Never use the owner's ordinary profile.
3. Create realistic supported conversations and graph records with the real application, close it, validate every file and relationship, then reopen and compare exact content and IDs. Inject malformed records, unsupported provider/tool/attachment data and interrupted writes.
4. Test a copy of the prior managed profile under the candidate build. Make an independent immutable backup before allowing any migration. Test crash and disk failure recovery on the copy. Keep the original vault untouched.
5. Run packaged A→B→A over authenticated QUIC, version-mismatch refusal, concurrent writer refusal and application readback on independent disposable profiles. Run the physical two-device real-data acceptance before a public compatibility claim.
6. Only after manual review, update the catalog and any required parser/wire/vault code in a reviewed commit. A future build with an unchanged schema still needs exact-binary qualification and a separately tested vault upgrade transaction. Do not change the matrix as a substitute for those tests.

The current public-source state has no qualified profile-upgrade command and no public binary release. The new monitor is a notification and preliminary test gate, not an automatic compatibility updater.
