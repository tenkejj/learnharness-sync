"""Strict, fail-closed beta release gate. Manual attestation is not certification."""
import json
from pathlib import Path
import sys

VERSION = '0.2.0-alpha.1'
REQUIRED = {'production_writer_exclusion_verified', 'learnharness_schema_adapter_verified',
            'linux_native_install_verified', 'native_quic_two_process_verified', 'two_physical_devices_verified',
            'asset_redistribution_rights_confirmed', 'public_release_authorized',
            'production_crash_recovery_verified', 'dependency_security_review_verified',
            'dependency_redistribution_verified'}

def failures(gates):
    if not isinstance(gates, dict) or set(gates) != REQUIRED | {'version'}:
        return ['invalid or incomplete release gate schema']
    if gates['version'] != VERSION:
        return ['release version does not match the beta target']
    return sorted(key for key in REQUIRED if gates[key] is not True)


def main():
    root = Path(__file__).resolve().parent.parent
    try:
        failed = failures(json.loads((root / '.github/release-gates.json').read_text()))
    except (OSError, ValueError):
        failed = ['release gate file unavailable']
    if failed:
        print('PUBLIC RELEASE BLOCKED: ' + ', '.join(failed))
        return 1
    print('Declared beta gates satisfied; independent test evidence and explicit authorization still apply.')
    return 0

if __name__ == '__main__':
    sys.exit(main())
