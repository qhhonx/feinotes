"""Offline native shared/personal parity regression with synthetic documents.

Build with cloudkit-sync,devtools. The transport fixture is opt-in, requires a
fresh temporary session fixture and never contacts CloudKit. Scripted windows
stay offscreen. No system clipboard or real notes are used.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

repo = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get('FDL_CLOUD_BIN', repo / 'target/debug/quiet_md'))
root = Path(tempfile.mkdtemp(prefix='quiet-sync-qa-sharing-ui-', dir='/private/tmp'))
script = root / 'script.txt'
script.write_text('''wait 1500
resize 1050 760
move 20 700
sharing-ui parity-setup
wait 500
click 120 135
wait 350
sharing-ui parity-pinned-opened
wait 400
shot shared-with-personal-pinned
click 120 135
wait 350
sharing-ui parity-pinned-returned
wait 400
sharing-ui parity-import
wait 300
sharing-ui parity-import-cancel
wait 350
sharing-ui parity-search
wait 350
sharing-ui parity-search-personal
wait 350
sharing-ui parity-search-shared
wait 250
shot shared-rename-field
key enter
wait 250
sharing-ui parity-renamed
wait 350
sharing-ui parity-readonly
wait 400
sharing-ui parity-list
wait 300
shot shared-list-toolbar
move 245 209
wait 100
click 251 209
wait 200
sharing-ui parity-menu
shot shared-row-menu
sharing-ui parity-menu-close
click 978 74
wait 400
sharing-ui parity-grid
shot shared-grid-toolbar
sharing-ui parity-window
wait 350
sharing-ui parity-window-close
wait 200
sharing-ui parity-organization
wait 300
shot shared-batch-removal
sharing-ui parity-cancel-batch
wait 1000
sharing-ui parity-copied
wait 1400
sharing-ui parity-partial
wait 1000
sharing-ui parity-done
quit
''')
bridge = os.environ.get('FDL_CLOUD_DYLD_PATH')
if not bridge:
    candidates = list((repo / 'target/debug/build').glob('quiet_md-*/out/libQuietCloudRestore.dylib'))
    bridge = str(max(candidates, key=lambda p: p.stat().st_mtime).parent)
env = dict(os.environ, TMPDIR='/private/tmp', QUIET_SYNC_QA_ROOT=str(root), QUIET_SYNC_QA_CHANNEL=str(uuid.uuid4()),
           QUIET_SYNC_QA_SHARED_SESSIONS='1', QUIET_SYNC_QA_SHARING_UI='1', FDL_SCRIPT=str(script), FDL_SHOTS=str(root),
           FDL_NOBLINK='1', QUIET_MD_LOG_DIR=str(root / 'logs'), DYLD_LIBRARY_PATH=bridge)
print(f'Artifacts: {root}', flush=True)
with (root / 'run.log').open('w') as log:
    process = subprocess.Popen([str(binary)], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        assert process.wait(timeout=60) == 0, f'See {root / "run.log"}'
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
text = (root / 'run.log').read_text()
assert 'SHARING_PARITY_OK' in text, text[-3000:]
assert len(list(root.glob('*.png'))) == 6, 'Expected six parity UI captures'
print(next(line for line in text.splitlines() if 'SHARING_PARITY_OK' in line))
