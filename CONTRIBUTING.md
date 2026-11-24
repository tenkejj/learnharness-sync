# Contributing

Read docs/ARCHITECTURE.md and docs/PROTOCOL.md first. Production mutation stays disabled until writer exclusion and LearnHarness schema/relationship adapters are independently verified. Never test with actual learning data or copy upstream AGPL implementation into this MIT source without an explicit license decision.

Use `npm ci`, `cargo test --locked -p harness-sync-core -p harness-sync-native`, `npm run test:peers`, `npm run build`, `npm test`, `npm run lint`, `npm run format:check`, and the preserved reference suite `python -m unittest discover -s tests -v`. Python is not a native build/runtime dependency. Make data-safety changes with synthetic regression tests and interruption tests. Explain which invariants changed and which platforms/flows were actually tested. The native Rust backend requires real transport/storage regression tests. Do not add fictional statuses or mark unverified peers connected.

Source is English, user-facing UI is English. Preserve the exact approved bitmap logo. Respect accessibility, keyboard focus, reduced motion and clear error recovery. Report actual test evidence, not aspirations. No private data or generated workspaces belong in Git.
