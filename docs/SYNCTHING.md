# Historical architecture — retired

The native LearnHarness Sync application no longer uses Syncthing, its API, device IDs, credentials, folders or services. Do not install or configure Syncthing for LearnHarness Sync. Do not stop, modify or uninstall an existing system Syncthing service.

The Python package and scripts/legacy-reference retain the previous behavioral reference tests. Their transport behavior does not certify the native engine. See ARCHITECTURE.md, PROTOCOL.md and NATIVE_TEST_EVIDENCE.md for the current implementation.
