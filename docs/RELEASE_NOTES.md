# LearnHarness Sync 0.2.0-alpha.1 experimental source

LearnHarness Sync is an independent, unofficial companion application for LearnHarness. It is not affiliated with or endorsed by the LearnHarness developers.

This source release completes the public rebrand and adds a Linux-only managed launcher for the pinned LearnHarness 0.6.1 x86_64 AppImage. The launcher creates an offline profile owned by a subordinate UID, gives the application a private display, network, PID namespace, HOME and XDG directories, and saves each clean exit as a read-only Btrfs generation. A crash leaves an explicit recovery state. Recovery retains the interrupted generation and resumes the last clean generation.

The existing native store remains at its established internal Linux path. Package upgrade tests verify that the device identity, peer trust, immutable objects, manifests, ledger and backups survive the new display name and bundle identifier.

The desktop supports authenticated LAN pairing, encrypted transfers of supported LearnHarness 0.6.1 content, quarantine on receipt, explicit consent before managed-profile Apply, independent backups and application readback. The ordinary personal LearnHarness profile is never opened or modified by synchronization. The exact packaged acceptance evidence and limits are recorded in [PRODUCTION_SYNC_ACCEPTANCE.md](PRODUCTION_SYNC_ACCEPTANCE.md).

The packaged acceptance suite used two independent LearnHarness Sync peers and real LearnHarness 0.6.1 instances on disposable managed profiles. It verified A → B → A content and graph readback, concurrent-writer refusal, injected storage-full handling, helper SIGKILL after the durable generation switch, explicit recovery, and successful retry. This remains experimental: local packaged peers do not replace acceptance on two physical computers or actual power-loss testing.

Binary publication remains blocked by two-physical-laptop acceptance, actual cold-power-loss and full-storage qualification, and completion of the third-party dependency, corresponding-source and license-notice review. The public project therefore publishes experimental source without installer assets.

[Managed launcher and integration boundaries](MANAGED_LINUX.md) · [Release gates](RELEASE_READINESS.md) · [Physical acceptance procedure](TWO_DEVICE_ACCEPTANCE.md)
