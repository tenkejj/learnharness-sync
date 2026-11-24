> Current reproducible desktop suite and results: [LearnHarness 0.6.1 qualification](LEARNHARNESS_061.md). The historical v1-only Python checker below rejects current version-2 chats.

> Historical reference evidence from the Python/Syncthing implementation. It does not certify the current native engine. See NATIVE_MIGRATION_AUDIT.md and NATIVE_TEST_EVIDENCE.md. Production access remains disabled.

# Isolated LearnHarness acceptance

The release needs more than a simplified snapshot fixture. This helper exercises a COPY of the real audited 0.6.1 AppImage with invented records and a private application home.

```sh
python scripts/learnharness_isolated.py \
  --appimage /path/to/LearnHarness-0.6.1-x86_64.AppImage \
  --report /tmp/harness-sync-learnharness.json
```

Requirements: Linux, real desktop/session and permitted loopback sockets. In the actual LearnHarness window inspect the clearly invented session, make only test edits, then quit normally. No provider key is imported. The original AppImage is copied first, so its updater cannot modify the user's original executable through this runner. HOME/XDG data/cache/state are redirected; only essential display/locale/path variables are inherited.

The supervisor runs separately as a Linux child subreaper and waits for every adopted descendant, including a double-forked detached worker. Its durable WRITER_ACTIVE marker plus engine lock prevent fixture export while the family is alive. A failed launch, nonzero exit, timeout or killed supervisor leaves quarantine in place. Do not delete the marker merely because a window disappeared. The kernel/process tests are real subprocess tests, not mocked name polling.

This is NOT a security sandbox for malicious code and NOT exclusion of unmanaged processes. It cannot prove that a foreign/manual launch is absent. It does not enable production mutations or replace the original LearnHarness launcher. It also does not grant semantic compatibility merely because JSON parses.

`learnharness_format.py` performs a conservative read-only structural audit using independently implemented checks from public upstream contracts: chat identity/lazy v1 version, graph endpoints/provenance/checkpoints, notes references, terminal jobs and secret/unknown-path refusal. It never follows attachment paths. Unknown plugin/durable state or unreadable directories refuse inspection; no files are silently dropped. Full native-message and optional-record semantics remain unverified, so the result always includes production_compatible=false.

A successful helper result is limited to the actual isolated application process family and supported structural subset. It is still not end-to-end production synchronization. On the current agent host, the helper returns blocked before launch because socket creation is denied. The next required evidence is a real run on the owner's Omarchy session, followed by the complete production adapter and unmanaged-launch exclusion design.
