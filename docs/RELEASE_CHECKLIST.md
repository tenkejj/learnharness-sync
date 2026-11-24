# Native release checklist

- [x] All native/frontend/reference tests pass on the exact candidate sources.
- [x] Two independent native processes pass authenticated transfer, restart and divergence.
- [x] Actual Tauri/WebKit smoke passes in a disposable Linux environment.
- [ ] GUI pairing is validated on two physical devices.
- [x] Linux candidate packages are independently installed and inspected for absent Python/sidecars/private state.
- [ ] The managed launcher excludes ordinary-UID writers and has crash-recovery evidence. Real-data peer capture and activation remain disabled in the installed desktop.
- [x] The owner authorized the supplied artwork and its derivatives for project distribution.
- [ ] Binary dependency notices, corresponding source and advisory review are complete.
- [x] Repository, index/history and extracted artifacts are scanned for private data and credentials.
- [x] No unsupported platform or internet connectivity claim appears in release notes.
- [ ] Exact checksums and signatures are reviewed.
- [x] The owner selected LearnHarness Sync and authorized experimental source publication. Identity, trust and backup continuity must be tested for each package update.

Do not infer checked items from this file. The current evidence is in LEARNHARNESS_061.md and false gates remain in `.github/release-gates.json`.
