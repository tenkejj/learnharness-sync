# Recovery

Production LearnHarness data is never imported or exported by this native build. These procedures concern invented synthetic engine profiles only.

Immutable payload objects are `objects/<sha256>`, canonical manifests are `manifests/<sha256>`, and `ledger.json` atomically selects the active head and retains conflict/backup references. Objects are immutable and all received bytes are verified before manifest commit. Unfinished transfers cannot appear as a valid head. Restart removes unpublished staging files while holding the exclusive profile lock and resumes by requesting missing verified objects.

Before fast-forward, the previous manifest and its payload bytes are copied under `backups/<snapshot-id>/`, flushed and atomically finalized. Only then is the ledger changed. A failure before that ledger rename leaves the original head active; a failure after rename/fsync can have an uncertain response, so refresh status rather than assuming rollback. Keep the entire profile intact. Fault-injection covers failed writes and quarantine refusal, but power-cut and real disk-full qualification remain pending.

In History → Test backups, expand a verified backup and choose Restore this backup and type `RESTORE SYNTHETIC BACKUP`. The engine verifies it, independently preserves the current head, and creates a NEW revision whose parent is the current head. History is never rewritten. No arbitrary filesystem import paths are accepted. The independent backup directory contains a manifest and hash-named payloads and can be recovered offline even if the active ledger is damaged; there is not yet an automatic offline repair command. Native restore rehydrates missing objects and the selected manifest from the independent verified backup. It also checks the remaining ancestry before publishing a restore revision. Do not manually overwrite the ledger or object store.

Divergence preserves both full branches and records the remote tip in History → preserved versions. No timestamp winner, automatic merge, conflict dismissal or production branch selection is implemented. Missing ancestry refuses reconciliation. Keep every object, manifest and backup until a reviewed recovery tool is available. Garbage collection never removes committed history in this MVP.

A WRITER_ACTIVE marker means an isolated writer is active or quarantined. The native engine refuses synthetic publication/apply/restore while it exists. It does not kill writers or automatically clear quarantine. The old supervisor is behavioral reference only. Production refusal is unconditional regardless of that marker.

If identity/trust is corrupt, startup fails; it never silently regenerates identity. Preserve the profile for inspection. New identity requires a fresh profile and new explicit pairing on both devices. Device removal revokes new access but keeps all received snapshots. The application has no at-rest encryption: preserve filesystem access controls and protect backups as private data.
