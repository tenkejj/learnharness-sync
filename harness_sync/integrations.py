"""Generate reversible native autostart plans, never install or activate them.

Only the observer/synthetic cycle is eligible. These plans cannot enable
production synchronization or replace a LearnHarness launcher.
"""
from pathlib import Path
import plistlib
import sys
import xml.etree.ElementTree as ET
from .core import SafetyError


def startup_plan(executable, profile, platform=None):
    platform = platform or sys.platform
    executable, profile = str(Path(executable).absolute()), str(Path(profile).absolute())
    if any(c in executable + profile for c in ('\r', '\n', '\x00')):
        raise SafetyError('Unsafe integration path')
    argv = [executable, 'watch', '--profile', profile]
    if platform == 'darwin':
        value = {'Label': 'io.github.tenkejj.learnharness-sync.observer', 'ProgramArguments': argv,
                 'RunAtLoad': True, 'KeepAlive': {'SuccessfulExit': False}, 'ProcessType': 'Background',
                 'Umask': 0o077}
        return {'filename': 'io.github.tenkejj.learnharness-sync.observer.plist', 'content': plistlib.dumps(value).decode(),
                'activate': 'Review and explicitly approve launchctl bootstrap gui/UID PATH',
                'undo': 'launchctl bootout gui/UID/io.github.tenkejj.learnharness-sync.observer'}
    if platform == 'win32':
        import subprocess
        namespace = 'http://schemas.microsoft.com/windows/2004/02/mit/task'
        ET.register_namespace('', namespace)
        def child(parent, name, text=None, **attributes):
            node = ET.SubElement(parent, '{' + namespace + '}' + name, attributes)
            node.text = text
            return node
        task = ET.Element('{' + namespace + '}Task', {'version': '1.2'})
        child(child(task, 'Triggers'), 'LogonTrigger')
        principals = child(task, 'Principals')
        principal = child(principals, 'Principal', id='CurrentUser')
        child(principal, 'LogonType', 'InteractiveToken')
        child(principal, 'RunLevel', 'LeastPrivilege')
        settings = child(task, 'Settings')
        child(settings, 'MultipleInstancesPolicy', 'IgnoreNew')
        child(settings, 'ExecutionTimeLimit', 'PT0S')
        actions = child(task, 'Actions', Context='CurrentUser')
        execute = child(actions, 'Exec')
        child(execute, 'Command', executable)
        child(execute, 'Arguments', subprocess.list2cmdline(argv[1:]))
        return {'filename': 'LearnHarnessSyncObserver.xml', 'content': ET.tostring(task, encoding='unicode'),
                'activate': 'Review and explicitly approve schtasks /Create /TN LearnHarnessSyncObserver /XML PATH',
                'undo': 'schtasks /Delete /TN LearnHarnessSyncObserver /F'}
    # systemd quoting prevents specifier and argument injection.
    def quote(value):
        return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%') + '"'
    content = '[Unit]\nDescription=LearnHarness Sync observer\n\n[Service]\nType=simple\nExecStart=' + ' '.join(quote(v) for v in argv)
    content += '\nUMask=0077\nRestart=on-failure\nNoNewPrivileges=true\n\n[Install]\nWantedBy=default.target\n'
    return {'filename': 'learnharness-sync-observer.service', 'content': content,
            'activate': 'Review and explicitly approve installation and systemctl --user enable --now learnharness-sync-observer',
            'undo': 'systemctl --user disable --now learnharness-sync-observer; remove only its reviewed unit'}
