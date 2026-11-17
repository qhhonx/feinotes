"""Offline native sharing UI regression with synthetic invitations and faults.

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
sharing-ui sidebar-setup
wait 200
sharing-ui sidebar-shared
wait 200
sharing-ui sidebar-returned
sharing-ui list
wait 500
shot shared-list
sharing-ui fold
wait 100
shot shared-sidebar-expanded
sharing-ui shared-reorder
wait 400
down 120 166
wait 60
dragto 120 150
wait 60
dragto 120 127
wait 60
up 120 127
wait 400
sharing-ui shared-gesture
sharing-ui fold
sharing-ui accept-home
wait 200
shot accept-linked-entry
sharing-ui accept-sidebar-toggle
wait 200
sharing-ui create
wait 200
shot create-empty
type person@example.test
key enter
wait 1200
shot upload-failed
sharing-ui retry-publish
wait 1400
sharing-ui failed-invite
wait 200
shot invite-failed
sharing-ui retry-invite
wait 1200
shot members
sharing-ui narrow
wait 600
shot members-dark-narrow
sharing-ui wide
wait 400
key escape
sharing-ui reopen-members
wait 600
sharing-ui members
wait 600
sharing-ui readonly
wait 600
sharing-ui revoke
wait 200
shot member-removal-confirm
sharing-ui remove-confirm
wait 600
sharing-ui revoked
wait 200
shot collaboration
sharing-ui collaboration
shot shared-document-context
sharing-ui original-review
wait 200
sharing-ui original-cancel
wait 200
sharing-ui rename-shared
wait 80
sharing-ui renamed-shared
sharing-ui return-list
wait 500
sharing-ui accept
wait 200
shot accept-invitation
type https://feinotes.app/share/?g=synthetic-fei-ui
key enter
wait 1200
sharing-ui accepted
wait 200
shot reader-collaboration
sharing-ui leave
wait 200
shot leave-confirmation
sharing-ui confirm-end
wait 900
sharing-ui left
wait 600
sharing-ui delete-document
wait 200
shot delete-confirmation
sharing-ui confirm-end
wait 700
shot delete-failed-recoverable
sharing-ui retry-end
wait 900
sharing-ui done
sharing-ui personal-delete
wait 900
sharing-ui personal-deleted
sharing-ui collections
wait 450
shot shared-private-folders
sharing-ui folder-invite
wait 950
sharing-ui folder-open
wait 450
shot collaborative-folder
click 249 106
wait 250
sharing-ui check-shared-add
shot shared-add-menu
click 309 169
wait 250
sharing-ui check-sidebar-invite
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
assert 'SHARING_UI_OK' in text and 'SHARING_INTERACTIONS_OK' in text, text[-3000:]
assert len(list(root.glob('*.png'))) == 19, 'Expected nineteen UI captures'
assert 'SHARED_FOLDERS_UI_OK' in text
print(next(line for line in text.splitlines() if 'SHARING_UI_OK' in line))
