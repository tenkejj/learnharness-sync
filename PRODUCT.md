# LearnHarness Sync
<!-- impeccable:product-schema 1 -->
## Platform
web
Desktop control center for Linux, backed by a local engine.
## Stack
Tauri 2, Rust/Quinn/rustls/Tokio/mdns-sd, React and TypeScript. One desktop process owns the native engine. Python code remains behavioral reference only. Linux is the initial build target.
## Users
LearnHarness users alternating laptop and desktop on Omarchy, including offline use.
## Product Purpose
Private, local-first synchronization for LearnHarness, with immutable histories and explicit divergence.
## Capabilities and Constraints
Never overwrite active data. Never modify production data, system services or existing launcher in this session. Unsupported lifecycle integrations fail closed. Use synthetic fixtures for tests. Product copy is English.
## Brand Commitments
LearnHarness Sync. Canonical approved PNGs in assets/brand, unchanged. Black, white and gray. Unofficial community companion.
## Product Principles
Preserve recoverability; report uncertainty; use causal ancestry; do not fabricate connectivity.
## Evidence on Hand
A complete explicit user brief in PROMPT_FOR_CODEX.md; approved bitmap branding. No authorized production integration or second-machine access.
