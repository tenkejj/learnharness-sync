# Owner-reported physical synthetic acceptance

The product owner reported that the ordinary Linux AppImage ran on two physical
computers, a PC and a laptop. They successfully paired, established an encrypted
LAN connection, and ran the built-in synthetic **Run sync test** in both
directions. Each device reported successful verification and adoption of the
test data for PC → laptop and laptop → PC.

This is owner-reported physical bidirectional **synthetic** synchronization.
The report does not include real LearnHarness 0.6.1 conversations, managed
profile capture or apply, application readback, full-volume exhaustion, or
cold power-loss testing on either physical computer. Those cases remain open;
the aggregate `two_physical_devices_verified` release gate remains false.

No transcript, credential, device key, pairing string, or personal profile was
provided or used for this report.
