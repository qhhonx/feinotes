#!/usr/bin/env python3
"""Pack a Quiet.md library folder into one .qmdb bundle for the web build.

A bundle is a run of [u32 LE path length][path][u32 LE data length][data], paths
relative to the library and joined with '/'. src/web.rs unpacks it into memory.
Pictures (.assets/) stay out of the bundle: they are copied next to it, into
<out without .qmdb>/, and listed in the bundle's .web/lazy, so the page fetches
each one when it is first shown. Absolute paths into the library (an import
writes them) become library-relative.

usage: web/bundle.py <library dir> <out.qmdb>
"""
import os, shutil, struct, sys

root, out = os.path.abspath(sys.argv[1]), sys.argv[2]
lazy_dir = out[:-len('.qmdb')] if out.endswith('.qmdb') else out + '.files'
SKIP = {'.DS_Store'}
prefixes = {root + '/', os.path.realpath(root) + '/'}
n = size = 0
lazy = []
shutil.rmtree(lazy_dir, ignore_errors=True)

def put(f, rel, data):
    rel = rel.encode()
    f.write(struct.pack('<I', len(rel)) + rel + struct.pack('<I', len(data)) + data)

with open(out, 'wb') as f:
    for dirpath, dirs, files in os.walk(root):
        dirs.sort()
        for name in sorted(files):
            if name in SKIP or name.endswith(('.bak', '.lock')): continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root).replace(os.sep, '/')
            data = open(path, 'rb').read()
            if rel.startswith('.assets/'):
                os.makedirs(os.path.dirname(os.path.join(lazy_dir, rel)), exist_ok=True)
                shutil.copyfile(path, os.path.join(lazy_dir, rel))
                lazy.append(rel)
                continue
            if name.endswith(('.md', '.xml')):
                text = data.decode()
                for p in prefixes: text = text.replace(p, '')
                data = text.encode()
            put(f, rel, data)
            n += 1; size += len(data)
    put(f, '.web/lazy', '\n'.join(lazy).encode())
print(f'{out}: {n} files, {size / 1e6:.1f} MB; {len(lazy)} pictures in {lazy_dir}')
