"""Run a synthetic offscreen Mac UI replay. Use only a QA devtools build."""
import argparse,json,os,sqlite3,subprocess,tempfile,uuid
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('mode',choices=['local','cloud']);p.add_argument('--expect-fixed',action='store_true');a=p.parse_args()
root=Path(tempfile.mkdtemp(prefix='quiet-sync-qa-edit-',dir='/private/tmp')); print(root,flush=True)
(root/'pause-cloud').touch()
env=dict(os.environ, QUIET_SYNC_QA_ROOT=str(root),QUIET_SYNC_QA_CHANNEL=str(uuid.uuid4()),FDL_SHOTS=str(root),FDL_NOBLINK='1',QUIET_MD_LOG_DIR=str(root/'logs'))
if bridge := os.environ.get('FDL_CLOUD_DYLD_PATH'): env['DYLD_LIBRARY_PATH']=bridge
if a.mode=='cloud': env['QUIET_SYNC_QA_SHARED_SESSIONS']='1'
else: (root/'use-local-library').touch();env['QUIET_SYNC_QA_LOCAL_FIRST']='1'
script=root/'steps'; env['FDL_SCRIPT']=str(script)
script.write_text("""resize 1200 850
wait 800
key cmd-n
wait 250
type Alpha
key enter
type ab中文ef
wait 850
key left
key left
qa-note-state before-split
key enter
wait 850
qa-note-state split
key cmd-z
wait 800
qa-note-state undo
key cmd-shift-z
wait 800
qa-note-state redo
key cmd-z
wait 800
key cmd-right
key shift-left
key shift-left
qa-note-state pre-replace
type Z
wait 850
qa-note-state replaced
key cmd-z
wait 850
qa-note-state undo-replace
key cmd-right
key shift-left
key shift-left
qa-note-state leaving
key cmd-n
wait 300
type Beta
wait 800
qa-note-open Alpha
wait 300
qa-note-state reopened
key cmd-n
wait 300
qa-note-state untouched
qa-note-home
wait 400
key cmd-n
wait 250
type X
key backspace
wait 900
qa-note-state edited-empty
qa-note-open Beta
wait 400
quit
""")
if a.expect_fixed:
    steps=script.read_text().replace('wait 800\nkey cmd-n', 'wait 800\nqa-note-guards\nkey cmd-n',1)
    steps=steps.replace('qa-note-state before-split', 'qa-note-peer\nqa-note-state before-split')
    steps=steps.replace('qa-note-state undo\n', 'qa-note-state undo\nqa-note-check-peer\n',1)
    script.write_text(steps)
with (root/'log').open('w') as log:
    r=subprocess.run([str(a.binary.resolve())],cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=80)
r.check_returncode()
states=[json.loads(l.partition('QA_STATE ')[2]) for l in (root/'log').read_text().splitlines() if l.startswith('QA_STATE ')]
s={v['label']:v for v in states};db=next(root.glob('*/library.sqlite3')) if a.mode=='local' else root/('a'*64)/'library.sqlite3'
with sqlite3.connect(f'file:{db}?mode=ro',uri=True) as conn:
    rows=conn.execute('SELECT id,title,trashed FROM library_documents').fetchall()
def sel(k):return [s[k]['anchor'],s[k]['head']]
checks={'undo_body':s['undo']['body']==s['before-split']['body'],'redo_body':s['redo']['body']==s['split']['body'],
        'undo_replacement_range':sel('pre-replace')==sel('undo-replace') and s['pre-replace']['body']==s['undo-replace']['body'],
        'undo_caret':sel('before-split')==sel('undo'),'redo_caret':sel('split')==sel('redo'),
        'reverse_range':sel('leaving')==sel('reopened') and s['leaving']['anchor']!=s['leaving']['head'],
        'pristine_absent':sum(t=='' for _,t,_ in rows)==(2 if a.expect_fixed else 1) and all(i!=s['untouched']['id'] for i,_,_ in rows),
        'edited_empty_kept':any(i==s['edited-empty']['id'] and t=='' and not trash for i,t,trash in rows)}
if a.expect_fixed:
    lines=(root/'log').read_text().splitlines()
    _,blank,guard=next(l for l in lines if l.startswith('QA_GUARD ')).split()
    with sqlite3.connect(f'file:{db}?mode=ro',uri=True) as conn:
        links=conn.execute('SELECT links FROM library_documents WHERE id=?',(guard,)).fetchone()[0]
        payload=str(conn.execute('SELECT payload FROM library_blocks WHERE document=?',(guard,)).fetchall())
        checks['old_empty_target_and_attachment']=(blank,'',0) in rows and blank in links and 'qa-keep.txt' in payload and (db.parent/'assets/files/qa-keep.txt').read_bytes()==b'QA attachment'
        checks['clean_trash']=conn.execute('SELECT count(*) FROM library_trash').fetchone()[0]==0
    checks['peer_selection']='QA_PEER_OK' in lines
    if a.mode=='local':
        script.write_text('wait 800\nqa-note-open Alpha\nwait 300\nqa-note-state restarted\nquit\n')
        with (root/'restart.log').open('w') as log:
            subprocess.run([str(a.binary.resolve())],cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=30,check=True)
        reopened=next(json.loads(l.partition('QA_STATE ')[2]) for l in (root/'restart.log').read_text().splitlines() if l.startswith('QA_STATE '))
        checks['restart_range']=[reopened['anchor'],reopened['head']]==sel('leaving')
print(json.dumps(dict(root=str(root),mode=a.mode,states=states,rows=rows,checks=checks),ensure_ascii=False,indent=2))
assert checks['undo_body'] and checks['redo_body'] and checks['edited_empty_kept']
if a.expect_fixed: assert all(checks.values()),checks
