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
modes.add_argument('--sync-badge', action='store_true', help='verify shared sync badge states, themes and editor geometry')
modes.add_argument('--folder-sidebar', action='store_true', help='exercise nested collaborative folders in the shared sidebar')
modes.add_argument('--sidebar-create', action='store_true', help='verify sidebar creation and preservation of the active editor')
modes.add_argument('--sidebar-consistency', action='store_true', help='check matched folder controls and deletion in both Mac sidebar sections')
modes.add_argument('--sidebar-origin', action='store_true', help='check sidebar expansion, Home origin, and failed invitation retry')
args = parser.parse_args()
binary = Path(os.environ.get('FDL_CLOUD_BIN', repo / 'target/debug/quiet_md'))
root = Path(tempfile.mkdtemp(prefix='quiet-sync-qa-sharing-ui-', dir='/private/tmp'))
script = root / 'script.txt'
sidebar_entry = '''wait 1500
resize 1050 760
move 20 700
sharing-ui sidebar-original-setup
wait 500
sharing-ui sidebar-entry-click-personal
wait 450
shot entry-personal
sharing-ui sidebar-entry-personal
sharing-ui sidebar-original-opened
sharing-ui sidebar-entry-click-shared
wait 350
shot entry-shared
sharing-ui sidebar-entry-shared
sharing-ui sidebar-original-released
sharing-ui sidebar-entry-click-personal
wait 350
sharing-ui sidebar-entry-personal
sharing-ui sidebar-entry-fold-personal
wait 350
sharing-ui sidebar-entry-none
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
sharing-ui sidebar-switch-setup
wait 450
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
sidebar_waiting = """wait 1500
sharing-ui sidebar-switch-setup
wait 450
resize 1050 760
move 20 700
sharing-ui unread-expanded
wait 300
click 120 166
wait 350
sharing-ui sidebar-switch-a
sharing-ui sidebar-switch-missing
click 120 135
wait 650
sharing-ui sidebar-switch-waiting
shot sidebar-waiting
sharing-ui sidebar-switch-offline
wait 1350
sharing-ui sidebar-switch-offline-feedback
shot sidebar-offline
sharing-ui sidebar-switch-reconnect
click 120 135
wait 650
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
shot sidebar-reopened
click 120 166
wait 350
sharing-ui sidebar-switch-a
sharing-ui sidebar-switch-missing
click 120 135
wait 650
sharing-ui sidebar-switch-waiting
sharing-ui sidebar-switch-deliver
wait 550
sharing-ui sidebar-switch-b
shot sidebar-opened-after-delivery
click 80 66
wait 350
sharing-ui sidebar-entry-home
quit
"""
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
sharing-ui original-organization
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
sharing-ui check-offline-toolbar
sharing-ui show-toolbar-panel
wait 200
sharing-ui check-toolbar-panel
shot feishu-participants-panel
sharing-ui hide-toolbar-panel
sharing-ui shared-notes
wait 200
shot notes-shared-toolbar
sharing-ui show-toolbar-panel
wait 200
sharing-ui check-toolbar-panel
sharing-ui hide-toolbar-panel
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
click 201 106
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
sharing-ui members-compact
sharing-ui narrow
wait 600
shot members-dark-narrow
sharing-ui members-overflow
wait 250
shot members-many
sharing-ui members-scroll-bottom
wait 250
shot members-many-scrolled
sharing-ui members-restore
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
type https://feinotes.app/share/?v=2&u=aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS9zeW50aGV0aWMtZmVpLXVp
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
click 201 106
wait 250
sharing-ui check-sidebar-invite
quit
'''
sync_badge = """wait 1500
resize 1050 760
move 20 700
sharing-ui sidebar-original-setup
wait 350
sharing-ui sync-badge-open
wait 700
"""
for theme in ('light', 'dark', 'notes-light', 'notes-dark'):
    sync_badge += f"""sharing-ui sync-badge-{theme}
wait 240
sharing-ui sync-badge-ready
wait 160
sharing-ui sync-badge-position
shot sync-{theme}-ready
sharing-ui sync-badge-uploading
wait 160
sharing-ui sync-badge-check-position
shot sync-{theme}-uploading
sharing-ui sync-badge-stalled
wait 160
sharing-ui sync-badge-check-position
shot sync-{theme}-stalled
sharing-ui sync-badge-ready
wait 160
sharing-ui sync-badge-check-position
shot sync-{theme}-recovered
"""
sync_badge += 'quit\n'
folder_sidebar = '''wait 1500
resize 1050 760
move 20 700
sharing-ui folder-sidebar-setup
wait 1800
sharing-ui folder-sidebar-ready
wait 4200
sharing-ui folder-sidebar-click-owner
wait 850
move 20 700
shot folder-sidebar-expanded
sharing-ui folder-sidebar-expanded
sharing-ui folder-sidebar-click-nested
wait 850
sharing-ui folder-sidebar-click-document
wait 850
move 20 700
shot folder-sidebar-document
sharing-ui folder-sidebar-opened
sharing-ui shared-notes
wait 300
shot folder-sidebar-document-notes
sharing-ui shared-feishu
wait 200
sharing-ui folder-sidebar-click-owner-chevron
wait 500
sharing-ui folder-sidebar-collapsed
sharing-ui folder-sidebar-click-owner-chevron
wait 500
sharing-ui folder-sidebar-click-personal-document
wait 850
sharing-ui folder-sidebar-personal-opened
sharing-ui folder-sidebar-create
wait 450
sharing-ui folder-sidebar-created
move 20 700
shot folder-sidebar-created
sharing-ui folder-sidebar-click-received
wait 850
sharing-ui folder-sidebar-received-open
sharing-ui folder-sidebar-click-empty
wait 800
move 20 700
shot folder-sidebar-empty
sharing-ui folder-sidebar-empty
sharing-ui folder-sidebar-click-readonly
wait 850
sharing-ui folder-sidebar-readonly
move 20 700
shot folder-sidebar-readonly
sharing-ui folder-sidebar-dark
wait 300
shot folder-sidebar-readonly-dark
quit
'''
sidebar_create = '''wait 1500
resize 1200 840
sharing-ui folder-sidebar-setup
wait 1600
sharing-ui folder-sidebar-ready
wait 700
sharing-ui folder-sidebar-create-readonly-absent
sharing-ui folder-sidebar-create-click-personal-shortcut
wait 200
row 文件夹
wait 700
sharing-ui folder-sidebar-create-personal-made
shot shared-subfolder-from-my-documents
sharing-ui folder-sidebar-click-personal-document
wait 650
sharing-ui folder-sidebar-create-capture
sharing-ui folder-sidebar-create-click-personal-header
wait 400
sharing-ui folder-sidebar-create-menu
shot personal-create-menu-over-shared-document
sharing-ui folder-sidebar-create-click-personal-header
wait 350
sharing-ui folder-sidebar-create-dismissed
sharing-ui folder-sidebar-create-click-shared-shortcut
wait 200
row 文件夹
wait 650
sharing-ui folder-sidebar-create-shared-made
shot shared-subfolder-from-shared-documents
sharing-ui folder-sidebar-create-click-nested-shortcut
wait 200
row 文件夹
wait 650
sharing-ui folder-sidebar-create-nested-made
sharing-ui folder-sidebar-click-personal-document
wait 650
sharing-ui folder-sidebar-create-click-personal-header
wait 300
row 文件夹
wait 550
sharing-ui folder-sidebar-create-personal-root-made
sharing-ui folder-sidebar-click-received
wait 650
sharing-ui folder-sidebar-received-open
sharing-ui folder-sidebar-create-readonly-absent
quit
'''
sidebar_consistency = '''wait 1500
resize 1200 840
sharing-ui folder-sidebar-setup
wait 1700
sharing-ui folder-sidebar-ready
wait 600
sharing-ui folder-sidebar-create-click-personal-shortcut
wait 200
row 文件夹
wait 550
sharing-ui folder-sidebar-consistency-fixture
sharing-ui folder-sidebar-consistency-expand
wait 180
sharing-ui folder-sidebar-consistency-presentation
sharing-ui folder-sidebar-click-personal-document
wait 450
sharing-ui folder-sidebar-create-capture
sharing-ui folder-sidebar-consistency-click-shared-create
wait 190
sharing-ui folder-sidebar-consistency-menu-stable
shot shared-folder-sidebar-create-menu
sharing-ui folder-sidebar-consistency-click-personal-more
wait 250
sharing-ui folder-sidebar-consistency-menu-stable
shot same-folder-my-documents-menu
sharing-ui folder-sidebar-consistency-click-delete
wait 250
sharing-ui folder-sidebar-consistency-click-confirm
wait 500
sharing-ui folder-sidebar-consistency-deleted
sharing-ui folder-sidebar-consistency-restore
wait 500
sharing-ui folder-sidebar-consistency-presentation
sharing-ui folder-sidebar-consistency-click-shared-more
wait 250
sharing-ui folder-sidebar-consistency-menu-stable
shot same-folder-shared-documents-menu
sharing-ui folder-sidebar-consistency-click-delete
wait 200
sharing-ui folder-sidebar-consistency-click-confirm
wait 500
sharing-ui folder-sidebar-consistency-deleted
shot folder-deleted-in-both-sidebars
sharing-ui folder-sidebar-consistency-click-root-more
wait 250
sharing-ui folder-sidebar-consistency-click-delete
wait 200
sharing-ui folder-sidebar-consistency-click-confirm
wait 1200
sharing-ui folder-sidebar-consistency-root-deleted
quit
'''
sidebar_origin = '''wait 1500
resize 1200 840
sharing-ui folder-sidebar-setup
wait 1700
sharing-ui folder-sidebar-ready
wait 600
sharing-ui folder-sidebar-origin-personal-home-folder
wait 250
sharing-ui folder-sidebar-origin-click-home-folder
wait 550
sharing-ui folder-sidebar-origin-check-personal-folder
shot home-my-documents-folder-selected
sharing-ui folder-sidebar-origin-click-breadcrumb
wait 550
sharing-ui folder-sidebar-origin-return-personal
shot breadcrumb-return-my-documents
sharing-ui folder-sidebar-origin-click-home-folder
wait 550
sharing-ui folder-sidebar-origin-fold-personal
wait 550
sharing-ui folder-sidebar-origin-no-jump
sharing-ui folder-sidebar-overview
wait 550
sharing-ui folder-sidebar-origin-shared-home-folder
wait 250
sharing-ui folder-sidebar-origin-click-home-folder
wait 550
sharing-ui folder-sidebar-origin-check-shared-folder
shot home-shared-documents-folder-selected
sharing-ui folder-sidebar-origin-click-breadcrumb
wait 550
sharing-ui folder-sidebar-origin-return-shared
shot breadcrumb-return-shared-documents
sharing-ui folder-sidebar-origin-click-home-folder
wait 550
sharing-ui folder-sidebar-origin-form-keeps-editor
sharing-ui folder-sidebar-origin-make-personal-folder
wait 550
sharing-ui folder-sidebar-origin-check-creation
shot new-folder-from-personal
sharing-ui folder-sidebar-origin-make-shared-folder
wait 550
sharing-ui folder-sidebar-origin-check-creation
shot new-folder-from-shared
sharing-ui folder-sidebar-origin-make-personal-document
wait 550
sharing-ui folder-sidebar-origin-check-creation
sharing-ui folder-sidebar-origin-make-shared-document
wait 550
sharing-ui folder-sidebar-origin-check-creation
sharing-ui folder-sidebar-overview
wait 550
sharing-ui folder-sidebar-origin-new-document
wait 660
sharing-ui folder-sidebar-origin-invite-pending
shot creating-shared-document
wait 650
sharing-ui folder-sidebar-origin-invite-failed
shot failed-invitation-no-library-entry
sharing-ui folder-sidebar-origin-invite-retry
wait 550
sharing-ui folder-sidebar-origin-invite-success
shot invitation-retried-single-item
sharing-ui folder-sidebar-overview
wait 550
sharing-ui folder-sidebar-origin-personal-home-document
wait 300
sharing-ui folder-sidebar-origin-click-home-document
wait 550
sharing-ui folder-sidebar-origin-check-personal-document
shot home-my-documents-document-selected
sharing-ui folder-sidebar-overview
wait 550
sharing-ui folder-sidebar-origin-new-folder
wait 660
sharing-ui folder-sidebar-origin-invite-pending
shot creating-shared-folder
wait 650
sharing-ui folder-sidebar-origin-invite-failed
sharing-ui folder-sidebar-origin-invite-cancel
sharing-ui folder-sidebar-origin-new-folder
wait 660
sharing-ui folder-sidebar-origin-invite-pending
shot creating-shared-folder
wait 650
sharing-ui folder-sidebar-origin-invite-failed
sharing-ui folder-sidebar-origin-invite-retry
wait 550
sharing-ui folder-sidebar-origin-invite-success
quit
'''
sidebar_cases = { 'sidebar_origin': sidebar_origin, 'sidebar_consistency': sidebar_consistency, 'sidebar_create': sidebar_create, 'sidebar_entry': sidebar_entry, 'sidebar_switch': sidebar_switch, 'sidebar_waiting': sidebar_waiting }
sidebar_case = next((name for name in sidebar_cases if getattr(args, name)), None)
script.write_text(folder_sidebar if args.folder_sidebar else sync_badge if args.sync_badge else sidebar_cases[sidebar_case] if sidebar_case else sidebar_original if args.sidebar_original else sidebar_scope if args.sidebar_scope else unread if args.unread else folder_scope if args.folder_scope else full)
bridge = os.environ.get('FDL_CLOUD_DYLD_PATH')
if not bridge:
    candidates = list((repo / 'target/debug/build').glob('quiet_md-*/out/libQuietCloudRestore.dylib'))
    bridge = str(max(candidates, key=lambda p: p.stat().st_mtime).parent)
