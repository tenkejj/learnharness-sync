# LearnHarness Sync 0.2.0-alpha.1 experimental source

LearnHarness Sync is an independent, unofficial companion application for LearnHarness. It is not affiliated with or endorsed by the LearnHarness developers.

This source release completes the public rebrand and adds a Linux-only managed launcher for the pinned LearnHarness 0.6.1 x86_64 AppImage. The launcher creates an offline profile owned by a subordinate UID, gives the application a private display, network, PID namespace, HOME and XDG directories, and saves each clean exit as a read-only Btrfs generation. A crash leaves an explicit recovery state. Recovery retains the interrupted generation and resumes the last clean generation.

The existing native store remains at its established internal Linux path. Package upgrade tests verify that the device identity, peer trust, immutable objects, manifests, ledger and backups survive the new display name and bundle identifier.

The desktop supports authenticated LAN pairing, encrypted test transfers, history, backup restore, read-only inspection of a narrow 0.6.1 backup profile and local managed LearnHarness sessions. Real conversation and knowledge-graph peer synchronization remains disabled in the installed desktop. The real-data adapter and atomic activation path are available only to the disposable qualification runner.

The qualification suite has reopened actual LearnHarness data after A → B → A round trips. It covers conversations, stable message identifiers, tested metadata, three propositions, two chain shapes, provenance and saved layout using a deterministic local provider. It also covers divergence, disk-full and permission errors, forced termination during activation, independent backup restoration and migration from a generated read-only snapshot.

Binary publication remains blocked by ordinary-desktop integration of real-data capture and activation, independent corresponding-source and notice review for bundled AppImage libraries, cold-power-loss testing and a two-physical-laptop acceptance run. The public project therefore publishes experimental source without installer assets.

[Managed launcher and integration boundaries](MANAGED_LINUX.md) · [Release gates](RELEASE_READINESS.md) · [Physical acceptance procedure](TWO_DEVICE_ACCEPTANCE.md)
