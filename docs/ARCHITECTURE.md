# Native desktop architecture

LearnHarness Sync's main Tauri process owns one `Arc<Engine>` for its entire lifecycle. Tokio runs QUIC accept/reconnect tasks, mdns-sd runs its in-process discovery thread, and bounded storage operations run through `spawn_blocking`. No Python child, local HTTP server, external sync daemon or mandatory network service exists. Closing the application shuts down the endpoint. Tray/autostart behavior is not implemented.

`sync-core` is pure snapshot validation and causal reconciliation. `native-engine/storage` owns private paths, profile exclusion, object staging, durable commits, independent backup copies and a single atomic ledger. `identity` owns persistent Ed25519 identity; `network` owns HSQ/1 authentication and transfers; `discovery` supplies untrusted LAN hints; `engine` owns trust, pending consent, real connection state and bounded command dispatch. `adapter` exposes only invented fixtures and an unconditional production refusal.

The Rust command enum, local-only main-window Tauri capability, CSP and request limits constrain the frontend boundary. No frontend-selected executables or arbitrary filesystem paths are supported. Native status events update the React interface every two seconds; periodic command polling recovers from missed events. Errors remain errors. Invalid response shapes cannot be rendered as success.

Native profiles have a synthetic-only marker and cannot adopt a nonempty ordinary directory. Identity/trust, immutable content and all backups reside in the OS app-data profile, outside the repository. Legacy Python profiles remain separate and unchanged. Unix ownership/modes, no-follow opens, bounded reads and single-instance profile locks apply. Windows/macOS filesystem durability and private ACL behavior are not certified.

The wire protocol and pairing threat model are in PROTOCOL.md and SECURITY.md. Production writer exclusion and real LearnHarness schema compatibility remain closed gates; scanning processes would not open them. The Linux Python subreaper supervisor remains reference code, since its separate-process descendant model is not safe to insert globally into a multithreaded desktop app.
