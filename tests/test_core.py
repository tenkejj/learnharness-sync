import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
from harness_sync.core import Engine, SafetyError, canonical, collect, digest, inventory, mkdir, parse, read_file, relative, verify, write


class EngineTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='harness-sync-test-')
        self.root = Path(self.tmp.name).resolve()
        self.a = Engine.initialize(self.root / 'a', 'laptop')
        self.b = Engine.initialize(self.root / 'b', 'desktop')

    def tearDown(self):
        self.tmp.cleanup()

    def fixture(self, engine, title='alpha'):
        mkdir(engine.data / 'chats')
        (engine.data / 'settings.json').write_bytes(canonical({'version': 1}))
        (engine.data / 'chats/synthetic.json').write_bytes(canonical({'version': 1, 'id': 'synthetic', 'title': title}))

    def transfer(self, source, dest):
        shutil.copytree(source.store, dest.store, dirs_exist_ok=True)

    def bootstrap(self):
        self.fixture(self.a)
        sid = self.a.export()
        self.transfer(self.a, self.b)
        self.b.apply(sid, bootstrap=True)
        return sid

    def test_roundtrip_fast_forward_and_idempotence(self):
        first = self.bootstrap()
        self.assertEqual(self.b.apply(first), 'Already applied')
        self.assertEqual(self.a.export(), first)
        self.fixture(self.b, 'beta')
        second = self.b.export()
        self.transfer(self.b, self.a)
        self.a.apply(second)
        self.assertEqual(collect(self.a.data), collect(self.b.data))
        self.assertTrue(Engine.ancestor(first, second, self.a.snapshots()))

    def test_two_nonempty_peers_require_authority(self):
        self.fixture(self.a); self.fixture(self.b, 'local')
        sid = self.a.export(); self.transfer(self.a, self.b)
        before = collect(self.b.data)
        with self.assertRaises(SafetyError): self.b.apply(sid, bootstrap=True)
        self.assertEqual(before, collect(self.b.data))

    def test_bootstrap_must_be_explicit(self):
        self.fixture(self.a); sid = self.a.export(); self.transfer(self.a, self.b)
        with self.assertRaises(SafetyError): self.b.apply(sid)

    def test_offline_delayed_transfer(self):
        first = self.bootstrap(); self.fixture(self.a, 'offline'); new = self.a.export()
        self.assertEqual(self.b.state()['head'], first)
        with self.assertRaises(SafetyError): self.b.apply(new)
        self.transfer(self.a, self.b); self.b.apply(new)
        self.assertEqual(self.b.state()['head'], new)

    def test_divergence_preserves_both(self):
        self.bootstrap()
        self.fixture(self.a, 'left'); self.fixture(self.b, 'right')
        a = self.a.export(); b = self.b.export(); self.transfer(self.a, self.b)
        before = collect(self.b.data)
        with self.assertRaises(SafetyError): self.b.apply(a)
        self.assertIn(a, self.b.snapshots()); self.assertIn(b, self.b.snapshots())
        self.assertEqual(before, collect(self.b.data))

    def test_uncommitted_local_edit_and_replay(self):
        first = self.bootstrap(); self.fixture(self.b, 'unpublished')
        with self.assertRaises(SafetyError): self.b.apply(first)
        self.fixture(self.a, 'remote'); new = self.a.export(); self.transfer(self.a, self.b)
        with self.assertRaises(SafetyError): self.b.apply(new)
        self.assertIn(b'unpublished', read_file(self.b.data / 'chats/synthetic.json'))

    def test_deletions_propagate_without_resurrection(self):
        self.bootstrap(); (self.a.data / 'chats/synthetic.json').unlink()
        sid = self.a.export(); self.transfer(self.a, self.b); self.b.apply(sid)
        self.assertFalse((self.b.data / 'chats/synthetic.json').exists())
        self.assertTrue(list(self.b.root.glob('recovery-*')))

    def test_writer_marker_and_engine_lock(self):
        self.fixture(self.a); (self.a.root / 'WRITER_ACTIVE').touch()
        with self.assertRaises(SafetyError): self.a.export()
        (self.a.root / 'WRITER_ACTIVE').unlink()
        with self.a.locked():
            with self.assertRaises(SafetyError): self.a.export()

    def test_unknown_ancestry_is_blocked(self):
        sid = self.bootstrap()
        fake = copy.deepcopy(self.a.snapshots())
        manifest = fake[sid][0]; manifest['parent_ids'] = ['f'*64]
        with self.assertRaises(SafetyError): Engine.ancestor('e'*64, sid, fake)

    def test_clock_skew_does_not_select_revision(self):
        with patch('harness_sync.core.time.time', return_value=9999999999): first = self.bootstrap()
        self.fixture(self.a, 'new')
        with patch('harness_sync.core.time.time', return_value=1): second = self.a.export()
        self.transfer(self.a, self.b); self.b.apply(second)
        self.assertNotEqual(first, second)

    def test_every_import_interrupt_rolls_back_or_commits(self):
        for stage in ('prepared', 'old-renamed', 'new-renamed', 'ledger-written', 'committed'):
            with self.subTest(stage=stage):
                engine = Engine.initialize(self.root / ('peer-' + stage), 'peer')
                self.fixture(self.a, stage); sid = self.a.export(); self.transfer(self.a, engine)
                before = collect(engine.data)
                with self.assertRaises(SafetyError): engine.apply(sid, bootstrap=True, failpoint=stage)
                engine.recover(); engine.recover()
                if stage == 'committed':
                    self.assertEqual(engine.state()['head'], sid)
                    self.assertEqual(collect(engine.data), collect(self.a.data))
                else:
                    self.assertIsNone(engine.state()['head'])
                    self.assertEqual(before, collect(engine.data))
                self.assertTrue(list((engine.root / 'backups').glob('*.json')))

    def test_corruption_missing_file_and_incomplete_transfer(self):
        self.fixture(self.a); sid = self.a.export(); self.transfer(self.a, self.b)
        folder = self.b.snapshots()[sid][2]
        (folder / 'COMPLETE').unlink()
        with self.assertRaises(SafetyError): self.b.apply(sid, bootstrap=True)
        (folder / 'COMPLETE').write_text(sid)
        (folder / 'payload/settings.json').write_text('{"version":1,"tampered":true}')
        with self.assertRaises(SafetyError): verify(folder)
        (folder / 'payload/settings.json').unlink()
        with self.assertRaises(SafetyError): verify(folder)

    @unittest.skipIf(os.name == 'nt', 'Unix FIFO/symlink test; Windows native link tests required separately')
    def test_symlink_hardlink_fifo_and_paths(self):
        self.fixture(self.a)
        external = self.root / 'external'; external.write_text('{"version":1}')
        p = self.a.data / 'settings.json'; p.unlink(); p.symlink_to(external)
        with self.assertRaises((SafetyError, OSError)): self.a.export()
        p.unlink(); os.link(external, p)
        with self.assertRaises(SafetyError): self.a.export()
        p.unlink(); os.mkfifo(p)
        with self.assertRaises(SafetyError): self.a.export()
        for name in ('../a.json', '/tmp/a.json', 'chats/../a.json', 'chats//a.json', 'auth.json', '.env', 'plugins/executable.js', 'chats/token.json', 'chats\\a.json'):
            with self.subTest(name=name), self.assertRaises(SafetyError): relative(name)

    def test_schema_secret_and_reference_guard(self):
        self.fixture(self.a)
        (self.a.data / 'settings.json').write_text('{"version":99}')
        with self.assertRaises(SafetyError): self.a.export()
        (self.a.data / 'settings.json').write_text('{"version":1}')
        mkdir(self.a.data / 'graph'); (self.a.data / 'graph/nodes.jsonl').write_text('{"chatId":"missing"}\n')
        with self.assertRaises(SafetyError): self.a.export()
        (self.a.data / 'graph/nodes.jsonl').unlink(); (self.a.data / 'auth.json').write_text('{"key":"PRIVATE"}')
        with self.assertRaises(SafetyError) as error: self.a.export()
        self.assertNotIn('PRIVATE', str(error.exception))

    def test_manifest_protocol_tampering_and_duplicate_json(self):
        self.fixture(self.a); sid = self.a.export(); folder = self.a.snapshots()[sid][2]
        manifest = parse(read_file(folder / 'manifest.json')); manifest['protocol_version'] = 999
        body = {k:v for k,v in manifest.items() if k != 'snapshot_id'}
        manifest['snapshot_id'] = digest(canonical(body))
        (folder / 'manifest.json').write_bytes(canonical(manifest))
        with self.assertRaises(SafetyError): verify(folder)
        for raw in (b'{"a":1,"a":2}', b'{', b'{"a":NaN}'):
            with self.assertRaises(SafetyError): parse(raw)

    @unittest.skipIf(os.name == 'nt', 'POSIX permission assertion; Windows ACL verification remains open')
    def test_limits_and_permissions(self):
        self.fixture(self.a); sid = self.a.export(); folder = self.a.snapshots()[sid][2]
        self.assertEqual((folder / 'manifest.json').stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.a.root.stat().st_mode & 0o777, 0o700)
        with patch('harness_sync.core.MAX_TOTAL', 1):
            with self.assertRaises(SafetyError): collect(self.a.data)

    def test_existing_directories_are_never_adopted(self):
        existing = self.root / 'production'; existing.mkdir()
        with self.assertRaises(SafetyError): Engine.initialize(existing, 'test')
        with self.assertRaises((SafetyError, OSError)): Engine(existing)

    def test_partial_publication_is_invisible(self):
        self.fixture(self.a)
        folder = self.a.store / 'devices/laptop/snapshots/.pending-interrupted'
        mkdir(folder); write(folder / 'manifest.json', b'{')
        self.assertEqual(self.a.snapshots(), {})
        sid = self.a.export(); self.assertIn(sid, self.a.snapshots())

    def test_graph_edits_diverge_no_union(self):
        self.bootstrap()
        for engine, label in ((self.a, 'left'), (self.b, 'right')):
            mkdir(engine.data / 'graph')
            (engine.data / 'graph/nodes.jsonl').write_bytes(canonical({'id':label}) + b'\n')
        a = self.a.export(); self.b.export(); self.transfer(self.a, self.b)
        with self.assertRaises(SafetyError): self.b.apply(a)
        self.assertNotIn(b'left', read_file(self.b.data / 'graph/nodes.jsonl'))

    @unittest.skipIf(os.name == 'nt', 'Windows reparse-point creation requires a dedicated native test')
    def test_transport_symlink_never_writes_outside_workspace(self):
        self.fixture(self.a)
        outside = self.root / 'outside'; outside.mkdir()
        (self.a.store / 'devices').symlink_to(outside, target_is_directory=True)
        with self.assertRaises(SafetyError): self.a.export()
        self.assertEqual(list(outside.iterdir()), [])

    def test_noop_export_requires_existing_verified_snapshot(self):
        self.fixture(self.a); sid = self.a.export()
        shutil.rmtree(self.a.snapshots()[sid][2])
        with self.assertRaises(SafetyError): self.a.export()

    def test_history_index_does_not_retain_payloads(self):
        self.fixture(self.a); self.a.export()
        self.assertTrue(all(payload is None for _, payload, _ in self.a.snapshots().values()))

    def test_backups_are_not_transport_conflicts(self):
        self.bootstrap()
        from harness_sync.status import status
        state = status(self.b.root)
        self.assertEqual(state['conflicts'], [])
        self.assertEqual(len(state['backups']), 1)

    def test_restore_refused_without_resolution(self):
        with self.assertRaises(SafetyError): self.a.restore('a'*64, 'yes')

if __name__ == '__main__': unittest.main()
