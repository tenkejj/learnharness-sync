"""REAL two-process Syncthing test. No filesystem-copy/mock transport fallback.
Exit 0: recorded scenarios passed; 1: failure; 2: blocked prerequisite.
Only generated synthetic records and newly created private temp directories.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from harness_sync.core import Engine, SafetyError, collect, inventory, verify, write
from harness_sync.transport import Client, TransportError


def until(action, seconds=45):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = action()
            if value:
                return value
        except (SafetyError, OSError, KeyError, ValueError):
            pass
        time.sleep(0.25)
    raise RuntimeError('Bounded live transport wait expired')


def run(binary, report):
    prerequisites = []
    if not binary or not Path(binary).is_file():
        prerequisites.append('Syncthing executable unavailable')
    try:
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
    except OSError:
        prerequisites.append('Loopback sockets denied by environment')
    if prerequisites:
        report.update(state='blocked', blockers=prerequisites)
        return 2
    root = Path(tempfile.mkdtemp(prefix='harness-sync-real-p2p-')).resolve()
    root.chmod(0o700)
    report['workspace'] = str(root)  # reports kept private, never committed
    processes, logs, peers = [], [], []
    def free_port():
        with socket.socket() as sock:
            sock.bind(('127.0.0.1',0)); return sock.getsockname()[1]
    ports = set()
    while len(ports)<4: ports.add(free_port())
    ports = list(ports)
    try:
        for i, device in enumerate(('laptop','desktop')):
            engine=Engine.initialize(root/device,device)
            home=root/('syncthing-'+device);home.mkdir(mode=0o700)
            subprocess.run([binary,'generate','--home',str(home),'--no-port-probing'],check=True,
                           stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=20)
            config_path=home/'config.xml'
            config=ET.parse(config_path);tree=config.getroot()
            for folder in list(tree.findall('folder')):tree.remove(folder)
            options=tree.find('options')
            def option(name,value):
                node=options.find(name)
                if node is None: node=ET.SubElement(options,name)
                node.text=value
            for name in ('globalAnnounceEnabled','localAnnounceEnabled','relaysEnabled','natEnabled','crashReportingEnabled'):
                option(name,'false')
            option('urAccepted','-1');option('autoUpgradeIntervalH','0')
            for listen in list(options.findall('listenAddress')): options.remove(listen)
            ET.SubElement(options,'listenAddress').text=f'tcp://127.0.0.1:{ports[2+i]}'
            key=tree.find('gui/apikey').text
            tree.find('gui/address').text=f'127.0.0.1:{ports[i]}'
            config.write(config_path,encoding='utf-8',xml_declaration=True)
            key_file=home/'api.key';write(key_file,key.encode())
            client=Client(key_file,ports[i],timeout=2)
            log=open(home/'process.log','wb');logs.append(log)
            env=dict(os.environ,STNOUPGRADE='1',STNORESTART='1')
            process=subprocess.Popen([binary,'serve','--home',str(home),'--no-browser','--no-restart','--no-upgrade'],
                                     env=env,stdout=log,stderr=subprocess.STDOUT)
            processes.append(process)
            did=until(lambda:client.request('GET','/rest/system/status').get('myID'))
            peers.append((engine,client,did))
        (a,ca,aid),(b,cb,bid)=peers
        ca.pair(a,bid,'desktop','live-test',address=f'tcp://127.0.0.1:{ports[3]}',confirmed=True)
        cb.pair(b,aid,'laptop','live-test',address=f'tcp://127.0.0.1:{ports[2]}',confirmed=True)
        until(lambda:any(p['connected'] for p in ca.inspect()['peers']))
        report['passed'].append('two isolated real Syncthing processes paired over TCP loopback')
        def await_snapshot(engine, sid):
            def ready():
                folder=next(engine.store.glob('devices/*/snapshots/'+sid),None)
                return folder is not None and verify(folder)[0]['snapshot_id']==sid
            until(ready)
        (a.data/'settings.json').write_text('{"version":1,"synthetic":1}')
        first=a.export();await_snapshot(b,first);b.apply(first,bootstrap=True)
        assert inventory(collect(a.data))==inventory(collect(b.data))
        report['passed'].append('A export -> actual Syncthing transfer -> verified B bootstrap with backup')
        (b.data/'settings.json').write_text('{"version":1,"synthetic":2}')
        second=b.export();await_snapshot(a,second);a.apply(second)
        assert inventory(collect(a.data))==inventory(collect(b.data))
        report['passed'].append('B change -> actual Syncthing transfer -> A fast-forward')
        # Pause only the isolated process's known test device, never a user service.
        ca.request('PATCH','/rest/config/devices/'+bid,{'paused':True})
        until(lambda:not any(p['connected'] for p in ca.inspect()['peers']))
        (a.data/'settings.json').write_text('{"version":1,"synthetic":3}')
        third=a.export()
        assert b.state()['head']==second
        ca.request('PATCH','/rest/config/devices/'+bid,{'paused':False})
        await_snapshot(b,third);b.apply(third)
        report['passed'].append('offline local continuation and delayed reconnection delivery')
        (a.data/'settings.json').write_text('{"version":1,"synthetic":"left"}')
        (b.data/'settings.json').write_text('{"version":1,"synthetic":"right"}')
        left=a.export();right=b.export();await_snapshot(a,right);await_snapshot(b,left)
        before=inventory(collect(b.data))
        try:b.apply(left)
        except SafetyError:pass
        else:raise AssertionError('Divergent state applied')
        assert inventory(collect(b.data))==before
        assert left in b.snapshots() and right in b.snapshots()
        report['passed'].append('concurrent edits preserve both transferred branches and refuse overwrite')
        # Incomplete/corrupt transfer acceptance and import interruption are covered in
        # deterministic unit failpoint tests, not mislabeled as live network scenarios.
        report['not_covered']=['actual LearnHarness writer exclusion','power loss','installer/native lifecycle',
                               'network fault injection during one payload','Windows/macOS user flow']
        report['state']='passed_synthetic_p2p_only'
        return 0
    except Exception as exc:
        report.update(state='failed',error=type(exc).__name__,message='Inspect private test workspace; no production data was used')
        return 1
    finally:
        for process in processes:
            if process.poll() is None:
                process.terminate()
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
        for log in logs:log.close()

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--syncthing',default=shutil.which('syncthing'))
    parser.add_argument('--report',type=Path,default=Path('/tmp/harness-sync-p2p-report.json') if os.name!='nt' else Path(tempfile.gettempdir())/'harness-sync-p2p-report.json')
    args=parser.parse_args()
    report={'state':'not_run','passed':[],'transport':'real-syncthing-tcp','production_ready':False}
    code=run(args.syncthing,report)
    args.report.parent.mkdir(parents=True,exist_ok=True)
    from harness_sync.core import atomic_json
    atomic_json(args.report,report)
    print(json.dumps({k:v for k,v in report.items() if k!='workspace'},indent=2))
    sys.exit(code)
