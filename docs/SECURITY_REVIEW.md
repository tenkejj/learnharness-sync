> Historical reference evidence from the Python/Syncthing implementation. It does not certify the current native engine. See NATIVE_MIGRATION_AUDIT.md and NATIVE_TEST_EVIDENCE.md. Production access remains disabled.

# Independent review results

An independent review agent read the reference engine and tests with the review-agent skill. Initial demonstrated issues were fixed and regression-tested:

- Symlink ancestry could cause export outside workspace before verification: directory creation now rejects link ancestry before mutation; regression confirms no external writes.
- Syncthing API redirects could forward X-API-Key: redirects are now forbidden and tested without real credentials.
- Historical inspection retained all payloads: index now retains metadata only; selected apply payload is reverified and loaded on demand.
- Unchanged export returned a missing snapshot ID: head existence/integrity is checked before success.
- Installer checked unsupported prefix characters after copy: validation moved before any mutation.

Fresh security review confirmed earlier fixes and 29 passing Python tests, with no demonstrated high-severity rollback defect. This is not an exhaustive security certification. Actual network socket integration, power-cut durability and concurrent filesystem attack resistance remain unproven. Production operations remain disabled.

A separate UI reviewer found malformed API shape crashes, raw network errors and inaccurate detected-path copy. All three were fixed, and the reviewer reran six frontend tests successfully. Its verdict covers those fixes only. Visual finish gate remains incomplete because screenshots were denied by tool approval policy. Manual native, visual, keyboard and screen-reader tests are still required.

## Continuation review

A fresh independent reviewer examined Profile/controller, REST changes, command authentication, Tauri bridge and new platform/live-runner source. It found two demonstrated REST isolation defects: unselected devices inherited from a folder template and stop-sharing allowed to target the local namespace. Both were fixed with regression tests; reviewer confirmed those fixes and all ten transport tests. The complete current Python suite has 53 tests; frontend has eleven. No claim of native/live execution follows from source review.


The continuation UI review found editable fields during in-flight settings saves and unclear unknown outcomes after a lost mutation response. Forms now disable their fieldsets while busy and reload validated configuration after save; command failures distinguish pre-send errors from possibly completed mutations. New regression tests cover uncertain network/JSON responses. The general status error no longer promises that no prior mutation occurred. Native/screenshot verification remains open.


## Beta continuation review

A fresh independent review found staged bytes omitted from the publication scan, historical forbidden paths omitted, unreadable directories silently skipped, and native timeout cleanup that could leave a onefile worker alive. All were fixed with real Git/process/filesystem regression tests. Production exclusion and actual LearnHarness compatibility remain deliberately unclaimed. See RELEASE_READINESS.md for current evidence and environment restrictions.
