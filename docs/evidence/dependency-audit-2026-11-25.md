# Dependency review, 2026-11-25

`npm audit --json` reported 0 vulnerabilities at every severity. `cargo audit
--json` reported 0 vulnerabilities and three warnings in the lockfile:

| Package | Advisory | Disposition |
| --- | --- | --- |
| `chrono 0.4.45` | [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543.html), unmaintained | `cargo tree --workspace --target all -i chrono@0.4.45` found no current workspace path. It remains in the lockfile and warrants pruning during a controlled lockfile update. |
| `proc-macro-error 1.0.4` | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html), unmaintained | Build-time proc macro through GTK/GLib macros in Tauri's Linux stack. No patched release is listed by RustSec. |
| `glib 0.18.5` | [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), unsound | Runtime dependency through Tauri/WebKitGTK/GTK. The advisory concerns `VariantStrIter` methods and lists `>=0.20.0` as patched. Project source has no direct use, but transitive GUI use has not been excluded. This is unresolved for binary publication. |

The AppImage ELF inventory and native-package scan in this directory identify
bundled components and found no credential patterns. Matching installed
package metadata is not a corresponding-source or license-notice clearance
for redistribution. The outer AppImage runtime, embedded resources, exact
corresponding source and notices remain under review. The dependency security
and redistribution release gates therefore remain false.
