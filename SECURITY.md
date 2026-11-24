# Security

LearnHarness Sync's installed peer path transfers **invented synthetic fixtures only**. Production LearnHarness export/import is disabled unconditionally. The real application adapter is restricted to a disposable qualification runner and a narrow 0.6.1 contract. Never copy real sessions or credentials into a test profile.

The Linux managed launcher creates a separate offline profile. A subordinate UID owns a mode-0700 Btrfs vault that the ordinary desktop UID cannot traverse. The application runs with private network, PID and mount namespaces and a private authenticated display. The supervisor holds the operation lock until the application process family exits. Clean exits produce read-only generations. An abnormal exit leaves a recovery marker and retains the interrupted writable generation. Recovery resumes the last clean generation. This boundary does not protect against root, a process deliberately started as the same subordinate identity, kernel compromise or physical storage failure.

## Network trust

QUIC uses Quinn with rustls TLS 1.3, ALPN `harnesssync/2`, no 0-RTT and no insecure verifier. Each installation creates a private Ed25519 key and self-signed certificate. Outgoing connections use the exact expected certificate as a trust root, validate the TLS handshake and server name, and additionally check certificate byte equality. Incoming clients prove possession of the certificate's Ed25519 private key with a signature bound to the TLS exporter, both certificate fingerprints and the endpoint role.

Discovery and invitations are untrusted hints. Initial certificate pinning alone does not defeat substituted discovery: users MUST compare all 128 bits of the session-bound verification string across both screens through a trusted channel. Both users explicitly approve before trust is persisted. A substituted/MITM session has a different exporter and verification string. Replayed proofs fail on another TLS connection. An unauthenticated peer cannot transfer objects. Pairing requests are accepted only during a two-minute user-opened window.

Trust pins certificate bytes and their SHA-256 fingerprint, persists locally, and can be revoked. Revocation closes active connections; already received history remains. Remove the relationship on both devices. Failed final acknowledgments can leave one-sided trust; remove and pair again on both computers. Device names are hints, never identities. Protect the entire engine profile: possession of its private key permits impersonation. Identity loss requires new pairing; corrupt identities are never silently regenerated.

## Storage and untrusted data

Object IDs are lowercase 64-character hashes, never paths. Deterministic v2 manifests inventory paths, lengths and SHA-256 hashes. The shipping wire protocol accepts only the synthetic JSON schema. The local Btrfs backup inspector has a separate read-only 0.6.1 validator and returns only aggregate counts. The managed vault can contain LearnHarness credentials if a user configures them. Its contents never enter the shipping peer path. All peer objects and complete ancestry are checked before active-head advancement. Interrupted transfers leave no valid partial snapshot; retries reuse verified complete objects. Local state changes use file fsync, rename and directory fsync on Unix. Backups are copied independently before a single atomic ledger update. Divergence is retained; timestamps never override history.

Private Unix directories require ownership and mode 0700. Files use 0600, no-follow opens and regular-file/link-count checks. Engine profiles are single-writer locked. The trust store is durably written before exposing new trust in memory. Peer requests can read only objects reachable from the head advertised in that exchange. Size/time/concurrency/history limits apply; see PROTOCOL.md. Committed data is not automatically garbage-collected.

## Limits and reporting

This is not a security certification. Physical LAN acceptance, hostile LAN review, forced power loss and Windows/macOS ACL/durability testing remain pending. Linux qualification has exercised kernel ENOSPC, EACCES and process termination immediately after an atomic exchange. A hostile process running as the desktop user can steal the peer engine's identity keys or manipulate its UI. It cannot traverse the subordinate-UID managed vault through ordinary discretionary access. LAN metadata advertises the public certificate, device name and addresses. Authorized peers can send unwanted synthetic history up to quotas. There is no at-rest encryption, relay or internet connectivity guarantee.

Report security issues privately to the repository owner using its private advisory channel when available. Include versions and a minimal invented fixture; never send private keys, engine profiles, real LearnHarness data or account credentials. Do not publish sensitive reproducers in public issues.
