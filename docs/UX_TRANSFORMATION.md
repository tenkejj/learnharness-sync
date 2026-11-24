# Desktop UX transformation

This is a working synthetic-data candidate. Real LearnHarness import/export stays
blocked. The owner reported a successful two-physical-Linux-device native pairing
and note transfer before this redesign; the redesigned candidate still needs that
same physical-device acceptance. Local automated peers do not replace it.

## Audit and disposition of the old interface

| Previous surface or control | Implemented disposition |
| --- | --- |
| Overview and its generic headline | Home begins with the actual connection or conflict state. |
| Permanent Setup destination and checklist | Removed. Discovery is contextual; fixture preparation is automatic. |
| Devices combining discovery, invitations, trust and fingerprints | Paired-device cards; Add Device opens focused discovery. |
| Separate Activity, Backups and Conflicts tabs | Combined into History, with conflict review placed first when needed. |
| Settings as a main navigation destination | Gear opens one accessible dialog. |
| Refresh status on every page | Live native status plus fallback polling; retry appears on engine failure. |
| Create workspace and invented-data confirmation | One-click test uses only engine-generated harmless data in a marked isolated profile. |
| Note textarea, Save, export and raw test counters | Automated test on Home. Manual note editing remains in opt-in Developer Mode. |
| Network port, addresses, identity, transport and profile paths | Settings → Advanced → Diagnostics. |
| Raw pairing fingerprint | Removed from the happy path; full session verification is retained. |
| Invitation generation/paste | Revealed by “Can’t find your computer?”; never needed on the discovery happy path. |
| Accept/reject pairing | Preserved, with all eight code groups and explicit approval on both screens. |
| Remove device beside every technical detail | Manage menu, named device, explicit confirmation and retained history. |
| Local name edit | Settings or pencil on the local device; host name chosen automatically on first launch. |
| Remote name management | Manage device → name; saved locally without changing the trust pin. |
| Snapshot hashes/manifests | Hidden inside explicit Technical details for a history entry. |
| Backup selection and exact restore phrase | Backup entry → Restore; protections retained, current version backed up. |
| Conflict branches | Warning on Home → History → actual preserved notes; no silent merge or branch deletion. |
| Limitations and production safety warnings | Routine details in Advanced; production-disabled and open-app requirements always visible. |
| Engine-unavailable warning | Actionable recovery state; stale content cannot masquerade as a live success. |

## Measured journeys

Counts exclude opening the application and include intentional clicks per computer.

- New user: Add Device → Pair nearby computer → approve matching string: **3 clicks
  on the initiating computer**, **2 on the receiving computer**. Both eight-group
  comparisons are still required. No separate workspace setup or tab change.
- Returning user: open → read Home: **0 clicks**.
- Synthetic test: Run sync test → evidence-based result: **1 click**. No editor,
  save, export, hash comparison or setup on the receiver.
- Offline device: read Home and allow automatic reconnection: **0 clicks**.
- Conflict: Review conflict → expand preserved version: **2 clicks** to read it;
  **1 more** to inspect the other version. Both remain intact.
- Diagnostics: Settings gear → Advanced → Diagnostics: **3 clicks**.

## Native evidence

HSQ/2 adds `Committed {head, received, conflict}` over the authenticated connection,
after payload verification, complete history validation and durable reconciliation.
The initiating engine requires `head == received == expected_test_revision`, no
conflict and an unchanged matching local head. Empty connections cannot satisfy it.
Each test uses a unique harmless nonce. Results and last receipts persist in the
isolated profile. Late acknowledgments can settle pending results after a restart.

Both computers must run this candidate: HSQ/1 builds are deliberately incompatible.
Identities and trusted certificates remain in the existing native-v2 profile, so
updating both computers does not require a new identity. Interrupted or asymmetric
pairing may still need removing the relationship on both sides before re-pairing.

## Verification and remaining limits

See UX_TEST_EVIDENCE.md for exact results and screenshots. No production data was
read, no system configuration was edited, and no service or Python runtime was
added. No commit, push, publication or release was created.

Conflicts can be read and retained; automatic merging is intentionally unsupported.
Only the latest test has a dedicated result record; ordinary immutable history,
activity and backups are retained separately. IPv4 common-LAN reachability is
required; invitations still select the first advertised address. There is no tray,
autostart, relay or background service. Device-name changes refresh discovery after
restarting the app. Other Linux distributions and physical machines need acceptance;
Windows/macOS and production compatibility are untested.
