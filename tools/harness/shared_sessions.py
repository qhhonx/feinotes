"""Synthetic, offline native regression for personal and shared library sessions.

Build with cloudkit-sync,devtools. No CloudKit account, real library, system
clipboard or foreground activation is used. FDL_CLOUD_BIN and
FDL_CLOUD_DYLD_PATH select the test executable and matching Swift bridge.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

repo = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get('FDL_CLOUD_BIN', repo / 'target/debug/quiet_md'))
root = Path(tempfile.mkdtemp(prefix='quiet-sync-qa-sessions-', dir='/private/tmp'))
script = root / 'script.txt'
script.write_text('wait 1200\ncheck-shared-sessions\nwait 500\nquit\n')
env = dict(os.environ, QUIET_SYNC_QA_ROOT=str(root), QUIET_SYNC_QA_CHANNEL=str(uuid.uuid4()), QUIET_SYNC_QA_SHARED_SESSIONS='1',
           FDL_SCRIPT=str(script), FDL_SHOTS=str(root), FDL_NOBLINK='1',
           QUIET_MD_LOG_DIR=str(root / 'logs'),
           DYLD_LIBRARY_PATH=os.environ.get('FDL_CLOUD_DYLD_PATH', str(binary.parent)))
print(f'Artifacts: {root}', flush=True)
with (root / 'run.log').open('w') as log:
    process = subprocess.Popen([binary], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        assert process.wait(timeout=60) == 0, f'See {root / "run.log"}'
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
text = (root / 'run.log').read_text()
assert 'SHARED_SESSIONS_OK' in text, text[-3000:]
assert len(list(root.glob('shared-session-*.png'))) == 3, 'Expected one capture for each library window'
print(next(line for line in text.splitlines() if 'SHARED_SESSIONS_OK' in line))
