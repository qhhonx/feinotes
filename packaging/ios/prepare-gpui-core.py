#!/usr/bin/env python3
"""Prepare the pinned GPUI core image-cache extension from its verified crate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--crate',type=Path)
parser.add_argument('--output',type=Path)
args = parser.parse_args()
base = Path(__file__).resolve().parent
metadata = json.loads((base/'patches/gpui-pre-image-cache.json').read_text())
patch = base/'patches/gpui-pre-image-cache.patch'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def verify(root):
    actual = {str(p.relative_to(root)):sha(p) for p in root.rglob('*') if p.is_file()}
    if actual != metadata['prepared_tree_sha256']:
        raise SystemExit('Prepared GPUI core differs from the pinned source manifest.')
assert sha(patch) == metadata['patch_sha256'], 'GPUI patch checksum changed'
destination = args.output or base/'.gpui-core'
if destination.exists():
    verify(destination)
    print('Pinned GPUI core already prepared and verified.')
    raise SystemExit(0)
archive = args.crate
if archive is None:
    cargo = Path(os.environ.get('CARGO_HOME',str(Path.home()/'.cargo')))
    matches = sorted((cargo/'registry/cache').glob('*/gpui-pre-0.3.10.crate'))
    if matches:
        archive = matches[0]
    else:
        download = tempfile.TemporaryDirectory(prefix='fei-gpui-download-')
        archive = Path(download.name)/'gpui-pre-0.3.10.crate'
        with urllib.request.urlopen('https://static.crates.io/crates/gpui-pre/gpui-pre-0.3.10.crate',timeout=60) as response:
            data = response.read(20*1024*1024+1)
        assert len(data) <= 20*1024*1024, 'GPUI archive exceeds the expected size limit'
        archive.write_bytes(data)
assert sha(archive) == metadata['archive_sha256'], 'GPUI crate checksum mismatch'
with tempfile.TemporaryDirectory(prefix='fei-gpui-core-') as tmp:
    staging = Path(tmp)
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            path = Path(member.name)
            assert not path.is_absolute() and '..' not in path.parts
            assert path.parts[0] == 'gpui-pre-0.3.10'
            assert not member.issym() and not member.islnk()
        tar.extractall(staging)
    source = staging/'gpui-pre-0.3.10'
    subprocess.run(['git','apply','--check',str(patch)],cwd=source,check=True)
    subprocess.run(['git','apply',str(patch)],cwd=source,check=True)
    verify(source)
    destination.parent.mkdir(parents=True,exist_ok=True)
    shutil.copytree(source,destination)
print('Prepared and verified GPUI core 0.3.10 image-cache extension.')
