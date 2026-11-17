"""Offline Mac startup regression. Use a cloudkit-sync,devtools build only.

All libraries are newly generated synthetic fixtures. Transport is paused;
no real account, clipboard, installed app or user library is used.
FDL_CLOUD_BIN and FDL_CLOUD_DYLD_PATH select the executable and Swift bridge.
"""
import hashlib
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import uuid

repo = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get('FDL_CLOUD_BIN', repo / 'target/debug/quiet_md')).resolve()
bridge = os.environ.get('FDL_CLOUD_DYLD_PATH')
assert bridge, 'Set FDL_CLOUD_DYLD_PATH to the matching Swift bridge directory'
account = 'a' * 64
channel = str(uuid.uuid4())

def fresh():
    return Path(tempfile.mkdtemp(prefix='quiet-sync-qa-startup-', dir='/private/tmp'))

def run(root, script, interactive):
    path = root / 'startup.txt'
    path.write_text(script)
    env = dict(os.environ, QUIET_SYNC_QA_ROOT=str(root), QUIET_SYNC_QA_CHANNEL=channel,
               QUIET_SYNC_QA_SHARED_SESSIONS='1', FDL_SCRIPT=str(path), FDL_SHOTS=str(root),
               FDL_NOBLINK='1', QUIET_MD_LOG_DIR=str(root / 'logs'), DYLD_LIBRARY_PATH=bridge)
    env.pop('QUIET_SYNC_QA_STARTUP', None)
    if interactive:
        env['QUIET_SYNC_QA_STARTUP'] = '1'
    with (root / 'run.log').open('w') as log:
        subprocess.run([str(binary)], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT,
                       timeout=45, check=True)
    return (root / 'run.log').read_text()

seed = fresh()
run(seed, 'wait 500\nquit\n', False)
assert (seed / account / 'library.sqlite3').is_file()

def case(name, fixture, script, local=False):
    root = fresh()
    shutil.copytree(seed, root, dirs_exist_ok=True)
    (root / 'active-account').write_text(account)
    (root / 'verified-account').write_text(account)
    (root / 'qa-startup-account').write_text(fixture)
    if fixture == 'bound-cache':
        (root / account / '.device-identity').write_text(hashlib.sha256(b'QA identity').hexdigest())
    if local:
        (root / 'use-local-library').touch()
    text = run(root, script, True)
    print(name, root, flush=True)
    return root, text

root, text = case('verified-cache', 'bound-cache', 'wait 150\nshot cache\nquit\n')
assert 'CloudKit sync library source=local\n' in text

root, text = case('background-restore', 'slow-cache', 'wait 150\nshot first-frame\nwait 3700\nshot connected\nquit\n')
assert text.index('first-frame.png') < text.index('CloudKit sync library source=local\n')
assert f'library {root / account}' in text

root, text = case('unverified-cache-unavailable', 'slow-unavailable', 'wait 1000\nshot fallback\nquit\n')
assert 'CloudKit sync library source=localOnly' in text
assert 'CloudKit sync library source=local\n' not in text

root, text = case('editing-while-waiting', 'slow-cache',
                  'wait 150\nkey cmd-n\nwait 250\ntype Startup local edit\nwait 4000\nshot retained\nquit\n')
assert 'CloudKit sync library source=localOnly' in text
assert 'CloudKit sync library source=local\n' not in text
with sqlite3.connect(f'file:{root / "local-only/library.sqlite3"}?mode=ro', uri=True) as db:
    assert db.execute('SELECT count(*) FROM library_documents WHERE title=?', ('Startup local edit',)).fetchone()[0] == 1
assert not (root / 'use-local-library').exists(), 'Implicit local editing must not change the explicit library preference'

root, text = case('explicit-local', 'slow-cache',
                  'wait 150\nkey cmd-n\nwait 250\ntype Explicit local note\nwait 800\nquit\n', local=True)
assert 'CloudKit sync library source=localOnly' in text and (root / 'use-local-library').exists()
with sqlite3.connect(f'file:{root / "local-only/library.sqlite3"}?mode=ro', uri=True) as db:
    assert db.execute('SELECT count(*) FROM library_documents WHERE title=?', ('Explicit local note',)).fetchone()[0] == 1

root = fresh()
(root / 'qa-startup-account').write_text('slow-signed-out')
(root / 'pause-cloud').touch()
text = run(root, 'wait 150\nshot first-install\nkey cmd-n\nwait 250\ntype Fresh local edit\nwait 16000\nshot still-editing\nquit\n', True)
assert (root / 'verified-account').read_text() == 'signed-out', 'First install must attempt account discovery without blocking editing'
assert not (root / 'active-account').exists() and not (root / 'use-local-library').exists()
with sqlite3.connect(f'file:{root / "local-only/library.sqlite3"}?mode=ro', uri=True) as db:
    assert db.execute('SELECT count(*) FROM library_documents WHERE title=?', ('Fresh local edit',)).fetchone()[0] == 1
print('first-install', root, flush=True)
print('STARTUP_OK verified-cache background-restore editing-retained explicit-local shortcut first-install', flush=True)