env = dict(os.environ, TMPDIR='/private/tmp', QUIET_SYNC_QA_ROOT=str(root), QUIET_SYNC_QA_CHANNEL=str(uuid.uuid4()),
           QUIET_SYNC_QA_SHARED_SESSIONS='1', QUIET_SYNC_QA_SHARING_UI='1', FDL_SCRIPT=str(script), FDL_SHOTS=str(root),
           FDL_NOBLINK='1', QUIET_MD_LOG_DIR=str(root / 'logs'), DYLD_LIBRARY_PATH=bridge)
if args.unread or args.sidebar_scope or args.sidebar_original or (sidebar_case and not (args.sidebar_create or args.sidebar_consistency or args.sidebar_origin)) or args.sync_badge:
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
if args.sync_badge:
    assert text.count('SYNC_BADGE_POSITION_OK sync-badge-check-position') == 12, text[-4000:]
    assert len(list(root.glob('sync-*.png'))) == 16
    print('SHARED_SYNC_BADGE_GEOMETRY_OK')
    raise SystemExit
if args.sidebar_origin:
    assert text.count('SIDEBAR_HOME_ORIGIN_OK') == 3 and 'SIDEBAR_COLLAPSE_SELECTION_OK' in text, text[-4000:]
    assert 'SIDEBAR_NEW_FORM_KEEPS_EDITOR' in text, text[-4000:]
    assert text.count('SIDEBAR_CREATE_ORIGIN_OK') == 4 and text.count('SIDEBAR_INVITE_FAILURE_HIDDEN') == 3, text[-4000:]
    assert text.count('SIDEBAR_INVITE_RETRY_ONE_ITEM') == 2 and 'SIDEBAR_INVITE_CANCEL_NO_ITEM' in text, text[-4000:]
    assert text.count('FOLDER_BREADCRUMB_RETURN_OK') == 2, text[-4000:]
    assert text.count('SIDEBAR_INVITE_PROGRESS_IN_PLACE') == 3 and len(list(root.glob('*.png'))) == 11 and 'PANIC' not in text, text[-4000:]
    print('SIDEBAR_ORIGIN_OK')
    raise SystemExit
