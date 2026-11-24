# Platform support

Linux x86_64 is the initial native target; see NATIVE_TEST_EVIDENCE.md for exact build/launch evidence. Installation passed on isolated Ubuntu and Fedora systems. Physical desktop/laptop acceptance of this candidate remains pending. See [qualification](LEARNHARNESS_061.md). This is not a claim of Linux distribution-wide compatibility.

Common engine modules use portable Rust/Quinn/Tokio interfaces. Unix-specific ownership, mode and no-follow checks are cfg-isolated. Windows/macOS installer builds, private ACL enforcement and directory-rename durability are unverified; no installers for those platforms are advertised. CI attempts engine tests on all three platforms but an unexecuted workflow is not local verification.

There is no production writer adapter on any platform, no tray mode and no installed daemon. The Python Linux subreaper remains reference-only. Running it is not part of the native application lifecycle.
