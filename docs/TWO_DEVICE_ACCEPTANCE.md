# Physical two-computer managed-sync acceptance

**Status: pending.** Do not mark this gate complete from loopback peers or from
the packaged acceptance run on one host. Record results from both physical
Linux computers and use only new disposable managed LearnHarness profiles.

## Before starting

- Install the same candidate AppImage on both x86_64 Linux computers and verify
  the supplied SHA-256. Record the checksum and each computer’s OS, kernel,
  display type and network.
- Install the pinned LearnHarness 0.6.1 AppImage at
  `~/Applications/LearnHarness-0.6.1-x86_64.AppImage` on both computers. The
  managed launcher executes a verified private copy in its isolated profile;
  it does not open the ordinary LearnHarness profile.
- Confirm each host has Btrfs, unprivileged user namespaces, subordinate
  UID/GID mappings, `newuidmap`/`newgidmap`, bubblewrap, util-linux, btrfs-progs
  and the required display support. Do not add system services or alter
  existing profiles to satisfy a failed prerequisite.
- Keep existing LearnHarness data, LearnHarness Sync device identities and
  backups intact. Do not copy `identity.json`, trust files or a profile between
  computers. Do not enter a personal provider credential into a test profile.

## Run the real A → B → A test

1. Launch LearnHarness Sync on both computers. Verify each shows the same
   candidate version and its own device name.
2. In Settings, open **Managed LearnHarness** on both computers. Confirm the
   session is separate from the normal LearnHarness profile. Use only newly
   created disposable content. If the offline managed app cannot create a
   completed conversation, stop here and report that blocker; do not migrate or
   copy a normal profile to make the test proceed.
3. On A, create a conversation with multiple user and assistant messages, a
   Unicode phrase, and supported knowledge-graph propositions and chains.
   Close LearnHarness cleanly. Check the managed snapshot and note its file and
   byte counts.
4. Pair A and B using **Add Device**. Compare every verification-string group
   in person and approve on both computers.
5. On A, explicitly authorize sending the managed conversation to B. Confirm
   that B reports a verified staged copy. The receipt must not claim the data
   was applied.
6. On B, inspect the revision and explicitly authorize **Apply verified
   revision**. Wait for the backup confirmation. Reopen LearnHarness from the
   managed session, verify the conversation, messages and graph, then close it
   cleanly. LearnHarness Sync must report application readback verified.
7. Create a new message and graph update on B. Close LearnHarness cleanly,
   explicitly authorize the B → A transfer, and apply it on A with the same
   backup and readback checks. Verify the original and new content together.
8. While LearnHarness is open on either side, attempt **Check managed
   snapshot**. It must refuse and identify that the writer is active. Close the
   app cleanly and retry; capture should then succeed.
9. Restart both LearnHarness Sync applications. Confirm device trust, histories
   and backups remain present. Do not remove existing device relationships or
   restore unrelated profiles during this test.

## Record and report

For each direction, record PASS/FAIL for export consent, verified transfer,
quarantine, explicit Apply, independent backup, real application readback,
conversation/message IDs, graph propositions/chains/provenance, writer refusal,
and restart persistence. Record any error text and recovery action. Keep
screenshots confined to the isolated apps; redact pairing strings. Do not
include transcripts, credentials, identity keys or normal-profile data in the
report.

This procedure does not test full-volume exhaustion or power loss. Those remain
separate release qualifications and must only use a dedicated disposable test
volume and expendable hardware.
