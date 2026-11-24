# LearnHarness Sync QUIC wire protocol — HSQ/2

Transport: QUIC via Quinn 0.11.12, rustls 0.23.46 TLS 1.3, ring cryptography. ALPN is `harnesssync/2`. Protocol version is 2; storage/manifest version is 2 (`harness-sync-synthetic-v2`). Unsupported versions/capabilities fail closed. There is no HTTP, Syncthing, Iroh, relay, rendezvous or 0-RTT.

## Authentication and pairing

An identity is an Ed25519 key and X.509 self-signed certificate for `harnesssync.local`. Identity ID = lowercase SHA-256(certificate DER). Certificates and keys are created with rcgen; signing/verification is performed by ring. Each outgoing TLS connection has only the expected certificate in its rustls root store. Normal chain, signature, validity and name verification run; the resulting certificate must also equal the exact expected bytes.

One bidirectional QUIC stream carries the session. Client sends Hello: `{version:2, capabilities:["synthetic-v2"], name, certificate:[bytes], signature:[bytes], pairing:boolean}`. Server validates Hello and returns its own Hello. Both signatures cover the following exact concatenation:

1. 64-byte TLS exporter using label `EXPORTER-HarnessSync-HSQ1`, context `identity-proof`.
2. Client identity ID (64 ASCII hex bytes).
3. Server identity ID (64 ASCII hex bytes).
4. Role byte 0 for client, 1 for server.

The incoming TLS server does not request a TLS client certificate: the application verifies the Ed25519 proof against the certificate key and an existing exact trust pin before granting any sync access. A signed Hello from an unknown client is insufficient to sync.

New pairing is permitted only during a local two-minute Add Device window. Both screens display the first 128 bits of SHA-256(the above binding with role byte 2), as eight groups of four hexadecimal characters. Users compare **all groups** through a trusted channel. This is session verification, not a weak password. Each side sends `Decision {accept}` only after local explicit approval/rejection. Only two positive decisions allow durable trust persistence. `Ack` is exchanged; stream FINs and their delivery are awaited before connection close. Timeouts/interruptions can leave one-sided consent persisted; retry by removing the relationship on both computers. No trust is established from DNS-SD alone.

Discovery: `_harnesssync._udp.local.` TXT properties `v=1`, `id`, `name`, certificate Base64 chunks `c0…`. At most 64 hints are kept, expiring after 120 seconds. Hints cannot update pinned certificates. IPv4 LAN MVP. A Base64 JSON invitation carries version, name, certificate and numeric LAN addresses. It grants no trust. The current client uses the first invitation address; multi-interface fallback is incomplete. Listen ports persist across restart.

## Transfer state machine

Every application message/frame starts with a **4-byte big-endian unsigned length**. Control JSON limit: 512 KiB. Object frame limit: 16 MiB. Lengths are checked before allocation. Headers/frames have deadlines. Unknown fields on protocol structs are rejected. Canonical manifests must reserialize to exactly their received bytes.

After authentication:

1. Client sends `Head {head: hash|null}`; server returns its captured head.
2. Client traverses missing remote ancestry using `Manifest {id}` requests. Server returns a raw canonical manifest frame.
3. Parents are fetched before child finalization. Missing payloads use `Blob {id}` requests; server returns raw bytes in a bounded frame. Existing objects are reverified and reused.
4. Client checks length/hash/schema, fsyncs object staging, atomically renames into the immutable object store, then commits each complete manifest. Missing ancestry, corruption or interruption never grants a valid snapshot head.
5. Client sends `End`. Roles reverse: the server requests missing history from the client's captured head, then sends `End`.
6. Each side reconciles against its CURRENT local head, creates an independent backup before fast-forward, and atomically commits its ledger. Concurrent local edits therefore cannot be overwritten by stale exchange state.
7. Each side sends `Committed {head, received, conflict}` after durable processing. `received` names the advertised remote head whose entire ancestry and payload were verified; `head` names the actual active head after reconciling against the then-current local history. `conflict` reports preserved divergence. Pairing still uses its separate `Ack`. Repeat exchanges every three seconds.

The one-click synthetic test initializes only the marked isolated profile, generates a unique harmless note, commits it and waits for the selected trusted peer’s authenticated receipt. Success requires the peer’s `head` and `received` to both equal that exact revision, no peer or local conflict, and the matching local active head. A connected socket, byte count or old receipt is insufficient. Failed writes cannot acknowledge adoption. Pending results can settle on a later matching receipt. A fresh receiving profile prepares itself automatically; no production adapter is involved.

HSQ/1 peers fail negotiation and need an update on both computers. Existing profile identities and trust pins are unchanged. Discovery TXT `v=1` and invitation version 1 describe the unchanged discovery/invitation encoding, independently of the transfer version.

The lexicographically smaller identity initiates automatic connections. Failed sessions are retried on a five-second scan. Fresh discovery may refresh an address only for the same pinned identity. An explicit new pairing may start from either device. Connection flags reflect a live authenticated Quinn connection; last exchange time is reported separately. No synthetic throughput is displayed.

Peers can request only manifests/objects reachable from that exchange's advertised head. Revocation closes the connection and removes the local trust pin. The desktop owns all transport tasks; exit closes the endpoint. No external daemon continues afterward.

## Bounds, reconciliation and recovery

16 MiB per file, 128 MiB per snapshot, 4096 files, 4096 ancestry nodes, one parent per manifest; 256 MiB received per pull and served per exchange; at most 4096×5 requests per serve; eight incoming sessions; four pending pairings; 32 trusted devices; 1 GiB total immutable object/manifest/backup quota, 100,000 stored files. Engine JSON commands are limited to 96 KiB. Synthetic note input is limited to 64 KiB. All committed history/backups remain retained. Quota exhaustion refuses changes and requests operator review.

Manifest struct field order is canonical: `version,format,device,parents,files`; entries are `path,hash,size`, paths strictly sorted. ID = SHA-256(compact serde_json bytes), no timestamp in identity. Files are addressed only by validated hashes on disk. Payload paths are metadata only and pass the narrow fixture allowlist; they are never joined onto peer-selected output paths.

Equal, ancestor and descendant cases are idempotent. A remote descendant fast-forwards only after complete ancestry verification; a local descendant stays local. Separate branches are recorded as conflicts. Unknown/cyclic/oversized ancestry refuses reconciliation. No conflict merge or automatic branch selection exists. Every valid branch remains in immutable storage. Previously complete objects survive interrupted connections and restart; unpublished staging files are removed under the exclusive profile lock. A failed disk write never permits an active-head update. Power-loss testing beyond fsync/rename semantics remains pending.


Saved exchange receipts remain useful as history, but a new live connection does
not expose them as current verification until that exact Quinn connection has
completed its own authenticated exchange. Superseded connections cannot mark the
new session verified. Repeated identical receipts retain their prior exchange
time; only changed receipt content creates a new stored exchange timestamp.
