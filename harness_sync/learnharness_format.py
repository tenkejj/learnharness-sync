"""Read-only, conservative structural audit of an ISOLATED LearnHarness 0.6.1 tree.

Independent implementation of documented record shapes, NOT a complete upstream
semantic validator. Does not enable production snapshots or read attachments.
"""
from pathlib import Path
import os
import re
from .core import SafetyError, MAX_TOTAL, MAX_FILES, parse, read_file
from .platforms import linked

IDS = re.compile(r'^[A-Za-z0-9_-]+$')
GRAPH = {'format.json', 'nodes.jsonl', 'chains.jsonl', 'sessions.jsonl', 'coverage.jsonl',
         'sources.json', 'positions.json', 'render-settings.json', 'source-organization.json'}
SENSITIVE = {'apikey', 'api_key', 'authorization', 'access_token', 'refresh_token', 'password', 'secret'}


def object_value(value):
    if not isinstance(value, dict):
        raise SafetyError('Expected an object in a LearnHarness record')
    return value


def fields(value, names):
    object_value(value)
    if any(not isinstance(value.get(name), str) for name in names):
        raise SafetyError('Missing or incompatible LearnHarness record fields')


def version(value):
    if type(value.get('version', 1)) is not int or value.get('version', 1) != 1:
        raise SafetyError('Unsupported LearnHarness schema version')


def inspect_isolated(root, app_version='0.6.1'):
    if app_version != '0.6.1':
        raise SafetyError('Application release has not been structurally audited')
    root = Path(root).absolute()
    if not root.is_dir() or any(linked(p) for p in [root, *root.parents]):
        raise SafetyError('Unsafe isolated data root')
    records, total, references = {}, 0, []
    paths = []
    def walk_error(_error):
        raise SafetyError('A data directory could not be inspected; no files may be omitted')
    for base, dirs, files in os.walk(root, followlinks=False, onerror=walk_error):
        for name in dirs:
            p = Path(base) / name
            if linked(p) or p.relative_to(root).parts[0] not in ('chats', 'jobs', 'graph'):
                raise SafetyError('Unknown data directory needs an audited adapter; no files omitted')
        for name in files:
            p = Path(base) / name
            relative = p.relative_to(root)
            parts = relative.parts
            allowed = relative.as_posix() == 'settings.json' or (
                len(parts) == 2 and ((parts[0] in ('chats', 'jobs') and p.suffix == '.json' and IDS.fullmatch(p.stem))
                                     or (parts[0] == 'graph' and name in GRAPH)))
            if not allowed:
                raise SafetyError('Unknown or sensitive data file needs an audited adapter; no files omitted')
            data = read_file(p)
            total += len(data)
            if total > MAX_TOTAL or len(records) >= MAX_FILES:
                raise SafetyError('Isolated data exceeds inspection limits')
            value = [object_value(parse(line)) for line in data.splitlines() if line.strip()] if p.suffix == '.jsonl' else object_value(parse(data))
            records[relative.as_posix()] = value
    chats = {}
    for path, value in records.items():
        if path.startswith('chats/'):
            version(value)
            fields(value, ('id', 'title', 'createdAt', 'updatedAt'))
            if value['id'] != Path(path).stem or not isinstance(value.get('messages'), list):
                raise SafetyError('Chat identity or message list is incompatible')
            for message in value['messages']:
                fields(message, ('role',))
                if message['role'] not in ('user', 'assistant', 'toolResult'):
                    raise SafetyError('Unknown native message role requires an adapter')
            chats[value['id']] = value
    nodes = set()
    for node in records.get('graph/nodes.jsonl', []):
        fields(node, ('id', 'name', 'statement', 'createdAt'))
        if not isinstance(node.get('aliases'), list) or not all(isinstance(x, str) for x in node['aliases']):
            raise SafetyError('Graph node aliases are incompatible')
        nodes.add(node['id'])
        references.extend(node.get('provenance', []))
    for chain in records.get('graph/chains.jsonl', []):
        fields(chain, ('id', 'conclusion', 'createdAt'))
        premises = chain.get('premises')
        if not isinstance(premises, list) or not premises or any(not isinstance(x, str) or x not in nodes for x in premises) or chain['conclusion'] not in nodes:
            raise SafetyError('Graph chain refers to missing propositions')
        references.extend(chain.get('provenance', []))
    sources = records.get('graph/sources.json', {})
    for sid, source in sources.items():
        fields(source, ('id', 'filePath', 'importedAt'))
        if source['id'] != sid:
            raise SafetyError('Notes source identity mismatch')
    for reference in references:
        fields(reference, ('chatId', 'messageId'))
        if reference['chatId'] not in chats and reference['chatId'] not in sources:
            raise SafetyError('Graph provenance refers to absent chat/source')
    for name in ('sessions', 'coverage'):
        for entry in records.get('graph/' + name + '.jsonl', []):
            fields(entry, ('chatId', 'foldedAt'))
            if entry['chatId'] not in chats and entry['chatId'] not in sources:
                raise SafetyError('Graph checkpoint refers to absent chat/source')
    if 'graph/format.json' in records:
        version(records['graph/format.json'])
        if 'version' not in records['graph/format.json']:
            raise SafetyError('Explicit graph format stamp malformed')
    if 'settings.json' in records:
        version(records['settings.json'])
    for path, value in records.items():
        if path.startswith('jobs/'):
            fields(value, ('id', 'chatId', 'toolCallId', 'activity', 'createdAt', 'updatedAt'))
            if value['id'] != Path(path).stem or value['chatId'] not in chats or value.get('kind') not in ('research', 'demo', 'visual', 'pdf'):
                raise SafetyError('Job identity, owner or kind is incompatible')
            if value.get('status') not in ('done', 'error'):
                raise SafetyError('Unfinished job means the tree is not ready for snapshot approval')
    def inspect(value):
        if isinstance(value, dict):
            for key, child in value.items():
                if key.lower() in SENSITIVE:
                    raise SafetyError('Credential-like field requires private local handling')
                if key in ('filePath', 'path') and isinstance(child, str) and Path(child).is_absolute():
                    paths.append(True)  # count only; never expose or read referenced paths
                inspect(child)
        elif isinstance(value, list):
            for item in value:
                inspect(item)
    for value in records.values():
        inspect(value)
    return {'application_version': app_version, 'structural_checks_passed': True, 'files': len(records),
            'chats': len(chats), 'nodes': len(nodes), 'bytes': total, 'external_path_references': len(paths),
            'production_compatible': False, 'limitations': ['Native message, plugin and optional record semantics not fully validated',
                                                          'This structural audit is not authorization to import production data']}
