# Desktop + laptop acceptance for the redesigned candidate

The owner reported native pairing and synthetic transfer between two physical
Linux computers before this redesign. This session verifies the new candidate
with local native peers and the packaged app; its physical two-computer acceptance
is still pending. Never select or alter a real LearnHarness data directory.

1. Copy the same candidate AppImage to both computers; record its SHA-256. Each
   generates its own identity. Do not copy profiles or change any existing service.
2. Launch normally. Home should immediately explain the connection state. Names
   can be changed through Settings (local) or Devices → Manage (remote).
3. Click Add Device on both. Confirm nearby discovery; record any invitation
   fallback separately. Pair on one side, compare all eight groups, reject once
   and verify that no trust is granted. Repeat and approve on both screens.
4. Confirm automatic return to Home and the correct two named computers. Click
   Run sync test once. Verify it reports actual adoption on the receiver, not just
   a connection. No setup or manual note creation should be necessary.
5. Run the test from the other computer. Confirm a verified result and retained
   backups under History when previous test data is replaced.
6. Close the laptop application. Desktop must say offline without extra clicks.
   Reopen laptop: identities and trust remain; reconnection is automatic. Home
   should recover its state from real receipts without manual refresh.
7. Optional advanced synthetic test: enable Developer Mode in Settings → Advanced.
   Close both apps, open only desktop, save an invented note, close it; open only
   laptop, save a different invented note; then reopen desktop. Each must preserve
   its own branch and show Two versions need attention. Review both notes in
   History. No automatic branch selection or merge should occur.
8. Restart both and verify preserved conflicts, notes and backups. For an optional
   synthetic backup restore, expand a backup in History, choose Restore and enter
   the exact confirmation phrase. A new revision and prior-state backup must remain.
9. Remove each relationship through Devices → Manage → Remove Device. Confirm
   sharing stops and retained histories remain available.
10. Confirm production operations stay blocked and the app never needed access to
    LearnHarness, credentials, a system service, Python or an external sync engine.

Record OS/kernel, GPU/session type, network type, package checksum, each outcome and
recovery messages. Use screenshots of the isolated app only; do not include private
profiles, invitation strings, real sessions or unrelated windows. Do not clear a
release gate without real evidence. Installer qualification is a separate gate.
