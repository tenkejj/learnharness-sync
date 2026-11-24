# Changelog

## Unreleased — native engine migration (2026-11-17)

- Replaced the Python subprocess/HTTP bridge and external sync-engine integration with an in-process Rust engine owned by Tauri.
- Added persistent Ed25519 identities, certificate-pinned TLS 1.3 QUIC, session-bound mutual pairing, LAN DNS-SD and invitation fallback.
- Added versioned deterministic manifests, SHA-256 objects, staged verification, durable metadata, complete ancestry, incremental transfer, retries/reconnection and divergence preservation.
- Added independent synthetic backups and explicit restore as a new descendant revision. Committed history is retained under a bounded object quota.
- Adapted approved React screens to real native discovery, pairing, trusted-device status, isolated notes and status events. Removed obsolete API-key and external-service setup.
- Removed externalBin/PyInstaller requirements and publication workflow; Linux native candidate build uses Rust/Tauri only.
- Preserved the Python behavioral reference suite and original frontend tests. Added native security/storage/transport tests and an actual two-process Rust acceptance harness.
- Kept production LearnHarness access disabled: real schema compatibility and race-free writer exclusion are unverified. Physical-device acceptance remains pending. See docs/NATIVE_TEST_EVIDENCE.md for actual results.

Historical Python/Syncthing evidence is retained for reference; it is not evidence for the new native transport. No public release has been made.
