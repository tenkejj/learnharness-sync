"""Create synthetic A/B peers with a real fast-forward; no production reads."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from harness_sync.core import Engine, write, canonical, mkdir
import shutil

root = Path(sys.argv[1])
if root.exists():
    raise SystemExit('Use a new demo directory')
a = Engine.initialize(root / 'laptop', 'laptop')
b = Engine.initialize(root / 'desktop', 'desktop')
mkdir(a.data / 'chats'); mkdir(a.data / 'graph')
write(a.data / 'settings.json', canonical({'version': 1}))
write(a.data / 'chats' / 'synthetic.json', canonical({'id': 'synthetic', 'version': 1, 'title': 'Synthetic fixture — orbital mechanics'}))
write(a.data / 'graph' / 'nodes.jsonl', canonical({'id': 'n1', 'chatId': 'synthetic'}) + b'\n')
sid = a.export()
shutil.copytree(a.store / 'devices', b.store / 'devices')
b.apply(sid, bootstrap=True)
print(f'Synthetic peers ready. UI: python -m harness_sync desktop --workspace {b.root}')
