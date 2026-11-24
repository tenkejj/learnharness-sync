# Physical two-computer managed-sync acceptance

**Status: pending.** Do not mark this gate complete from loopback peers or from
the packaged acceptance run on one host. Record results from both physical
Linux computers and use only new disposable managed LearnHarness profiles.

The owner has separately [reported successful physical bidirectional synthetic
sync](evidence/physical-synthetic-owner-report.md) with the ordinary AppImage.
That verifies pairing, encrypted LAN transport and test-data adoption on two
computers. The real LearnHarness managed-profile steps below remain untested
across those two physical computers.

## Disposable offline test setup

The ordinary AppImage has no test provider, so a new offline profile may be
unable to create a completed conversation without credentials. For this
acceptance test only, build the **private feature-only** AppImage on the
development host with `scripts/build-private-acceptance-appimage`. Copy that
AppImage and `scripts/physical-managed-acceptance.py` privately to both test
computers. Never publish the feature-enabled AppImage or run it against a
normal HOME. It contains test-only commands and is not a release candidate.
The current local private artifact is
`build/private-acceptance/LearnHarness-Sync_0.2.0-alpha.1_PRIVATE-ACCEPTANCE.AppImage`
with SHA-256
`2bb7ab89f103e58fe435bf602dfc791642f28cfd73ca4ca257533b395277f70e`.
Verify that hash after copying it to each computer.

On **each** computer, create a fresh private parent directory on Btrfs and run
the following with local paths to the ordinary AppImage, private test AppImage,
and verified LearnHarness 0.6.1 AppImage:

```sh
mkdir -m 700 -p "$HOME/learnharness-sync-disposable-test"
python3 physical-managed-acceptance.py prepare \
  --parent "$HOME/learnharness-sync-disposable-test" \
  --ordinary "$HOME/Downloads/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage" \
  --acceptance "$HOME/Downloads/LearnHarness-Sync_0.2.0-alpha.1_PRIVATE-ACCEPTANCE.AppImage" \
  --learnharness "$HOME/Downloads/LearnHarness-0.6.1-x86_64.AppImage"
```

Save the newly printed root path separately on each computer. The runner
verifies the upstream SHA-256, checks that the ordinary AppImage rejects the
acceptance console, copies the real application into its own fresh HOME, and
pins both package hashes. It refuses changed packages, symlinks, a non-Btrfs
parent, or a root that is not owned by the current user with mode 0700. It
does not select or migrate an existing profile. Do not manually delete the
disposable roots: they contain Btrfs subvolumes that require exact cleanup.

Use `python3 physical-managed-acceptance.py seed ROOT_A` on A to create four
messages and a supported graph through the real LearnHarness application and
its isolated keyless local provider. On B, use
`python3 physical-managed-acceptance.py learnharness ROOT_B` once to initialize
its empty managed profile, then close the window. Launch the **ordinary** GUI
on each computer with `python3 physical-managed-acceptance.py app ROOT_A` and
the matching `ROOT_B`. Pair and transfer in those GUIs. After B has applied
the received revision, close its GUI and run
`python3 physical-managed-acceptance.py learnharness ROOT_B` for real
application readback. Then run
`python3 physical-managed-acceptance.py extend ROOT_B` to add the B-authored
Unicode follow-up. Reopen the ordinary GUIs for B → A transfer, and finally
run `python3 physical-managed-acceptance.py learnharness ROOT_A` to confirm
readback on A. If any command fails, keep that disposable root for inspection;
do not substitute a personal LearnHarness profile or credential.

After each real-application readback, independently check the captured
conversation and graph with the private runner. Use the chat ID printed by
`seed`: `python3 physical-managed-acceptance.py verify ROOT_B --chat-id CHAT_ID
--messages 4` on B, then the same command with `ROOT_A --messages 6` after
the reverse transfer. It prints IDs, counts and the revision hash, never the
conversation text. Record the outputs on both computers without publishing
device identity keys or transcripts.

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
   created disposable content. If the private offline setup above cannot
   create a completed conversation, stop and report that blocker; do not
   migrate or copy a normal profile to make the test proceed.
3. On A, confirm the private seed reported four messages, three propositions
   and two chains. Check the ordinary packaged managed snapshot and note its
   file and byte counts.
4. Pair A and B using **Add Device**. Compare every verification-string group
   in person and approve on both computers.
5. On A, explicitly authorize sending the managed conversation to B. Confirm
   that B reports a verified staged copy. The receipt must not claim the data
   was applied.
6. On B, inspect the revision and explicitly authorize **Apply verified
   revision**. Wait for the backup confirmation. Reopen LearnHarness from the
   managed session, verify the conversation, messages and graph, then close it
   cleanly. LearnHarness Sync must report application readback verified.
7. Run the private `extend ROOT_B` step above after B's readback to add the
   Unicode follow-up through the real application. Explicitly authorize the
   B → A transfer, and apply it on A with the same
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
