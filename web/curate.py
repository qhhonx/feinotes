#!/usr/bin/env python3
"""Fix a sample library's sidebar order for the web demo: top-level entries in
the given order, everything inside folders by name. Runs on .quiet/index.json,
which the app writes on first open (web/README.md).

usage: web/curate.py <library dir> "<top name>" "<top name>" ...
"""
import json, sys, os
root, tops = sys.argv[1], sys.argv[2:]
p = os.path.join(root, '.quiet', 'index.json')
idx = json.load(open(p))
rank = {name: i + 1 for i, name in enumerate(tops)}
by_parent = {}
for key in idx:
    k = key.rstrip('/')
    parent = os.path.dirname(k)
    by_parent.setdefault(parent, []).append(key)
for parent, keys in by_parent.items():
    keys.sort(key=lambda k: (k.endswith('/') is False, os.path.basename(k.rstrip('/')).lower()))
    for i, key in enumerate(keys):
        m = idx[key]
        if parent == '':
            m['root_order'] = rank.get(os.path.basename(key.rstrip('/')), len(rank) + i + 1)
        else:
            m['order'] = i + 1
json.dump(idx, open(p, 'w'), ensure_ascii=False, indent=1)
print(f'{p}: {len(idx)} entries')
