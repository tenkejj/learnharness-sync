"""Read-only candidate extraction/inspection. Optional development tool, never a build/runtime dependency."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
bundle = root / 'target/release/bundle'
artifacts = [bundle/'appimage/LearnHarness-Sync_0.2.0-alpha.1_amd64.AppImage', bundle/'deb/LearnHarness-Sync_0.2.0-alpha.1_amd64.deb', bundle/'rpm/LearnHarness-Sync-0.2.0-alpha.1-1.x86_64.rpm']
forbidden = {'identity.json','devices.json','ledger.json','sandbox.json','engine_profile.json','auth.json','harness-sync-engine','harness-sync-engine.exe'}
patterns = [re.compile(rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\r?\n[A-Za-z0-9+/=\r\n]{48,}-----END'), re.compile(rb'gh[pousr]_[A-Za-z0-9]{30,}'), re.compile(rb'github_pat_[A-Za-z0-9_]{40,}'), re.compile(rb'AKIA[0-9A-Z]{16}')]
# Manually inspected: this exact bundled public Unicode-name table contains the
# AWS-prefix regex as concatenated dictionary words, not an AWS credential.
PUBLIC_TABLE_SHA256 = '9be222982d717cd29b2ce262b4f7410bd2601725563bb2df4b3517f5fab2d199'
# Verified byte-for-byte against the official GnuTLS crypto-selftests-pk.c.
PUBLIC_GNUTLS_SHA256 = '7f3e375be368b886e898a4f0ec0ee194a1179d50a4139b8dea10afff54dfd815'
PUBLIC_SELFTEST_MATCHES = {
'8f7e981b1bf5ea40670d1215da5c92c7bcce577ea8296720cc0ea183f47d6503',
'5ddf905c9d54ece0fe06ae61c74ca76af2a9a800f57637978eb6e37f9d3f2ce5',
'effa052b9d20c73aa9aca90486f5d1cb32662e909d841d4049afe9a00b46d8da',
'0200ea14848fe59717c21a32ba6d7f8000a688b0e57b06b3bd515528f97f6d07',
'd34730e61db9f0788eb86bfd04f5c69d11713b16d834d28ca95782ec361a408c',
'6b5f7abf3daad2dee80976d65b8576f38b64d2a1618589f27a2877a26aac2714',
'1e4ef7695765f411b0b2c282757d8ba6dfe3379538d03b2a092b37537b726c9c'}
results = []
for artifact in artifacts:
    if not artifact.is_file(): raise SystemExit('Candidate missing: '+artifact.name)
    with tempfile.TemporaryDirectory(prefix='harnesssync-artifact-audit-') as temporary:
        stage = Path(temporary)
        if artifact.suffix == '.AppImage':
            subprocess.run([str(artifact),'--appimage-extract'],cwd=stage,check=True,stdout=subprocess.DEVNULL)
        elif artifact.suffix == '.deb':
            subprocess.run(['bsdtar','-xf',str(artifact),'-C',str(stage)],check=True)
            data = next(stage.glob('data.tar.*'))
            subprocess.run(['bsdtar','-xf',str(data),'-C',str(stage)],check=True)
            for p in stage.glob('*.tar.*'): p.unlink()
        else:
            subprocess.run(['bsdtar','-xf',str(artifact),'-C',str(stage)],check=True)
        files = 0
        reviewed_false_positives = []
        for path in stage.rglob('*'):
            if path.is_symlink() or not path.is_file(): continue
            files += 1
            name = path.name.lower()
            if name in forbidden or 'syncthing' in name or 'python' in name or path.suffix in ('.py','.pyc'):
                raise SystemExit('Unexpected runtime/private entry: '+str(path.relative_to(stage)))
            data = path.read_bytes()
            if name == 'harness-sync' and os.fsencode(Path.home()) + b'/' in data:
                raise SystemExit('Developer home path embedded in packaged executable')
            for index, pattern in enumerate(patterns):
                for match in pattern.finditer(data):
                    if (index == 3 and name == 'libunistring.so.5'
                            and hashlib.sha256(data).hexdigest() == PUBLIC_TABLE_SHA256
                            and match.start() == 972850
                            and data[match.start()-8:match.start()] == b'INWARDIS'):
                        reviewed_false_positives.append({'file':str(path.relative_to(stage)), 'offset':match.start(), 'reason':'Manually reviewed public Unicode-name dictionary words; not a credential', 'sha256':PUBLIC_TABLE_SHA256})
                    elif (index == 0 and name == 'libgnutls.so.30'
                            and hashlib.sha256(data).hexdigest() == PUBLIC_GNUTLS_SHA256
                            and hashlib.sha256(match.group()).hexdigest() in PUBLIC_SELFTEST_MATCHES):
                        reviewed_false_positives.append({'file':str(path.relative_to(stage)), 'offset':match.start(), 'reason':'Public GnuTLS cryptographic self-test vector; exact match verified against official upstream source', 'match_sha256':hashlib.sha256(match.group()).hexdigest(), 'source':'https://github.com/gnutls/gnutls/blob/master/lib/crypto-selftests-pk.c'})
                    else:
                        raise SystemExit('Credential-like bytes require review in '+str(path.relative_to(stage)))
        results.append({'artifact':artifact.relative_to(root).as_posix(),'bytes':artifact.stat().st_size,'sha256':hashlib.sha256(artifact.read_bytes()).hexdigest(),'extracted_regular_files':files,'external_engine_or_private_state_files':False,'targeted_credentials_found':False,'reviewed_false_positives':reviewed_false_positives})
print(json.dumps({'candidates':results,'scope':'targeted filenames and common credential patterns; not exhaustive certification','installed':False},indent=2))
