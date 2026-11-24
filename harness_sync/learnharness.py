"""Read-only discovery and conservative writer evidence. Evidence is NOT exclusion."""
import os
from pathlib import Path
import re
import sys
from .platforms import learnharness_path, lifecycle_capabilities


def audit():
    data = learnharness_path()
    evidence = []
    if sys.platform.startswith('linux') and Path('/proc').is_dir():
        for process in Path('/proc').iterdir():
            if not process.name.isdigit():
                continue
            try:
                if process.stat().st_uid != os.getuid():
                    continue
                name = (process / 'comm').read_text().strip().lower()
                # Conservative: unrelated node/Electron may also block future automation.
                # No cmdline/environment/session payloads are read or returned.
                if any(word in name for word in ('learnharness', 'electron', 'node')):
                    evidence.append({'pid': int(process.name), 'kind': 'possible-writer'})
            except (OSError, ValueError):
                pass
    candidates = []
    if sys.platform.startswith('linux'):
        candidates = sorted((Path.home() / 'Applications').glob('LearnHarness-*.AppImage'))
    versions = []
    for candidate in candidates:
        match = re.fullmatch(r'LearnHarness-([0-9]+\.[0-9]+\.[0-9]+)-[^/]+\.AppImage', candidate.name)
        if match:
            versions.append({'version': match.group(1), 'evidence': 'filename-only'})
    return {'data_path': str(data), 'detected': data.is_dir(), 'versions': versions,
            'possible_writers': evidence, 'scan_complete': False, 'safe_to_mutate': False,
            'capabilities': lifecycle_capabilities(),
            'message': 'Process evidence cannot exclude detached/unmanaged writers. Production operations are blocked.'}
