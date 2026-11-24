"""Synthetic automatic cycle only. Production automation is impossible by construction."""
from .core import Engine, SafetyError, collect, inventory


def tick(workspace):
    engine = Engine(workspace)
    engine.recover()
    state = engine.state()
    if not state['head']:
        return {'state': 'awaiting_authority', 'message': 'Choose initial synthetic authority explicitly before automatic synchronization.'}
    if inventory(collect(engine.data)) != state['baseline']:
        engine.export()
    history = engine.snapshots()
    head = engine.state()['head']
    candidates = [sid for sid in history if sid != head and engine.ancestor(head, sid, history)]
    divergent = [sid for sid in history if not engine.ancestor(head, sid, history) and not engine.ancestor(sid, head, history)]
    if divergent:
        return {'state': 'conflict', 'message': 'Both synthetic histories retained; no automatic branch choice.'}
    tips = [sid for sid in candidates if not any(sid != other and engine.ancestor(sid, other, history) for other in candidates)]
    if len(tips) == 1:
        engine.apply(tips[0])
        return {'state': 'applied', 'snapshot': tips[0]}
    if len(tips) > 1:
        return {'state': 'conflict', 'message': 'Multiple causal successors; no branch choice.'}
    return {'state': 'waiting', 'message': 'No verified newer snapshot locally available; remote freshness unknown.'}