if args.sidebar_consistency:
    assert text.count('SIDEBAR_FOLDER_CONTROLS_ALIGNED') == 2 and text.count('SIDEBAR_FOLDER_MENU_STABLE') == 3, text[-4000:]
    assert text.count('SIDEBAR_FOLDER_DELETE_BOTH') == 2 and 'SIDEBAR_FOLDER_ROOT_DELETED' in text, text[-4000:]
    assert len(list(root.glob('*.png'))) == 4 and "PANIC" not in text, text[-4000:]
    print('SIDEBAR_FOLDER_CONSISTENCY_OK')
    raise SystemExit
if args.sidebar_create:
    assert 'SIDEBAR_CREATE_MENU_STABLE' in text and 'SIDEBAR_CREATE_PERSONAL_ROOT' in text, text[-4000:]
    assert text.count('SIDEBAR_SHARED_CHILD_CREATED') == 3, text[-4000:]
    assert text.count('SIDEBAR_CREATE_READONLY_PROTECTED') == 2 and len(list(root.glob('*.png'))) == 3, text[-4000:]
    print('SIDEBAR_CREATE_OK')
    raise SystemExit
if sidebar_case:
    marker = {'sidebar_entry': 'SIDEBAR_ENTRY sidebar-entry-shared', 'sidebar_switch': 'SIDEBAR_SWITCH sidebar-switch-b', 'sidebar_waiting': 'SIDEBAR_SWITCH sidebar-switch-waiting'}[sidebar_case]
    assert marker in text and 'PANIC' not in text, text[-4000:]
    print(f'{sidebar_case.upper()}_OK')
    raise SystemExit
if args.folder_sidebar:
    assert 'SHARED_FOLDER_SIDEBAR_OK' in text and len(list(root.glob('*.png'))) == 7, text[-4000:]
    print('SHARED_FOLDER_SIDEBAR_OK')
    raise SystemExit
if args.sidebar_original:
    assert 'SIDEBAR_ORIGINAL_OK' in text and 'SHARED_ORIGINAL_ORGANIZATION_OK' in text, text[-4000:]
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
    assert len(list(root.glob('*.png'))) == 21, 'Expected twenty-one UI captures'
assert 'SHARED_FOLDERS_UI_OK' in text
assert 'SHARED_PERSONAL_FOLDER_REMOVAL_OK' in text
marker = 'SHARED_PERSONAL_FOLDER_REMOVAL_OK' if args.folder_scope else 'SHARING_UI_OK'
print(next(line for line in text.splitlines() if marker in line))
