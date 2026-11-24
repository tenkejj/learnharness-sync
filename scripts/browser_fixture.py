"""Render-only fixture from real engine state; no claims of live API verification."""
import base64
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from harness_sync.status import status
root = Path(__file__).resolve().parent.parent
view = status(sys.argv[1])
assert view['mode'] == 'synthetic'
from harness_sync.core import Engine
view['publication_id'] = Engine(sys.argv[1]).state()['device']
css = next((root / 'dist/assets').glob('*.css')).read_text()
js = next((root / 'dist/assets').glob('*.js')).read_text()
logo = base64.b64encode((root / 'assets/brand/learnharness-sync-app-icon-512.png').read_bytes()).decode()
js = js.replace('/brand/learnharness-sync-app-icon-512.png', 'data:image/png;base64,' + logo)
projection = json.dumps(view).replace('<', '\\u003c')
from harness_sync.profile import DEFAULTS
configuration = json.dumps(dict(DEFAULTS, workspace=str(Path(sys.argv[1]).absolute()), api_key_configured=False, production_mutation=False))
fixture_js = 'window.fetch=async(url,options)=>{let value=' + projection + ';let status=200;if(url==="/api/session")value={token:"render-only"};if(url==="/api/command"){if(JSON.parse(options.body).command==="settings")value={result:' + configuration + '};else{value={error:"Render-only fixture: mutations are not executed"};status=409}}return new Response(JSON.stringify(value),{status,headers:{"Content-Type":"application/json"}})};'
html = '<!doctype html><html lang="en"><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>LearnHarness Sync synthetic render fixture</title><style>' + css + '</style><div id="root"></div><script>' + fixture_js + '</script><script type="module">' + js + '</script></html>'
Path('/tmp/harness-sync-preview.html').write_text(html)
print('/tmp/harness-sync-preview.html — synthetic projection, no live API')
