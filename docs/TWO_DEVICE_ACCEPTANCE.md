# Physical two-computer LearnHarness 0.7.0 acceptance

**Status: pending owner results from both computers.** The owner has verified bidirectional **synthetic** sync on two Omarchy computers. Packaged real LearnHarness 0.7.0 A → B → A passed on one host with independent disposable peers; that does not replace this physical test.

Use the private transfer folder described in [SECOND_COMPUTER.md](SECOND_COMPUTER.md). It contains exactly two distinct LearnHarness Sync AppImages, the reviewed LearnHarness 0.7.0 AppImage, the private runner, the reviewed build registry, and `SHA256SUMS`. The `PRIVATE-ACCEPTANCE` AppImage contains offline test commands. **Never publish it or run it under your normal HOME.** The ordinary AppImage has no acceptance console. Python 3 is needed only for this physical test runner.

## Prepare separate disposable profiles

On **both** computers, put the six transfer files together in `$HOME/Downloads/learnharness-sync-physical-070`, then verify them:

```sh
cd "$HOME/Downloads/learnharness-sync-physical-070"
sha256sum -c SHA256SUMS
stat -f -c %T "$HOME"
test -x /usr/bin/node
test -x /usr/bin/curl
mkdir -p -m 700 "$HOME/learnharness-sync-disposable-test"
chmod 700 "$HOME/learnharness-sync-disposable-test"
```

The filesystem reported for the private parent must be `btrfs`. The private offline seed also needs the host's `/usr/bin/node` and `/usr/bin/curl`; the packaged test assets include Playwright but not the Node executable. Do not change an existing personal profile or system service to satisfy a failed prerequisite. The managed launcher also needs the existing Omarchy user namespace, subordinate UID/GID mapping, bubblewrap, `newuidmap`/`newgidmap`, util-linux, btrfs-progs, and display tools described in [SECOND_COMPUTER.md](SECOND_COMPUTER.md).

On the **PC (A)**, run:

```sh
cd "$HOME/Downloads/learnharness-sync-physical-070"
ROOT_A=$(python3 physical-managed-acceptance.py prepare \
  --version 0.7.0 --registry "$PWD/learnharness-builds.json" \
  --parent "$HOME/learnharness-sync-disposable-test" \
  --ordinary "$PWD/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage" \
  --acceptance "$PWD/LearnHarness-Sync_0.2.0-alpha.1_PRIVATE-ACCEPTANCE.AppImage" \
  --learnharness "$PWD/LearnHarness-0.7.0-x86_64.AppImage")
printf 'PC disposable root: %s\n' "$ROOT_A"
python3 physical-managed-acceptance.py seed "$ROOT_A"
python3 physical-managed-acceptance.py snapshot "$ROOT_A"
```

Write down `ROOT_A` and the chat ID printed by `seed`. The seed opens the **real LearnHarness 0.7.0** application under a new subordinate-UID profile and creates four supported messages, three propositions, two chains, provenance, layout, and a 0.7.0 conversation folder. It uses an isolated keyless local test provider; no personal credentials are needed.

On the **laptop (B)**, prepare a different profile and initialize its empty managed LearnHarness window:

```sh
cd "$HOME/Downloads/learnharness-sync-physical-070"
ROOT_B=$(python3 physical-managed-acceptance.py prepare \
  --version 0.7.0 --registry "$PWD/learnharness-builds.json" \
  --parent "$HOME/learnharness-sync-disposable-test" \
  --ordinary "$PWD/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage" \
  --acceptance "$PWD/LearnHarness-Sync_0.2.0-alpha.1_PRIVATE-ACCEPTANCE.AppImage" \
  --learnharness "$PWD/LearnHarness-0.7.0-x86_64.AppImage")
printf 'Laptop disposable root: %s\n' "$ROOT_B"
python3 physical-managed-acceptance.py learnharness "$ROOT_B"
```

The runner checks the complete upstream image SHA-256 and size against the [reviewed registry](../support/learnharness-builds.json), copies it only into the fresh disposable HOME, and records the selected version and build hash. Every later runner command rechecks the private image and the two LearnHarness Sync artifacts. A 0.6.1 test remains available by explicitly choosing `--version 0.6.1` with the separately verified 0.6.1 AppImage; it creates a different managed vault and peer wire format. Do not mix versions in one test.

## A → B → A through the ordinary desktop

1. On both computers, run `python3 physical-managed-acceptance.py app "$ROOT_A"` or `... app "$ROOT_B"` in separate terminals. Pair the two new device identities. Compare **all eight verification-string groups** in person and approve on both sides.
2. On A, **Check managed snapshot**, then explicitly authorize export to B. Confirm B reports a verified **quarantined transfer**, not an applied profile.
3. On B, inspect the received revision, explicitly authorize **Apply verified revision**, and confirm its independent backup. Close the B desktop window. Run `python3 physical-managed-acceptance.py learnharness "$ROOT_B"`, inspect the real conversation and graph, then close LearnHarness. Run `python3 physical-managed-acceptance.py verify "$ROOT_B" --chat-id CHAT_ID --messages 4`.
4. On B, run `python3 physical-managed-acceptance.py extend "$ROOT_B"`. This uses real LearnHarness to add a Unicode follow-up. Reopen ordinary desktops on both computers, authorize B → A export and explicit Apply on A, and confirm A's independent backup.
5. Close A's desktop, run `python3 physical-managed-acceptance.py learnharness "$ROOT_A"`, inspect the real conversation and graph, close LearnHarness, then run `python3 physical-managed-acceptance.py verify "$ROOT_A" --chat-id CHAT_ID --messages 6`.
6. While either managed LearnHarness window is open, try **Check managed snapshot** there. It must refuse an active writer. Close LearnHarness cleanly and retry. Restart both ordinary desktops and confirm trust, history and backups remain.

The `verify` command independently compares each captured file's size and SHA-256 to the received manifest, then checks chat and message IDs, graph relationships, provenance, layout, and the 0.7.0 folder placement. It prints identifiers and counts, never conversation text. Record PASS/FAIL for consent, transport, quarantine, backup, Apply, real application readback, writer refusal, and restart persistence in **both directions**. Keep private keys, pairing strings, transcripts, and personal-profile data out of reports.

If any command fails or a result is uncertain, stop and retain that disposable root for inspection. Do not substitute a personal LearnHarness profile or credential. Do not recursively delete a Btrfs acceptance root: its subvolumes need exact, privileged cleanup. Full-volume exhaustion and cold power loss remain separate release tests on expendable storage and hardware.
