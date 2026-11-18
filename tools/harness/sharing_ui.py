"""Offline native sharing UI regression with synthetic invitations and faults.

Build with cloudkit-sync,devtools. The transport fixture is opt-in, requires a
fresh temporary session fixture and never contacts CloudKit. Scripted windows
stay offscreen. No system clipboard or real notes are used.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

repo = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
modes = parser.add_mutually_exclusive_group()
modes.add_argument('--folder-scope', action='store_true', help='check existing personal folders, creation scope, and the Mac toolbar themes')
modes.add_argument('--unread', action='store_true', help='capture unread indicators for an original shown in both sidebar sections')
modes.add_argument('--sidebar-scope', action='store_true', help='check both rendered sidebars after clicking into and out of shared sessions')
modes.add_argument('--sidebar-original', action='store_true', help='check opening an owned shared original from My Documents without an intermediate document')
modes.add_argument('--sidebar-entry', action='store_true', help='check a single visible entry selection across shared and personal routes')
modes.add_argument('--sidebar-switch', action='store_true', help='check real A/B clicks during cold SQLite contention and after it clears')
modes.add_argument('--sidebar-waiting', action='store_true', help='switch away before the shared body arrives')
args = parser.parse_args()
binary = Path(os.environ.get('FDL_CLOUD_BIN', repo / 'target/debug/quiet_md'))
root = Path(tempfile.mkdtemp(prefix='quiet-sync-qa-sharing-ui-', dir='/private/tmp'))
script = root / 'script.txt'
sidebar_entry = '''wait 1500
resize 1050 760
move 20 700
sharing-ui sidebar-original-setup
wait 500
click 778 499
wait 250
click 120 272
wait 450
shot entry-personal
sharing-ui sidebar-entry-personal
sharing-ui sidebar-original-opened
click 120 197
wait 350
shot entry-shared
sharing-ui sidebar-entry-shared
sharing-ui sidebar-original-released
click 120 272
wait 350
sharing-ui sidebar-entry-personal
click 50 243
wait 350
sharing-ui sidebar-entry-shared
sharing-ui sidebar-entry-fold-all
wait 350
sharing-ui sidebar-entry-none
click 80 66
wait 350
sharing-ui sidebar-entry-home
sharing-ui sidebar-entry-open-without-row
wait 450
sharing-ui sidebar-entry-shared
quit
'''
sidebar_switch = '''wait 1500
resize 1050 760
move 20 700
sharing-ui unread-expanded
wait 300
click 120 166
wait 350
sharing-ui sidebar-switch-a
sharing-ui sidebar-switch-lock
click 120 135
wait 100
sharing-ui sidebar-switch-responsive
click 120 166
click 120 135
click 120 166
wait 1900
sharing-ui sidebar-switch-a
sharing-ui sidebar-switch-existing-db-lock
click 120 135
wait 100
sharing-ui sidebar-switch-writer
wait 1750
sharing-ui sidebar-switch-b
click 120 166
wait 350
sharing-ui sidebar-switch-a
''' + '''click 120 135
wait 150
sharing-ui sidebar-switch-b
click 120 166
wait 150
sharing-ui sidebar-switch-a
''' * 8 + '''click 80 66
wait 350
sharing-ui sidebar-entry-home
quit
'''
sidebar_waiting = '''wait 1500
resize 1050 760
move 20 700
sharing-ui unread-expanded
wait 300
sharing-ui sidebar-switch-missing
click 120 135
wait 350
shot sidebar-waiting
sharing-ui sidebar-switch-waiting
click 120 135
wait 200
sharing-ui sidebar-switch-waiting
click 120 166
wait 350
sharing-ui sidebar-switch-a
sharing-ui sidebar-switch-deliver
wait 350
sharing-ui sidebar-switch-a
click 120 135
wait 350
sharing-ui sidebar-switch-b
click 80 66
wait 350
sharing-ui sidebar-entry-home
quit
'''
sidebar_original = '''wait 1500
resize 1050 760
move 20 700
sharing-ui sidebar-original-setup
wait 500
click 778 499
wait 250
shot original-before
click 120 272
wait 450
shot original-opened
sharing-ui sidebar-original-opened
down 120 272
wait 300
shot original-held
sharing-ui sidebar-original-held
up 120 272
wait 450
sharing-ui sidebar-original-released
click 120 135
wait 450
sharing-ui sidebar-original-other-room
down 120 272
wait 300
sharing-ui sidebar-original-other-room
up 120 272
wait 450
sharing-ui sidebar-original-opened
click 80 66
wait 450
sharing-ui sidebar-original-deferred
wait 450
sharing-ui sidebar-original-newer-route
down 120 272
dragto 120 445
wait 120
up 120 445
wait 650
sharing-ui sidebar-original-personal-reorder
click 120 135
wait 450
sharing-ui sidebar-original-other-room
click 120 303
wait 450
sharing-ui sidebar-original-same-title
quit
'''
sidebar_scope = '''wait 1500
resize 1050 760
move 20 700
sharing-ui sidebar-scope-setup
wait 500
shot sidebar-before
sharing-ui sidebar-scope-before
click 120 127
wait 500
shot sidebar-shared
sharing-ui sidebar-scope-opened
click 120 166
wait 500
sharing-ui sidebar-scope-switched
click 80 66
wait 500
sharing-ui sidebar-scope-returned
click 251 243
wait 200
click 306 271
wait 200
type New sidebar fixture
wait 850
sharing-ui sidebar-scope-typed
click 80 66
wait 500
shot sidebar-created
sharing-ui sidebar-scope-created
down 120 272
dragto 120 460
wait 100
up 120 460
wait 500
sharing-ui sidebar-scope-personal-reordered
sharing-ui shared-reorder
quit
'''
unread = '''wait 1500
resize 1050 760
move 20 700
sharing-ui unread-collapsed
wait 220
shot unread-collapsed
sharing-ui unread-expanded
wait 520
shot unread-expanded
move 120 197
wait 200
shot unread-expanded-hover
move 20 700
wait 200
shot unread-restored
click 243 197
move 20 700
wait 200
sharing-ui unread-menu-open
shot unread-menu-open
click 20 700
wait 200
sharing-ui unread-menu-closed
shot unread-menu-closed
sharing-ui unread-cleared
wait 200
shot unread-cleared
sharing-ui fold
wait 220
shot unread-cleared-collapsed
quit
'''
folder_scope = '''wait 1500
resize 1050 760
move 20 700
sharing-ui personal-feishu
wait 200
shot feishu-personal-toolbar
sharing-ui personal-notes
wait 200
shot notes-personal-toolbar
sharing-ui personal-feishu
sharing-ui sidebar-setup
wait 500
sharing-ui shared-feishu
wait 200
shot feishu-shared-toolbar
click 900 24
wait 200
sharing-ui check-toolbar-panel
shot feishu-participants-panel
click 900 24
sharing-ui shared-notes
wait 200
shot notes-shared-toolbar
click 953 24
wait 200
sharing-ui check-toolbar-panel
click 953 24
sharing-ui shared-feishu
sharing-ui return-list
wait 500
sharing-ui collections
wait 500
shot shared-private-folders
sharing-ui folder-invite
wait 1200
sharing-ui folder-open
wait 500
shot collaborative-folder
click 249 106
wait 250
sharing-ui check-shared-add
shot shared-add-menu
click 309 137
wait 250
sharing-ui check-sidebar-invite
quit
'''
full = '''wait 1500
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
click 309 137
wait 250
sharing-ui check-sidebar-invite
quit
'''
sidebar_cases = { 'sidebar_entry': sidebar_entry, 'sidebar_switch': sidebar_switch, 'sidebar_waiting': sidebar_waiting }
sidebar_case = next((name for name in sidebar_cases if getattr(args, name)), None)
script.write_text(sidebar_cases[sidebar_case] if sidebar_case else sidebar_original if args.sidebar_original else sidebar_scope if args.sidebar_scope else unread if args.unread else folder_scope if args.folder_scope else full)
bridge = os.environ.get('FDL_CLOUD_DYLD_PATH')
if not bridge:
    candidates = list((repo / 'target/debug/build').glob('quiet_md-*/out/libQuietCloudRestore.dylib'))
    bridge = str(max(candidates, key=lambda p: p.stat().st_mtime).parent)
env = dict(os.environ, TMPDIR='/private/tmp', QUIET_SYNC_QA_ROOT=str(root), QUIET_SYNC_QA_CHANNEL=str(uuid.uuid4()),
           QUIET_SYNC_QA_SHARED_SESSIONS='1', QUIET_SYNC_QA_SHARING_UI='1', FDL_SCRIPT=str(script), FDL_SHOTS=str(root),
           FDL_NOBLINK='1', QUIET_MD_LOG_DIR=str(root / 'logs'), DYLD_LIBRARY_PATH=bridge)
if args.unread or args.sidebar_scope or args.sidebar_original or sidebar_case:
    env['QUIET_SYNC_QA_UNREAD_UI'] = '1'
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
if sidebar_case:
    marker = {'sidebar_entry': 'SIDEBAR_ENTRY sidebar-entry-shared', 'sidebar_switch': 'SIDEBAR_SWITCH sidebar-switch-b', 'sidebar_waiting': 'SIDEBAR_SWITCH sidebar-switch-waiting'}[sidebar_case]
    assert marker in text and 'PANIC' not in text, text[-4000:]
    print(f'{sidebar_case.upper()}_OK')
    raise SystemExit
if args.sidebar_original:
    assert 'SIDEBAR_ORIGINAL_OK' in text, text[-4000:]
    print('SIDEBAR_ORIGINAL_OK')
    raise SystemExit
if args.sidebar_scope:
    assert 'SIDEBAR_PERSONAL_REORDER_OK' in text and (root / 'qa-reordered-sidebar').is_file(), text[-4000:]
    print('SIDEBAR_SCOPE_OK')
    raise SystemExit
if args.unread:
    assert len(list(root.glob('*.png'))) == 8, 'Expected eight sidebar captures'
    print('SHARED_UNREAD_UI_CAPTURED')
    raise SystemExit
if args.folder_scope:
    assert len(list(root.glob('*.png'))) == 8, 'Expected eight UI captures'
else:
    assert 'SHARING_UI_OK' in text and 'SHARING_INTERACTIONS_OK' in text, text[-3000:]
    assert len(list(root.glob('*.png'))) == 19, 'Expected nineteen UI captures'
assert 'SHARED_FOLDERS_UI_OK' in text
assert 'SHARED_PERSONAL_FOLDER_REMOVAL_OK' in text
marker = 'SHARED_PERSONAL_FOLDER_REMOVAL_OK' if args.folder_scope else 'SHARING_UI_OK'
print(next(line for line in text.splitlines() if marker in line))
