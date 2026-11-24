import * as wasm from './wasm/feinotes.js';
import {loadWorkspaceEngine} from './workspace-engine.js';
import {openDatabase,readLibrary,writeLibrary,saveRestoredLibrary,listRestoredLibraries,bytesB64,b64Bytes} from './storage.js';
import {loadCloudKit,createPrivateTransport,isExpiredSession,isProductionLibrary,parseInvitation} from './cloudkit.js';
import {AppleSignIn} from './cloud-auth.js';
import {createCloudSessionVault,sessionScope} from './cloud-session.js';
import {decodeObjectId, encodeObjectLink, personalScope, scopeMatches, sharedScope} from './object-link.js';
import {BackupValidationError,inspectMacBackup} from './backup-check.js';
import {createAttachmentPreview,download} from './attachment-preview.js';
import {createPresencePublisher,createPresenceReader} from './shared-presence.js';
const $=id=>document.getElementById(id);
for(const dialog of document.querySelectorAll('dialog')){
  dialog.addEventListener('keydown',event=>{
    if(event.key!=='Tab'||event.defaultPrevented||!dialog.matches(':modal'))return;
    const stops=[...dialog.querySelectorAll('a[href],button,input,select,textarea,[tabindex],iframe,audio[controls],video[controls],embed')]
      .filter(element=>!element.matches(':disabled')&&element.tabIndex>=0&&element.getClientRects().length&&getComputedStyle(element).visibility==='visible');
    if(!stops.length){event.preventDefault();return;}
    const active=document.activeElement;
    const boundary=event.shiftKey?stops[0]:stops.at(-1);
    if(active!==boundary&&stops.includes(active))return;
    event.preventDefault();
    (event.shiftKey?stops.at(-1):stops[0]).focus();
  });
}
const englishCloudCopy=new Map([
  ['正在打开文档库…','Opening the library…'],
  ['正在连接 iCloud…','Connecting to iCloud…'],
  ['正在下载文档…','Downloading document…'],
  ['无法打开文档库，请刷新后重试。','Could not open the library. Refresh to try again.'],
  ['此浏览器','This browser'],
  ['iCloud 离线缓存','iCloud offline cache'],
  ['iCloud 文档库 · 只读预览','iCloud Library · Read-only preview'],
  ['iCloud 文档库','iCloud Library'],
  ['iCloud · 只读预览','iCloud · Read-only preview'],
  ['正在打开文档库…','Opening the library…'],
  ['正在连接 iCloud…','Connecting to iCloud…'],
  ['使用 Apple 账号登录','Sign in with Apple'],
  ['立即同步','Sync now'],
  ['退出 iCloud','Sign out of iCloud'],
  ['返回','Back'],
  ['连接 Apple 账号，查看 iCloud 文档。','Connect your Apple Account to view iCloud documents.'],
  ['使用 Apple 账号读取和同步文档。','Use your Apple Account to read and sync documents.'],
  ['正在准备 Apple 授权…','Preparing Apple sign-in…'],
  ['重试 Apple 授权','Retry Apple sign-in'],
  ['暂时无法打开 Apple 登录。','Apple sign-in is temporarily unavailable.'],
  ['浏览器未打开 Apple 登录窗口。','Your browser did not open the Apple sign-in window.'],
  ['请在 Apple 窗口完成登录。','Finish signing in in the Apple window.'],
  ['Apple 窗口已关闭，尚未收到授权。','The Apple window closed before authorization was received.'],
  ['Apple 登录等待超时，请重试。','Apple sign-in timed out. Try again.'],
  ['Apple 未返回有效授权。','Apple did not return a valid authorization.'],
  ['正在读取 iCloud 私有文档库…','Reading your private iCloud library…'],
  ['无法读取 iCloud 私有文档库，请重试。','Could not read your private iCloud library. Try again.'],
  ['重新登录 Apple','Sign in with Apple again'],
  ['当前离线，请联网后重新连接 iCloud。','You’re offline. Reconnect to the internet to use iCloud.'],
  ['已连接 iCloud，可查看文档。','Connected to iCloud. You can view your documents.'],
  ['已登录 Apple 账号。','Signed in to your Apple Account.'],
  ['尚未登录 Apple 账号。','Signed out of your Apple Account.'],
  ['iCloud 账号已变化，请重新登录。','Your iCloud account changed. Sign in again.'],
  ['无法恢复 iCloud 会话，请重试。','Could not restore your iCloud session. Try again.'],
  ['暂时无法恢复 iCloud 会话。','Could not restore your iCloud session.'],
  ['请登录 Apple 账号恢复 iCloud 同步。','Sign in with Apple to reconnect to iCloud.'],
  ['Apple 会话已失效，请重新登录。','Your Apple session expired. Sign in again.'],
  ['Apple 授权未能读取此 iCloud 文档库，请重新登录。','Could not verify access to this iCloud library. Sign in again.'],
  ['找不到飞记的 iCloud 文档库。请先在 Mac 打开飞记并同步。','Could not find your Fei Notes iCloud library. Open Fei Notes on your Mac and sync first.'],
  ['此浏览器的 iCloud 会话已结束，请重新连接。','This browser’s iCloud session ended. Reconnect to continue.'],
  ['已退出 iCloud。','Signed out of iCloud.'],
  ['暂时无法连接 iCloud，请稍后重试。','Unable to connect to iCloud. Try again later.'],
  ['操作未完成，请重试。','Couldn’t complete the action. Try again.'],
  ['本地保存失败 · 可导出恢复副本','Couldn’t save locally · Export a recovery copy'],
  ['正在保存…','Saving…'],
  ['已保存到此浏览器','Saved in this browser'],
  ['有文档需要处理','Some documents need attention'],
  ['离线 · 已保存到此浏览器','Offline · Saved in this browser'],
  ['同步中…','Syncing…'],
]);
const cloudText=text=>document.documentElement.lang==='en'?(englishCloudCopy.get(text)??text):text;
const cloudLabels={
  'apple-sign-in-button':'使用 Apple 账号登录',sync:'立即同步',
  signout:'退出 iCloud','cloud-back':'返回',
};
let saveStatusText='正在打开文档库…',saveStatusPhase='saving',connectionText='';
let ready=false,db,config,transport,authorization,setupPromise,account=null,epoch=0,serial=Promise.resolve(),timer,syncing=false,dirty=false;
let key='local',writer=false,releaseLock,authGeneration=0;
let saveRevision=0;
let networkRevision=0,syncRequested=false;
let lastSuccessfulSync='';
let cloudSyncPhase='';
let sharedWanted=false,sharedTask,sharedAssetRead,sharedRevision=0,checkedAt=0;
const catalogueScope='00000000-0000-0000-0000-000000000001';
const assetScope='00000000-0000-0000-0000-000000000002';
let bodyRequest=0;
let trialRequest=0;
let openingBody;
let sharedReturn=null,reloadDestination=null;
let shownOwnedScopes='[]';
let shownGuestScopes='[]';
const parameters = new URLSearchParams(location.search);
const cloudReadOnly=document.documentElement.dataset.cloudAccess==='read-only';
const lastLibraryKey=cloudReadOnly?'last-library:cloud-read-only':'last-library';
const canEdit=()=>writer&&!(cloudReadOnly&&account);
const localPreview=['localhost','127.0.0.1','[::1]'].includes(location.hostname);
const demo=localPreview&&parameters.get('demo')==='1';
const presencePreview=localPreview&&parameters.get('presence-preview')==='1';
const publishPresence=createPresencePublisher();
let presenceSession;
const ownedDemo=demo&&parameters.get('owned-preview')==='1';
const sharedEditPreview=ownedDemo&&parameters.get('shared-edit-preview')==='1';
const sharedConflictPreview=sharedEditPreview&&parameters.get('share-conflict-preview')==='1';
const restoredId=parameters.get('local-library');
const restoredWorkspace=restoredId!==null;
const restoredPrefix='restored:'+(sharedConflictPreview?'demo-share-conflict':sharedEditPreview?'demo-edit':ownedDemo?'demo-owned':demo?'demo':'live')+':';
const libraryIdPattern=/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/;
let demoShares;
const invitationPreview=!localPreview||parameters.get('invite-preview')!=='0';
const canAcceptInvitation=demo||(!cloudReadOnly&&localPreview&&parameters.get('invite-preview')==='1');
const openInvitationInApp='请先在飞记 App 接受邀请，加入后可回到网页版打开共享。';
const shareFragment=/^#[cg]=[A-Za-z0-9_-]{1,256}$/;
let incomingInvitation=!demo&&!restoredWorkspace&&invitationPreview&&shareFragment.test(location.hash)
  ?'https://feinotes.app/share/?'+location.hash.slice(1):null;
let inspectedInvitation,invitationBusy=false,invitationRevision=0,checkInvitationAfterSignIn=false;
const objectFragment=/^#object=([A-Za-z0-9_-]{44})$/.exec(location.hash);
let pendingObject=!demo&&!restoredWorkspace&&objectFragment?decodeObjectId(objectFragment[1]):null;
let objectRequest=0;
let objectDirectoryPending=null;
let importRequest,importing=false;
let boardPictureRequest;
let boardClipboardMessage = '';
let attachmentRequest;
let attachmentRead;
const attachmentPreview=createAttachmentPreview($('attachment-preview'));
let closePrintFrame;
let membersRequest, membersVersion=0, backupVersion=0;
let leaveShareRequest;
let verifiedBackup;
let libraryBackup;
const demoInvitation='https://feinotes.app/share/?c=0feinotes-demo';
const markdownExt=/\.(md|mdown|markdown|mark|txt)$/i;
const xmlExt=/\.xml$/i;
const libraryScope = new URL('.',location.href).pathname + (sharedConflictPreview?'share-conflict-demo:':sharedEditPreview?'shared-edit-demo:':ownedDemo?'owned-demo:':demo?'demo:':'');
const preferencesKey = 'feinotes:appearance:' + libraryScope + (restoredWorkspace?'restored:'+restoredId:'');
const sharedReloadKey = 'feinotes:shared-reload:' + libraryScope;
const cloudSessionKey='feinotes:cloud-session:'+libraryScope;
const sessionVault=cloudReadOnly&&!demo&&!restoredWorkspace?
  createCloudSessionVault(window.localStorage,navigator.locks,cloudSessionKey):null;
let cloudSessionId;
const sharedRoutePattern=/^web-shared:[a-f0-9]{64}:[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/;
if (demo) key=sharedConflictPreview?'local-demo-share-conflict':sharedEditPreview?'local-demo-shared-edit':ownedDemo?'local-demo-owned':'local-demo';

function clearPresence(){
  if(!presenceSession)return;
  presenceSession=undefined;
  void publishPresence(null,null);
  wasm.set_shared_presence('','[]');
}
function demoPresence({note,route},available){
  const started=Date.now();
  const people=['林夏','周宁','陈安'];
  const participants=people.map((givenName,index)=>({type:'USER',permission:'READ_WRITE',acceptanceStatus:'ACCEPTED',
    userIdentity:{userRecordName:'_demoEditor'+index,nameComponents:{givenName}}}));
  const state=bytesB64(new TextEncoder().encode(JSON.stringify({note,anchor:null,head:null})));
  const selection=wasm.demo_shared_selection(route);
  return createPresenceReader({account:'_demoViewer',note,available,readShare:async()=>({participants}),
    readPage:async syncToken=>({syncToken:'demo',records:syncToken?[]:participants.map((member,index)=>{
      const stamp={userRecordName:member.userIdentity.userRecordName,timestamp:started};
      return {recordName:String(index+1),recordType:'QuietPresence',created:stamp,modified:stamp,
        fields:{state:{type:'BYTES',value:index===0&&selection?selection:state}}};
    })})});
}
function pollPresence(){
  if(!ready||restoredWorkspace||document.hidden||!navigator.onLine||!demo&&!transport?.presenceReader){clearPresence();return;}
  const raw=wasm.active_shared_presence();
  if(!raw){clearPresence();return;}
  if(presenceSession?.raw!==raw||presenceSession.cloud!==transport){
    clearPresence();
    const context=JSON.parse(raw);
    const session={raw,cloud:transport,route:context.route, retryAfter:0};
    presenceSession=session;
    const active=()=>presenceSession===session&&session.cloud===transport&&!document.hidden&&navigator.onLine
      &&wasm.active_shared_presence()===raw;
    session.active=active;
    session.reader=demo?demoPresence(context,active):transport.presenceReader(context,account,active);
    session.write=transport?.presenceWriter?.(context,account,()=>active()&&document.hasFocus()&&
      !!wasm.shared_editing_presence(session.route));
  }
  const session=presenceSession;
  if(session.write)void publishPresence(session.write,document.hasFocus()?wasm.shared_editing_presence(session.route):null);
  if(!session.reader)return;
  const show=people=>wasm.set_shared_presence(session.route,JSON.stringify(people));
  show(session.reader.visible());
  if(session.pending||session.retryAfter>Date.now())return;
  session.pending=true;
  void session.reader.refresh().then(people=>{
    if(session.active())show(people);
  },()=>{
    if(session.active())show([]);
    session.retryAfter=Date.now()+8000;
  }).finally(()=>{session.pending=false;});
}

function status(text,phase='saved'){
  saveStatusText=text;
  saveStatusPhase=phase;
  if(ready)queueMicrotask(()=>wasm.set_save_status(cloudText(saveStatusText),saveStatusPhase));
}
let downloadNotice;
function beginDownloadProgress(text){
  downloadNotice?.();
  const timer=setTimeout(()=>wasm.set_download_progress(text),160);
  const finish=()=>{
    clearTimeout(timer);
    if(downloadNotice===finish){
      downloadNotice=null;
      wasm.set_download_progress('');
    }
  };
  downloadNotice=finish;
  return finish;
}
function message(text){$('message').hidden=!text;$('message').textContent=text;}
function fail(error){
  if(error?.localSaveFailed){
    const text='本地保存失败 · 可导出恢复副本';
    message(cloudText(text));
    status(text,'failed');
  }else message(cloudText('操作未完成，请重试。'));
}
function acquire(key){
  return new Promise((resolve,reject)=>{
    navigator.locks.request('feinotes:'+libraryScope+key,{ifAvailable:true},lock=>{
      if(!lock){resolve(null);return;}
      return new Promise(release=>resolve(release));
    }).catch(reject);
  });
}
function useLock(release){
  releaseLock?.();
  releaseLock=release;
  writer=!!release;
  if(ready)wasm.set_write_access(canEdit());
}
function run(fn){serial=serial.catch(()=>{}).then(fn);serial.catch(fail);return serial;}
function pendingUploadCount({outbox,shared_pending_edits,shared_moves}){
  const queued=Object.values(shared_pending_edits??{}).reduce((total,edits)=>total+edits.length,0);
  const readyMoves=Object.values(shared_moves??{}).filter(move=>!move.awaiting.length).length;
  return outbox.length+queued+readyMoves;
}
async function persist(){
  if(!ready||!writer)return;
  let state;
  try{state=wasm.export_state();}catch(e){if(String(e).includes('composition-pending')){scheduleSave();return false;}throw e;}
  const revision=saveRevision;
  try{await writeLibrary(db,key,state);}catch(cause){throw Object.assign(new Error('本地保存失败：'+(cause?.message||String(cause))),{localSaveFailed:true});}
  if(revision!==saveRevision||wasm.has_pending_document_save())return false;
  dirty=false;
  const pending=pendingUploadCount(JSON.parse(state));
  if(account&&cloudReadOnly)status('iCloud · 只读预览','read-only');
  else status(account&&pending?'已保存到此浏览器 · 待同步':'已保存到此浏览器');
  return true;
}
function markUnsaved(){saveRevision++;dirty=true;status('正在保存…','saving');}
function scheduleSave(){markUnsaved();clearTimeout(timer);timer=setTimeout(()=>run(async()=>{if(await persist()&&account)void sync({afterCurrent:true});}),400);}
function hasUnsavedChanges(){return ready&&writer&&(dirty||wasm.has_pending_document_save());}
function updateConnectionState(){
  if(ready)queueMicrotask(()=>{
    const time=lastSuccessfulSync?new Date(lastSuccessfulSync).toLocaleString(document.documentElement.lang):'';
    wasm.set_cloud_state(!!transport,!!account,navigator.onLine,$('cloud-status').textContent,canLeaveShare(),time,syncing,cloudSyncPhase,cloudReadOnly);
  });
}
function setCloudSyncPhase(phase){
  if(cloudSyncPhase===phase)return;
  cloudSyncPhase=phase;
  updateConnectionState();
}
async function refreshTrial(){
  const cloud=transport;
  if(!cloud||!navigator.onLine)return;
  const revision=++trialRequest,library=epoch,network=networkRevision;
  const current=()=>revision===trialRequest&&library===epoch&&cloud===transport&&network===networkRevision&&navigator.onLine;
  wasm.set_cloud_trial('checking');
  try{
    const expiresAt=await cloud.readTrial(current);
    if(current())wasm.set_cloud_trial(expiresAt===null?'missing':'known',expiresAt??undefined);
  }catch{
    if(current())wasm.set_cloud_trial('unavailable');
  }
}
async function readSyncTime(libraryKey){
  try{
    const time=await readLibrary(db,'sync-time:'+libraryKey);
    return typeof time==='string'&&!Number.isNaN(Date.parse(time))?time:'';
  }catch{return '';}
}
async function markSynced(){
  const stamp=new Date().toISOString();
  lastSuccessfulSync=stamp;
  try{await writeLibrary(db,'sync-time:'+key,stamp);}catch{}
  updateConnectionState();
}
function canLeaveShare(){return !restoredWorkspace&&(demo?!sharedEditPreview:!!transport?.leaveShare&&navigator.onLine);}
function showConnection(text){
  const online=navigator.onLine;
  connectionText=text;
  $('cloud-status').textContent=cloudText(online?text:'当前离线，请联网后重新连接 iCloud。');
  $('apple-sign-in-button').hidden=!!transport||!online;
  $('signout').hidden=!account;
  $('sync').hidden=!transport||!online;
  updateConnectionState();
}
async function changeAccount(nextAccount,cloud=null,firstPage=null){
  wasm.set_cloud_trial('unavailable');
  attachmentPreview.close();
  objectDirectoryPending=null;
  resetShared();
  const revision=++epoch;
  return run(async()=>{
    if(revision!==epoch)return;
    document.body.classList.add('account-switching');
    try{
      if(writer&&!await persist()){
        wasm.cancel_composition();
        if(!await persist())throw new Error('请先完成当前文档的保存');
      }
      if(revision!==epoch)return;
      const nextKey=nextAccount?(cloudReadOnly?'cloud-read-only:':'')+
        [config.environment,config.containerIdentifier,config.zoneName,nextAccount].join(':'):'local';
      const switchLock=nextKey!==key;
      const nextLock=switchLock?await acquire(nextKey):releaseLock;
      let showShared,syncTime;
      let replaced=false;
      try{
        const raw=await readLibrary(db,nextKey);
        syncTime=nextAccount?await readSyncTime(nextKey):'';
        if(revision!==epoch)return;
        showShared=!!nextAccount&&wasm.shared_reconnect_waiting();
        const destination=!switchLock&&showShared?sharedReturn:null;
        wasm.replace_library(raw||'');
        sharedReturn=destination;
        replaced=true;
      }finally{if(switchLock&&!replaced)nextLock?.();}
      account=nextAccount;key=nextKey;transport=null;lastSuccessfulSync=syncTime;cloudSyncPhase='';
      if(switchLock)useLock(nextLock);
      wasm.set_shared_write_preview(!!cloud?.writeShare&&canEdit());
      if(firstPage&&(writer||cloudReadOnly)){
        const cached=JSON.parse(wasm.export_state());
        wasm.import_changes(JSON.stringify({edits:firstPage.edits,uploaded:[],cursor:cached.cursor??firstPage.cursor??null}));
        await persist();
      }
      await writeLibrary(db,lastLibraryKey,JSON.stringify({key,account}));
      transport=cloud;
      if(!account)status('已保存到此浏览器');
      showConnection(account?(cloudReadOnly?'已连接 iCloud，可查看文档。':'已登录 Apple 账号。'):'尚未登录 Apple 账号。');
      if(cloud)void refreshTrial();
      if(!writer&&!cloudReadOnly)message('文档库已在另一个标签页打开。此标签页可以查看文档。');
      return showShared;
    }finally{document.body.classList.remove('account-switching');}
  });
}
function requireSignIn(text,reconnect=true){
  authGeneration++;
  sessionVault?.clear(cloudSessionId);
  cloudSessionId=undefined;
  transport=null;
  if(ready)wasm.set_cloud_trial('unavailable');
  objectDirectoryPending=null;
  if(!reconnect)sharedReturn=null;
  resetShared(reconnect);
  authorization?.dispose();
  authorization=null;
  if(cloudReadOnly&&account){
    void changeAccount(null).then(()=>{
      showConnection(text);
      if(reconnect&&$('settings').open)return setupCloud();
    }).catch(fail);
    return;
  }
  showConnection(text);
  if(reconnect&&$('settings').open)setupCloud().catch(fail);
}
function resetShared(rememberDestination=false){
  closePrintFrame?.();
  clearPresence();
  if(attachmentPreview.isShared())attachmentPreview.close();
  closeShareMembers();
  leaveShareRequest=null;
  $('leave-share').close();
  sharedRevision++;
  sharedTask=null;
  checkedAt=0;
  shownOwnedScopes='[]';
  shownGuestScopes='[]';
  if(!ready||demo)return;
  const route=rememberDestination&&account&&wasm.active_shared_route();
  invalidateInvitation();
  wasm.replace_shared('[]');
  if(route)sharedReturn={key,route};
  wasm.set_shared_status(navigator.onLine?'请先登录 iCloud 查看共享文档':'联网后可查看共享文档',true);
}
function resumeShared(failedCount){
  const target=sharedReturn;
  if(!target)return;
  sharedReturn=null;
  if(target.key!==key||!wasm.shared_reconnect_waiting()||$('invitation').open)return;
  if(wasm.resume_shared_route(target.route))return;
  if(failedCount)sharedReturn=target;
  else wasm.set_shared_status('原共享项目无法打开，请检查当前账号的共享权限。');
}
function rememberSharedOnReload(){
  if(!ready||demo||restoredWorkspace)return;
  try{
    sessionStorage.removeItem(sharedReloadKey);
    if(!account||!wasm.remembers_last_document()||$('invitation').open)return;
    const route=wasm.active_shared_route();
    const target=route?{key,route}:wasm.shared_reconnect_waiting()?sharedReturn:null;
    if(target?.key===key&&sharedRoutePattern.test(target.route))sessionStorage.setItem(sharedReloadKey,JSON.stringify(target));
  }catch{}
}
function sharedReloadTarget(){
  if(demo||restoredWorkspace)return;
  let target;
  try{
    const saved=sessionStorage.getItem(sharedReloadKey);
    sessionStorage.removeItem(sharedReloadKey);
    if(saved)target=JSON.parse(saved);
  }catch{return;}
  if(account&&target?.key===key&&typeof target.route==='string'&&sharedRoutePattern.test(target.route))return target;
}
const sharedScopeKey=({owner,root,folder,owned})=>JSON.stringify([owner,root,folder,owned===true]);
const sharedScopeList=scopes=>JSON.stringify(scopes.map(sharedScopeKey).sort());
const composing=error=>String(error).includes('composition-pending');
async function readSharedBranch(request){
  try{return await request;}
  catch(error){
    if(isExpiredSession(error)||composing(error))throw error;
    return null;
  }
}
function refreshShared({validateOnly=false,prompt=false}={}){
  if(!ready||restoredWorkspace)return;
  if(demo){
    if(checkedAt)return;
    wasm.replace_shared(demoShares);
    wasm.set_shared_status(sharedEditPreview?'合成共享库':'演示文档 · 只读预览');
    checkedAt=Date.now();
    return;
  }
  if(!navigator.onLine){wasm.set_shared_status('联网后可查看共享文档',true);return;}
  const cloud=transport;
  if(!cloud?.readAcceptedShares){
    wasm.set_shared_status('请先登录 iCloud 查看共享文档',true);
    if(prompt)openAccount();
    return;
  }
  if(sharedTask)return sharedTask;
  const revision=sharedRevision;
  const owner=account;
  const current=()=>revision===sharedRevision&&transport===cloud&&navigator.onLine;
  const activeScope=wasm.active_shared_scope();
  const focused=activeScope?JSON.parse(activeScope):null;
  const notesFor=scope=>focused&&sharedScopeKey(scope)===sharedScopeKey(focused.scope)?[focused.route.split(':').at(-1)]:[];
  if(!validateOnly)wasm.set_shared_status('正在读取共享文档…');
  sharedTask=(async()=>{
    try{
      const candidates=JSON.parse(wasm.owned_share_roots());
      const [ownedResult,guestResult]=await Promise.all([
        readSharedBranch(cloud.ownedShareScopes(candidates,owner)),
        readSharedBranch(validateOnly?cloud.acceptedShareScopes():cloud.readAcceptedShares(notesFor)),
      ]);
      if(!current())return;
      const owned=ownedResult??[];
      const accepted=validateOnly?guestResult??[]:[];
      if(validateOnly){
        if(wasm.retain_shared(JSON.stringify([...accepted,...owned]))){
          closeShareMembers();
          if(attachmentPreview.isShared())attachmentPreview.close();
          wasm.set_shared_status('共享范围已更新 · 只读预览');
        }
      }
      if(!validateOnly||!ownedResult||!guestResult||sharedScopeList(accepted)!==shownGuestScopes||sharedScopeList(owned)!==shownOwnedScopes||owned.length!==wasm.loaded_owned_share_count()){
        const [guest,own]=await Promise.all([
          validateOnly&&guestResult?readSharedBranch(cloud.readAcceptedShares(notesFor)):guestResult,
          ownedResult?readSharedBranch(cloud.readOwnedShares(owned,owner,notesFor)):null,
        ]);
        if(!current())return;
        closeShareMembers();
        if(attachmentPreview.isShared())attachmentPreview.close();
        const rooms=[...(guest?.rooms??[]),...(own?.rooms??[])];
        if(!canEdit())for(const room of rooms)room.editable=false;
        const rejected=wasm.replace_shared(JSON.stringify(rooms));
        const failedCount=(guest?.failed??1)+(own?.failed??1)+rejected+(candidates.length-owned.length);
        shownOwnedScopes=own&&!rejected?sharedScopeList(own.rooms):null;
        shownGuestScopes=guest&&!rejected?sharedScopeList(guest.rooms):null;
        const status=failedCount
          ? rooms.length>rejected?'部分共享文档暂时无法打开，请重试':'暂时无法验证共享访问，请重新检查'
          : rooms.some(room=>room.editable)?'共享文档':rooms.length?'共享文档 · 只读预览':'暂无共享文档';
        wasm.set_shared_status(status);
        resumeShared(failedCount);
      }else{
        const active=wasm.active_shared_scope();
        if(active){
          const {route,scope}=JSON.parse(active);
          try{
            const note=route.split(':').at(-1);
            const room=await cloud.readUpdatedShare(scope,owner,note===scope.root&&scope.folder?null:note);
            if(!current())return;
            if(room){
              wasm.update_shared_room(JSON.stringify({...room,editable:canEdit()&&room.editable}),route);
              if(!room.roleOnly&&wasm.active_shared_route()===route&&attachmentPreview.isShared())attachmentPreview.close();
            }
            const sameDocument=()=>current()&&wasm.active_shared_route()===route;
            if(cloudReadOnly&&sameDocument()){
              await readSharedBodyAssets(cloud,scope,owner,route,sameDocument,assets=>
                wasm.update_shared_room(JSON.stringify({...assets,editable:false}),route));
            }
          }catch(error){
            if(!current())return;
            if(isExpiredSession(error)||composing(error))throw error;
            const unchanged=[...accepted,...owned].filter(item=>sharedScopeKey(item)!==sharedScopeKey(scope));
            if(wasm.active_shared_route()===route){closeShareMembers();if(attachmentPreview.isShared())attachmentPreview.close();}
            wasm.retain_shared(JSON.stringify(unchanged));
            if(scope.owned)shownOwnedScopes=null;else shownGuestScopes=null;
            wasm.set_shared_status('此共享文档暂时无法更新，请重试');
          }
        }
      }
      checkedAt=Date.now();
    }catch(error){
      if(!current())return;
      if(composing(error)){
        shownOwnedScopes=shownGuestScopes=null;
        return;
      }
      if(isExpiredSession(error))requireSignIn('Apple 会话已失效，请重新登录。');
      else{
        closeShareMembers();
        if(attachmentPreview.isShared())attachmentPreview.close();
        wasm.replace_shared('[]');
        shownOwnedScopes='[]';
        shownGuestScopes='[]';
        wasm.set_shared_status('暂时无法验证共享访问，请重新检查');
        checkedAt=Date.now();
      }
    }finally{if(current())sharedTask=null;}
  })();
  return sharedTask;
}
async function readSharedBodyAssets(cloud,scope,owner,route,current,apply,
  paths=new Set(JSON.parse(wasm.missing_body_asset_paths(route)))){
  if(!paths.size)return;
  const operation=Symbol();
  sharedAssetRead=operation;
  wasm.set_shared_asset_loading(route);
  try{
    const assets=await cloud.readSharedDocument(scope,assetScope,owner,current,paths);
    if(current())await apply(assets);
  }catch(error){
    if(current())throw error;
  }finally{
    if(sharedAssetRead===operation){
      sharedAssetRead=undefined;
      wasm.set_shared_asset_loading();
    }
  }
}
function applyPersonalBody(edits,current){
  return run(async()=>{
    if(!current())return;
    const {cursor}=JSON.parse(wasm.export_state());
    wasm.import_changes(JSON.stringify({edits,uploaded:[],cursor}));
    if(writer)await persist();
  });
}

async function requestCloudAttachment({path,name,route,download}){
  if(!cloudReadOnly||!path?.startsWith('assets/'))return;
  const english=document.documentElement.lang.startsWith('en');
  if(!transport||!navigator.onLine){
    message(english?'Connect to iCloud to download this attachment.':'请联网并连接 iCloud 后下载附件。');
    return;
  }
  const cloud=transport,owner=account,started=epoch,sharedVersion=sharedRevision;
  const note=route?null:wasm.active_personal_document();
  const shared=route?JSON.parse(wasm.active_shared_scope()??'null'):null;
  if(route?shared?.route!==route:!note)return;
  const operation=Symbol(),paths=new Set([path]);
  const current=()=>attachmentRead===operation&&cloud===transport&&started===epoch&&navigator.onLine&&
    (route?sharedVersion===sharedRevision&&wasm.active_shared_route()===route:wasm.active_personal_document()===note);
  const finishProgress=beginDownloadProgress(english?'Downloading attachment…':'正在下载附件…');
  const failure=english?'Could not download this attachment. Try again.':'附件未能下载，请重试。';
  attachmentRead=operation;
  try{
    if(route){
      while(sharedTask){await sharedTask;if(!current())return;}
      const apply=room=>run(async()=>{
        if(current())wasm.update_shared_room(JSON.stringify({...room,editable:false}),route);
      });
      const task=readSharedBodyAssets(cloud,shared.scope,owner,route,current,apply,paths);
      sharedTask=task;
      try{await task;}finally{if(sharedTask===task)sharedTask=null;}
    }else{
      const {received}=JSON.parse(wasm.export_state());
      const edits=await cloud.readDocumentScope(assetScope,current,received,paths);
      await applyPersonalBody(edits,current);
    }
    if(current()&&!wasm.finish_cloud_attachment(path,name,route??undefined,download))message(failure);
  }catch(error){
    if(!current())return;
    if(isExpiredSession(error))requireSignIn('Apple 会话已失效，请重新登录。');
    else if(error?.code==='SHARED_ACCESS_CHANGED')void refreshShared({validateOnly:true});
    else message(failure);
  }finally{
    finishProgress();
    if(attachmentRead===operation)attachmentRead=null;
  }
}

async function loadDocumentBody(target,action){
  const open=action==='open',printing=action==='print',exportBody=action==='export'||printing,requested=open||exportBody;
  if(!requested&&openingBody===target.id)return;
  if(!transport||!navigator.onLine){
    if(requested)message('这篇文档尚未下载，请联网并连接 iCloud 后重试。');
    return;
  }
  const revision=++bodyRequest,started=epoch,cloud=transport,owner=account;
  const current=()=>revision===bodyRequest&&started===epoch&&cloud===transport&&navigator.onLine;
  const inView=()=>target.scope?wasm.active_shared_route()===target.id:wasm.active_personal_document()===target.id;
  let opened=false;
  const finishProgress=requested?beginDownloadProgress(cloudText('正在下载文档…')):()=>{};
  const finishOpen=()=>{
    if(!current())return false;
    openingBody=target.id;
    try{
      opened=wasm.finish_body_open(target.id,target.location);
      finishProgress();
      if(!opened)message('暂时无法打开此文档，请刷新目录或重试。');
      return opened;
    }finally{openingBody=null;}
  };
  try{
    if(target.scope){
      while(sharedTask){await sharedTask;if(!current())return;}
      const work=(async()=>{
        if(!requested&&!inView())return;
        const room=await cloud.readSharedDocument(target.scope,target.note,owner,current);
        if(!current())return;
        const replace=room=>run(async()=>{
          if(current())wasm.update_shared_room(JSON.stringify({...room,editable:canEdit()&&room.editable}),target.id);
        });
        await replace(room);
        if(open&&!finishOpen())return;
        if(current())
          await readSharedBodyAssets(cloud,target.scope,owner,target.id,current,replace);
      })();
      sharedTask=work;
      try{await work;}finally{if(sharedTask===work)sharedTask=null;}
    }else{
      if(!requested&&!inView())return;
      const stored=JSON.parse(wasm.export_state());
      const edits=await cloud.readDocumentScope(target.note,current,stored.received);
      await applyPersonalBody(edits,current);
      if(open&&cloudReadOnly&&!finishOpen())return;
      const readingAssets=()=>current()&&(!cloudReadOnly||exportBody||inView());
      if(readingAssets()){
        const paths=new Set(JSON.parse(wasm.missing_body_asset_paths(target.id)));
        if(paths.size){
          const assets=await cloud.readDocumentScope(assetScope,readingAssets,stored.received,paths);
          await applyPersonalBody(assets,readingAssets);
        }
      }
      if(open&&!opened)finishOpen();
    }
    if(exportBody&&current()){
      wasm.finish_body_export(target.id,target.location,printing);
    }
  }catch(error){
    if(!current())return;
    if(isExpiredSession(error)){requireSignIn('Apple 会话已失效，请重新登录。');return;}
    if(requested&&(!opened||inView()))message(opened?'部分附件尚未下载，请重试。':'文档下载未完成，请重试。');
  }finally{finishProgress();}
}
async function syncSharedWrites(cloud,active){
  if(!cloud.writeShare)return;
  while(sharedTask){await sharedTask;if(!active())return;}
  const revision=sharedRevision;
  const current=()=>active()&&revision===sharedRevision&&navigator.onLine;
  const work=(async()=>{
    const owner=account;
    let firstFailure;
    for(const roomKey of JSON.parse(wasm.shared_write_rooms())){
      if(!current()||dirty)return firstFailure;
      const raw=wasm.shared_write_batch(roomKey);
      if(!raw)continue;
      const {scope,route,title,edits}=JSON.parse(raw);
      const scopes=edits.map(edit=>edit.note);
      const opened=wasm.active_shared_scope();
      if(opened){
        const active=JSON.parse(opened);
        if(sharedScopeKey(active.scope)===sharedScopeKey(scope))scopes.push(active.route.split(':').at(-1));
      }
      const pull=async()=>{
        setCloudSyncPhase('downloading');
        const room=await cloud.readShareForWrite(scope,owner,scopes);
        if(!current())return null;
        const saved=await run(async()=>{
          if(!current())return;
          setCloudSyncPhase('applying');
          wasm.update_shared_room(JSON.stringify(room),route);
          return persist();
        });
        return saved?room:null;
      };
      try{
        const room=await pull();
        if(!room?.editable||!current()||dirty)continue;
        const queue=wasm.shared_write_batch(roomKey);
        if(!queue)continue;
        setCloudSyncPhase('uploading');
        await cloud.writeShare(scope,owner,JSON.parse(queue).edits,room.generation,current);
        if(current())await pull();
      }catch(error){
        if(!current())return firstFailure;
        if(isExpiredSession(error)||error?.localSaveFailed||composing(error))throw error;
        if(!firstFailure){
          const english=document.documentElement.lang==='en';
          const userMessage=english
            ? `${title?`“${title}”`:'A shared document'} could not sync. The changes are saved in this browser.`
            : `${title?`「${title}」`:'有共享文档'}暂未同步。改动仍保存在此浏览器。`;
          firstFailure=Object.assign(new Error(userMessage,{cause:error}),{
            ckErrorCode:error?.ckErrorCode,serverErrorCode:error?.serverErrorCode,
          });
        }
      }
    }
    return firstFailure;
  })();
  sharedTask=work;
  try{return await work;}
  finally{if(sharedTask===work)sharedTask=null;}
}
async function pullPrivateChanges(cloud,active){
  for(let pages=0;pages<200;pages++){
    if(!active())return false;
    const stored=await run(async()=>(!writer||await persist())?JSON.parse(wasm.export_state()):null);
    if(!stored||!active())return false;
    setCloudSyncPhase('downloading');
    const note=wasm.active_personal_document();
    const scopes=[catalogueScope,...(note?[note]:[])];
    const page=await cloud.changes(stored.cursor,{current:active,scopes});
    if(!active())return false;
    const applied=await run(async()=>{
      if(!active())return false;
      setCloudSyncPhase('applying');
      wasm.import_changes(JSON.stringify({...page,uploaded:[]}));
      return !writer||persist();
    });
    if(!applied)return false;
    if(!page.more){
      const sameDocument=()=>active()&&wasm.active_personal_document()===note;
      const paths=cloudReadOnly&&note&&sameDocument()?new Set(JSON.parse(wasm.missing_body_asset_paths(note))):null;
      if(paths?.size){
        const {received}=JSON.parse(wasm.export_state());
        const edits=await cloud.readDocumentScope(assetScope,sameDocument,received,paths)
          .catch(error=>{if(sameDocument())throw error;return [];});
        if(edits.length)await run(async()=>{
          if(!sameDocument())return;
          const {cursor}=JSON.parse(wasm.export_state());
          wasm.import_changes(JSON.stringify({edits,uploaded:[],cursor}));
          if(writer)await persist();
        });
      }
      return active();
    }
  }
  throw new Error('同步记录较多，请再次同步以继续');
}
async function sync({afterCurrent=false}={}){
  if(!ready||(!writer&&!cloudReadOnly)||!account||!transport||!navigator.onLine)return;
  if(syncing){if(afterCurrent)syncRequested=true;return;}
  syncing=true;cloudSyncPhase='checking';updateConnectionState();
  const startEpoch=epoch,cloud=transport,network=networkRevision;
  const active=()=>startEpoch===epoch&&cloud===transport&&network===networkRevision&&navigator.onLine;
  try{
    if(!await run(()=>active()&&(!writer||persist()))||dirty||!active())return;
    if(!cloudReadOnly)status('同步中…','syncing');
    if(!cloudReadOnly)await cloud.ensureZone(active);
    const previousOwners=wasm.owned_share_roots();
    // Pull before upload so a remote deletion cannot resurrect a note.
    if(!await pullPrivateChanges(cloud,active))return;
    if(objectDirectoryPending===cloud){
      objectDirectoryPending=null;
      if(pendingObject)void openPendingObject();
    }
    if(previousOwners!==wasm.owned_share_roots()){
      if(sharedTask)await sharedTask;
      if(active())void refreshShared({validateOnly:checkedAt!==0});
    }
    if(cloudReadOnly){
      if(active()){
        status('iCloud · 只读预览','read-only');
        await markSynced();
        if(active())setCloudSyncPhase('up-to-date');
      }
      return;
    }
    let sharedFailure;
    try{
      sharedFailure=await syncSharedWrites(cloud,active);
      if(cloud.writeShare&&wasm.has_ready_shared_move()){
        // The owner could have changed the personal original while the other
        // zone was committing. Re-read it before publishing its new pointer.
        if(!await pullPrivateChanges(cloud,active))return;
        await run(async()=>{
          if(!active())return;
          wasm.finish_shared_move();
          await persist();
        });
      }
    }
    catch(error){
      if(composing(error)){scheduleSave();return;}
      if(isExpiredSession(error)||error?.localSaveFailed)throw error;
      sharedFailure=error;
    }
    const snapshot=await run(async()=>{
      if(!active())return null;
      if(!document.hidden&&wasm.expire_trash(true))await persist();
      return JSON.parse(wasm.export_state());
    });
    if(!snapshot)return;
    for(const edit of snapshot.outbox){
      if(!active())return;
      if(!wasm.upload_pending(edit.id))continue;
      setCloudSyncPhase('uploading');
      const uploaded=await cloud.upload(edit,active);
      if(!active())return;
      const saved=await run(async()=>{
        if(!active())return false;
        const now=JSON.parse(wasm.export_state());
        wasm.import_changes(JSON.stringify({edits:[],uploaded:[uploaded],cursor:now.cursor}));
        return persist();
      });
      if(!saved)return;
    }
    if(sharedFailure)throw sharedFailure;
    if(active()){
      const snapshot=JSON.parse(wasm.export_state());
      const {held,pending,conflicts,shared_moves}=snapshot;
      const pausedMove=Object.values(shared_moves??{}).some(move=>move.paused);
      const uploading=pendingUploadCount(snapshot)>0;
      const receiving=Object.keys(pending??{}).length>0;
      const conflicted=Object.keys(conflicts??{}).length>0;
      message('');
      if(dirty)status('正在保存…','saving');
      else if(pausedMove||conflicted||held?.length)status('有文档需要处理','attention');
      else if(uploading||receiving)status('同步中…','syncing');
      else status('已同步');
      if(!dirty&&!pausedMove&&!conflicted&&!held?.length&&!uploading&&!receiving){
        await markSynced();
        if(active())setCloudSyncPhase('up-to-date');
      }
    }
  }catch(e){if(active()){
    if(isExpiredSession(e)){
      requireSignIn('Apple 会话已失效，请重新登录。');
      if(!cloudReadOnly){status('待同步 · 请重新登录','attention');message('已缓存文档和待同步修改都保留在此浏览器。');}
    }
    else{
      const quota=[e?.ckErrorCode,e?.serverErrorCode].includes('QUOTA_EXCEEDED');
      setCloudSyncPhase(e?.localSaveFailed?'local-save-failed':quota?'quota':'retry');
      fail(e);
      if(!cloudReadOnly&&!e?.localSaveFailed&&!dirty)status('同步未完成 · 已保存在此浏览器','attention');
    }
  }}
  finally{
    syncing=false;
    if(['checking','downloading','applying','uploading'].includes(cloudSyncPhase))cloudSyncPhase='';
    updateConnectionState();
    if(syncRequested){syncRequested=false;void sync();}
  }
}
function syncNow(){
  void sync({afterCurrent:true});
  if(transport)void refreshShared();
}
function setupCloud({restoreOnly=false}={}){
  if(transport)return Promise.resolve();
  if(authorization)return authorization.prepare();
  if(setupPromise)return setupPromise;
  const generation=authGeneration;
  setupPromise=(async()=>{
    const [CloudKit,profile]=await Promise.all([
      loadCloudKit(),
      fetch('./config.json',{cache:'no-store',redirect:'error',signal:AbortSignal.timeout(12000)}).then(async response=>{
        if(!response.ok)throw new Error('尚未配置 CloudKit Web 授权');
        const value=await response.json();
        if(!isProductionLibrary(value)||location.origin!=='https://feinotes.app')throw new Error('CloudKit Web 配置无效');
        return value;
      }),
    ]);
    if(generation!==authGeneration)return;
    config=profile;
    const saved=sessionVault?.read();
    const scope=sessionScope(config);
    const active=()=>generation===authGeneration;
    async function connect(entry,previous){
      const {session,clientId}=entry;
      const browserSession=sessionVault?.connect(entry,previous,active);
      const cloud=createPrivateTransport(config,CloudKit,session,clientId,active,
        {readShared:true,previewInvitations:invitationPreview,previewLeaveShares:config.previewLeaveShares===true,
          allowInvitationAcceptance:canAcceptInvitation,
          previewPresence:presencePreview,previewPresenceWrites:config.previewPresenceWrites===true,
          previewSharedWrites:localPreview&&config.previewSharedWrites===true,lazyBodies:true,readOnly:cloudReadOnly,
          sessionRequest:browserSession?.request});
      if(entry.owner)cloud.owner=entry.owner;
      let firstPage;
      try{firstPage=await cloud.changes(null,{bootstrap:true,scopes:[catalogueScope]});}
      catch(error){
        if(isExpiredSession(error))error.userMessage='Apple 授权未能读取此 iCloud 文档库，请重新登录。';
        else if([error?.ckErrorCode,error?.serverErrorCode].includes('ZONE_NOT_FOUND'))error.userMessage='找不到飞记的 iCloud 文档库。请先在 Mac 打开飞记并同步。';
        throw error;
      }
      if(!active())return;
      if(browserSession){
        const remembered=await browserSession.remember(firstPage.owner);
        if(!active())return;
        cloudSessionId=remembered?clientId:undefined;
      }
      const showShared=await changeAccount(firstPage.owner,cloud,firstPage);
      if(!active())return;
      objectDirectoryPending=firstPage.more&&(writer||cloudReadOnly)?cloud:null;
      authorization?.dispose();
      authorization=null;
      $('settings').close();
      if(writer||cloudReadOnly)message('');
      if($('invitation').open){
        $('settings').close();
        if(checkInvitationAfterSignIn)void inspectInvitation();
        else $('invitation-status').textContent='已连接 iCloud，请检查邀请。';
      }
      if(showShared){
        const target=sharedReturn;
        wasm.show_shared();
        sharedReturn=target;
      }
      else if(!$('invitation').open&&!pendingObject)void refreshShared();
      if(writer||cloudReadOnly)void sync({afterCurrent:true});
      if(pendingObject)void openPendingObject();
    }
    if(saved?.scope===scope){
      showConnection('正在连接 iCloud…');
      try{await connect(saved);return;}
      catch(error){
        if(!active())return;
        if(isExpiredSession(error)||error?.code==='ACCOUNT_MISMATCH')sessionVault.clear(saved.clientId);
        showConnection(error?.userMessage||(error?.code==='ACCOUNT_MISMATCH'?
          'iCloud 账号已变化，请重新登录。':'无法恢复 iCloud 会话，请重试。'));
      }
    }
    if(restoreOnly&&!$('settings').open)return;
    const previous=sessionVault?.read()?.clientId;
    authorization=new AppleSignIn(config,$('apple-sign-in-button'),(session,clientId)=>
      connect({session,clientId,scope},previous),showConnection,cloudText);
    await authorization.prepare();
  })().finally(()=>{setupPromise=null;});
  return setupPromise;
}
function openAccount(){
  if(restoredWorkspace)return;
  if(!$('settings').open)$('settings').showModal();
  if(demo){
    showConnection('这是此浏览器中的演示文档库。');
    $('apple-sign-in-button').hidden=true;
    return;
  }
  showConnection(transport?(cloudReadOnly?'已连接 iCloud，可查看文档。':'已登录 Apple 账号。'):
    (cloudReadOnly?'连接 Apple 账号，查看 iCloud 文档。':'使用 Apple 账号读取和同步文档。'));
  if(navigator.onLine)setupCloud().catch(()=>showConnection('暂时无法连接 iCloud，请稍后重试。'));
}
function objectStatus(text,canSignIn=false){
  $('object-link-status').textContent=text;
  $('object-link-sign-in').hidden=!canSignIn;
  $('object-link-retry').hidden=!transport;
  if(!$('object-link').open&&!$('settings').open)$('object-link').showModal();
}
async function openPendingObject(){
  const target=pendingObject;
  if(!ready||!target)return;
  const cloud=transport,owner=account,started=++objectRequest;
  if(!cloud||!owner){
    objectStatus('登录 iCloud 后继续打开原文档或文件夹。',true);
    return;
  }
  if(objectDirectoryPending===cloud){
    objectStatus('正在同步目录，完成后继续打开。');
    void sync();
    return;
  }
  const current=()=>transport===cloud&&account===owner&&pendingObject===target&&started===objectRequest;
  objectStatus('正在查找原文档或文件夹…');
  try{
    let opened=false;
    if(target.kind===1){
      const range=await personalScope(owner,config.zoneName);
      if(!current())return;
      if(scopeMatches(range,target))opened=wasm.open_object_link(target.object);
    }else{
      while(sharedTask){await sharedTask;if(!current())return;}
      const work=(async()=>{
        const [owned,joined]=await Promise.all([
          cloud.ownedShareScopes(JSON.parse(wasm.owned_share_roots()),owner),
          cloud.acceptedShareScopes(),
        ]);
        if(!current())return;
        const matched=(await Promise.all([...owned,...joined].map(async scope=>scopeMatches(await sharedScope(scope),target)?scope:null))).filter(Boolean);
        if(!current()||matched.length!==1)return;
        const scope=matched[0];
        if(!scope.folder&&scope.root!==target.object)return;
        const room=await cloud.readSharedDocument(scope,target.object,owner,current);
        if(!current())return;
        wasm.merge_linked_shared_room(JSON.stringify({...room,editable:canEdit()&&room.editable}));
        opened=wasm.open_object_link(target.object,JSON.stringify(scope));
        shownOwnedScopes=shownGuestScopes=null;
      })();
      const task=work.catch(()=>{});
      sharedTask=task;
      try{await work;}finally{if(sharedTask===task)sharedTask=null;}
    }
    if(!current())return;
    if(opened){
      pendingObject=null;
      $('object-link').close();
      void refreshShared({validateOnly:true});
    }else objectStatus('找不到此对象，或当前账号没有访问权限。请确认账号和同步状态。',true);
  }catch(error){
    if(!current())return;
    if(isExpiredSession(error))requireSignIn('Apple 会话已失效，请重新登录。');
    objectStatus('暂时无法打开此链接，请确认当前账号和网络状态。',true);
  }
}
$('object-link-sign-in').onclick=()=>{
  $('object-link').close();
  if(transport)requireSignIn('请使用可访问原对象的 Apple 账号登录。',false);
  openAccount();
};
$('object-link-retry').onclick=openPendingObject;
$('settings').addEventListener('close',()=>{
  if(pendingObject&&!transport)objectStatus('登录 iCloud 后继续打开原文档或文件夹。',true);
});
$('object-link-close').onclick=$('object-link').oncancel=()=>{
  pendingObject=null;
  objectRequest++;
  history.replaceState(history.state,'',location.pathname+location.search);
};
async function resolveObjectLink({object,scope}){
  const owner=account,cloud=transport;
  if(!owner||!cloud)throw new Error('iCloud unavailable');
  const range=await (scope?sharedScope(scope):personalScope(owner,config.zoneName));
  if(transport!==cloud||account!==owner)throw new Error('Account changed');
  const url=new URL(encodeObjectLink(scope?2:1,range,object));
  const id=url.searchParams.get('id');
  if(!decodeObjectId(id))throw new Error('Invalid object link');
  return new URL('./#object='+id,location.href).href;
}
function copyObjectLink(target){
  if(!account||!transport||!navigator.clipboard){message('暂时无法复制链接，请登录 iCloud 后重试。');return;}
  const text=resolveObjectLink(target).then(link=>new Blob([link],{type:'text/plain'}));
  navigator.clipboard.write([new ClipboardItem({'text/plain':text})]).then(
    ()=>message('已复制链接。'),()=>message('暂时无法复制链接，请重试。'));
}
function openObjectTab(target){
  const tab=window.open('about:blank','_blank');
  if(!tab){message(cloudText('浏览器未打开新标签页。'));return;}
  tab.opener=null;
  resolveObjectLink(target).then(link=>tab.location.replace(link))
    .catch(()=>{tab.close();message(cloudText('暂时无法打开此文档，请重试。'));});
}
function invalidateInvitation(){
  invitationRevision++;
  inspectedInvitation=null;
  $('invitation-details').hidden=true;
  $('invitation-details').textContent='';
  $('invitation-accept').hidden=true;
  $('invitation-switch-account').hidden=true;
}
function showInvitation(link){
  if(!invitationPreview||$('invitation').open)return;
  invalidateInvitation();
  $('invitation-status').textContent='';
  $('invitation-demo').hidden=!demo;
  $('invitation-link').value=link??(demo?demoInvitation:'');
  $('invitation-link').readOnly=!!link;
  $('invitation').showModal();
}
function setInvitationBusy(busy){
  invitationBusy=busy;
  for(const id of ['invitation-link','invitation-check','invitation-accept','invitation-cancel','invitation-switch-account'])$(id).disabled=busy;
}
function invitationError(error,fallback){
  if(isExpiredSession(error)){
    return '请先在飞记 App 加入共享。如已加入，可从网页版的「共享文档」打开。';
  }
  const code=[error?.serverErrorCode,error?.ckErrorCode].find(value=>typeof value==='string'&&/^[A-Z][A-Z0-9_]{0,60}$/.test(value));
  if(code==='INVITATION_ACCOUNT_REQUIRED'||['ACCESS_DENIED','PERMISSION_FAILURE','NOT_AUTHORIZED','FORBIDDEN'].includes(code)){
    $('invitation-switch-account').hidden=false;
    return '此 Apple 账号无法访问这项共享。请切换到受邀账号，或请邀请者检查共享权限。';
  }
  if(['NOT_FOUND','UNKNOWN_ITEM','ZONE_NOT_FOUND','ZONE_DELETED'].includes(code))
    return '找不到这项共享。请向邀请者确认链接是否有效。';
  return fallback;
}
$('invitation-form').onsubmit=event=>{
  event.preventDefault();
  void inspectInvitation();
};
async function inspectInvitation(){
  if(invitationBusy||!$('invitation').open)return;
  checkInvitationAfterSignIn=false;
  invalidateInvitation();
  const value=$('invitation-link').value.trim();
  let parsed;
  try{parsed=parseInvitation(value);}
  catch{$('invitation-status').textContent='请粘贴完整的飞记或 iCloud 共享邀请链接。';return;}
  if(!demo&&!navigator.onLine){
    $('invitation-status').textContent='联网后可检查邀请。';
    return;
  }
  const cloud=transport;
  if(!demo&&!cloud?.resolveInvitation){
    checkInvitationAfterSignIn=true;
    $('invitation-status').textContent='请先连接接收邀请的 iCloud 账号。';
    openAccount();
    return;
  }
  const revision=invitationRevision;
  let openAccepted=false;
  setInvitationBusy(true);
  $('invitation-status').textContent='正在检查邀请…';
  try{
    let info;
    if(demo){
      if(value!==demoInvitation){$('invitation-status').textContent='请使用此演示页提供的邀请链接。';return;}
      const rooms=JSON.parse(wasm.demo_shares()),{owner,root,folder}=rooms[0];
      info={title:'发布安排（共享）',region:parsed.region,status:'INVITED',permission:'READ_ONLY',
        scope:{owner,root,folder},accept:async()=>{wasm.replace_shared(JSON.stringify(rooms));return {owner,root,folder};}};
    }else info=await cloud.resolveInvitation(value);
    if(revision!==invitationRevision||cloud!==transport)return;
    const title=info.title||(info.scope.folder?'共享文件夹':'共享文档');
    const permission=info.permission==='READ_WRITE'?'可编辑':'只读';
    inspectedInvitation={info};
    $('invitation-details').textContent=title+' · '+permission;
    $('invitation-details').hidden=false;
    const accepted=info.status==='ACCEPTED';
    $('invitation-status').textContent=accepted?'此账号已接受邀请。':canAcceptInvitation?'确认后将在飞记中打开。':openInvitationInApp;
    $('invitation-accept').textContent=accepted?'打开共享':'接受并打开';
    $('invitation-accept').hidden=!accepted&&!canAcceptInvitation;
    openAccepted=!!incomingInvitation&&accepted;
  }catch(error){
    if(revision===invitationRevision&&cloud===transport)$('invitation-status').textContent=invitationError(error,'无法验证此邀请，请核对 Apple 账号和链接');
  }finally{setInvitationBusy(false);}
  if(openAccepted&&revision===invitationRevision&&cloud===transport)await openInspectedInvitation();
}
async function openInspectedInvitation(){
  if(invitationBusy||!inspectedInvitation||!$('invitation').open)return;
  const request=inspectedInvitation;
  if(request.info.status!=='ACCEPTED'&&!canAcceptInvitation)return;
  const cloud=transport,revision=invitationRevision;
  const current=()=>cloud===transport&&revision===invitationRevision;
  setInvitationBusy(true);
  $('invitation-status').textContent=request.scope?'正在打开共享…':'正在确认并打开共享…';
  try{
    request.scope??=await request.info.accept();
    if(!current())return;
    if(demo){checkedAt=Date.now();wasm.set_shared_status('演示文档 · 只读预览');}
    else{
      while(sharedTask){await sharedTask;if(!current())return;}
      const work=(async()=>{
        if(!navigator.onLine)throw new Error('共享内容需要联网');
        const room=await cloud.readInvitedShare(request.scope,account);
        if(!current())return;
        if(!navigator.onLine)throw new Error('共享内容需要联网');
        const editable=canEdit()&&room.editable;
        wasm.merge_invited_shared_room(JSON.stringify({...room,editable}));
        shownGuestScopes=null;
        if(!checkedAt){
          checkedAt=Date.now();
          wasm.set_shared_status(editable?'共享文档':'共享文档 · 只读预览');
        }
      })();
      const task=work.catch(()=>{});
      sharedTask=task;
      try{await work;}finally{if(sharedTask===task)sharedTask=null;}
    }
    if(!current())return;
    if(wasm.open_shared(JSON.stringify(request.scope)))$('invitation').close();
    else{
      $('invitation-status').textContent='邀请已接受，正文暂时无法打开。';
      $('invitation-accept').textContent='重试打开';
    }
  }catch(error){
    if(current()){
      if(request.scope&&!isExpiredSession(error)){
        $('invitation-status').textContent=invitationError(error,'邀请已接受，共享内容暂时无法打开，请重试');
        $('invitation-accept').textContent='重试打开';
      }else{
        invalidateInvitation();
        $('invitation-status').textContent=invitationError(error,'暂时无法确认邀请状态，请先重新检查邀请');
      }
    }
  }finally{setInvitationBusy(false);}
}
$('invitation-accept').onclick=openInspectedInvitation;
$('invitation-switch-account').onclick=()=>{
  if(invitationBusy)return;
  requireSignIn('请使用受邀的 Apple 账号登录。',false);
  void inspectInvitation();
};
$('invitation-link').addEventListener('input',()=>{
  checkInvitationAfterSignIn=false;
  invalidateInvitation();
  $('invitation-status').textContent='';
});
$('invitation-cancel').onclick=()=>$('invitation').close();
$('invitation').addEventListener('cancel',event=>{if(invitationBusy)event.preventDefault();});
$('invitation').addEventListener('close',()=>{
  if($('invitation').open)return;
  checkInvitationAfterSignIn=false;
  invalidateInvitation();
  $('invitation-link').value='';
  if(incomingInvitation){
    if(shareFragment.test(location.hash))history.replaceState(history.state,'',location.pathname+location.search);
    incomingInvitation=null;
  }
  if(!demo&&transport&&!checkedAt)void refreshShared();
});
function openRecovery(){
  if(!ready)return;
  try{
    const xml=wasm.current_xml();
    if(!xml)return;
    $('recovery-content').value=xml;
    $('recovery').showModal();
  }catch(e){fail(e);}
}
function closeShareMembers(){
  membersRequest=null;
  membersVersion++;
  const dialog=$('share-members');
  if(dialog.open)dialog.close();
  dialog.classList.remove('anchored-members');
  dialog.style.removeProperty('left');
  dialog.style.removeProperty('top');
  $('share-members-list').replaceChildren();
}
function positionShareMembers(){
  const dialog=$('share-members');
  const anchor=membersRequest?.anchor;
  if(!dialog.open||!anchor)return;
  const {width,height}=dialog.getBoundingClientRect();
  const beside=anchor.x+12+width<=innerWidth-8?anchor.x+12:anchor.x-width-8;
  dialog.style.left=Math.max(8,Math.min(beside,innerWidth-width-8))+'px';
  dialog.style.top=Math.max(8,Math.min(anchor.y,innerHeight-height-8))+'px';
}
function openShareMembers({title,scope,route,previewWritable,isRoot,anchor}){
  const dialog=$('share-members');
  if(dialog.open)closeShareMembers();
  const anchored=Number.isFinite(anchor?.x)&&Number.isFinite(anchor?.y);
  membersRequest={title,scope,route,previewWritable,isRoot,anchor:anchored?anchor:null};
  $('share-members-title').textContent=scope.owned?'成员与权限':'共享权限';
  $('share-members-leave').hidden=!(isRoot&&!scope.owned&&!previewWritable&&writer&&canLeaveShare());
  $('share-members-document').textContent=title;
  if(anchored){dialog.classList.add('anchored-members');dialog.show();positionShareMembers();}
  else dialog.showModal();
  void refreshMembers();
}
async function refreshMembers(){
  const request=membersRequest,cloud=transport,version=++membersVersion;
  const current=()=>$('share-members').open&&version===membersVersion&&cloud===transport;
  const status=$('share-members-status'),list=$('share-members-list'),button=$('share-members-refresh');
  list.replaceChildren();list.hidden=true;
  if(!request)return;
  const {scope,previewWritable}=request;
  button.disabled=true;
  status.textContent='正在读取成员与权限…';
  try{
    const members=demo?[
      {name:scope.owned?'':'项目发起人',self:scope.owned,owner:true,permission:'write',status:'accepted'},
      {name:scope.owned?'合成参与者':'',self:!scope.owned,permission:!scope.owned&&previewWritable?'write':'read',status:'accepted'},
    ]:await cloud?.readShareMembers?.(scope,account);
    if(!current())return;
    if(!members?.length){status.textContent='当前账号暂无法查看成员名单。';return;}
    for(const member of members){
      const item=document.createElement('li'),name=document.createElement('strong'),detail=document.createElement('small');
      name.textContent=member.self?(member.name?`${member.name}（你）`:'你'):member.name||(member.owner?'所有者':'参与者');
      detail.textContent=[member.owner?'所有者':'参与者',
        {write:'iCloud 可编辑',read:'iCloud 只读',unknown:'权限未知'}[member.permission],
        {accepted:'已加入',pending:'待接受',unknown:'状态未知'}[member.status]].join(' · ');
      item.append(name,detail);list.append(item);
    }
    list.hidden=false;
    status.textContent=demo?'合成共享成员':'';
  }catch(error){
    if(current())status.textContent=isExpiredSession(error)?'请重新登录 iCloud 后查看成员。':'暂时无法读取此共享的成员与权限。';
  }finally{if(current()){button.disabled=false;positionShareMembers();}}
}
$('share-members-refresh').onclick=()=>void refreshMembers();
$('share-members-leave').onclick=()=>{
  const request=membersRequest;
  if(!request?.isRoot||$('share-members-leave').hidden)return;
  closeShareMembers();
  confirmLeaveShares([request]);
};
$('share-members').addEventListener('close',()=>{if(!$('share-members').open)closeShareMembers();});
window.addEventListener('resize',positionShareMembers);
document.addEventListener('pointerdown',event=>{
  if(membersRequest?.anchor&&!$('share-members').contains(event.target))closeShareMembers();
},true);
document.addEventListener('keydown',event=>{
  if(event.key==='Escape'&&membersRequest?.anchor){
    event.preventDefault();event.stopPropagation();closeShareMembers();
  }
},true);
const leaveShareDialog=$('leave-share');
const leaveShareConfirm=$('leave-share-confirm');
const leaveShareCancel=$('leave-share-cancel');
const leaveShareStatus=$('leave-share-status');
function showLeaveShareItems({items,total}){
  $('leave-share-title').textContent=total===1?'离开共享？':'离开所选共享？';
  $('leave-share-detail').textContent=total===1?'退出后，你将无法访问这项共享。其他成员仍可继续使用。':
    '退出后，你将无法访问所选共享。其他成员仍可继续使用。';
  $('leave-share-item').textContent=total===1?items[0]?.title??'':'';
  const list=$('leave-share-items');
  list.replaceChildren();
  list.hidden=total===1;
  if(total>1)for(const {title} of items){
    const row=document.createElement('li');
    row.textContent=title;list.append(row);
  }
}
function confirmLeaveShares(items){
  if(leaveShareDialog.open||!canLeaveShare()||!writer||!items.length||
    !items.every(({route,isRoot})=>isRoot&&wasm.can_leave_share(route)))return;
  leaveShareRequest={items,total:items.length,cloud:transport,owner:account};
  showLeaveShareItems(leaveShareRequest);
  leaveShareConfirm.textContent='离开共享';
  leaveShareCancel.textContent='取消';
  leaveShareStatus.textContent='';
  leaveShareConfirm.disabled=false;
  leaveShareCancel.disabled=false;
  leaveShareDialog.showModal();
}
leaveShareDialog.addEventListener('close',()=>{if(!leaveShareDialog.open)leaveShareRequest=null;});
leaveShareDialog.addEventListener('cancel',event=>{if(leaveShareConfirm.disabled)event.preventDefault();});
leaveShareConfirm.onclick=async()=>{
  const request=leaveShareRequest;
  if(!request||leaveShareConfirm.disabled||!writer||!canLeaveShare())return;
  const {cloud,owner,total}=request;
  const current=()=>leaveShareRequest===request&&transport===cloud&&account===owner;
  leaveShareConfirm.disabled=true;
  leaveShareCancel.disabled=true;
  leaveShareStatus.textContent='正在离开共享…';
  try{
    while(sharedTask){await sharedTask.catch(()=>{});if(!current())return;}
    const work=(async()=>{
      let needsSignIn=false;
      for(const item of request.items.slice()){
        if(!current()||!canLeaveShare())return;
        try{
          if(!item.confirmed){
            if(!wasm.can_leave_share(item.route))throw new Error('共享访问已变更');
            if(demo)demoShares=JSON.stringify(JSON.parse(demoShares).filter(room=>sharedScopeKey(room)!==sharedScopeKey(item.scope)));
            else await cloud.leaveShare(item.scope,owner);
            if(!current())return;
            item.confirmed=true;
          }
          closeShareMembers();
          if(attachmentPreview.isShared())attachmentPreview.close();
          wasm.complete_leave_share(JSON.stringify(item.scope),total===1);
          request.items=request.items.filter(pending=>pending!==item);
          shownGuestScopes=null;
        }catch(error){
          if(!current())return;
          if(isExpiredSession(error)){needsSignIn=true;break;}
        }
      }
      if(!current())return;
      checkedAt=Date.now();
      if(!request.items.length){
        if(demo)wasm.set_shared_status(JSON.parse(demoShares).length?'演示文档 · 只读预览':'暂无共享文档');
        leaveShareDialog.close();
        return;
      }
      showLeaveShareItems(request);
      const failed=request.items.length;
      leaveShareStatus.textContent=(total>1?`已离开 ${total-failed} 项，${failed} 项尚未确认。`:'未能确认已离开共享。')+
        (needsSignIn?'请重新登录 iCloud 后再试。':'可以重试未确认的项目。');
      leaveShareConfirm.textContent='重试';
      if(total>failed)leaveShareCancel.textContent='关闭';
    })();
    const task=work.catch(()=>{});
    sharedTask=task;
    try{await work;}finally{if(sharedTask===task)sharedTask=null;}
    if(!demo&&!request.items.length&&transport===cloud&&account===owner)void refreshShared({validateOnly:true});
  }finally{
    if(current()){
      leaveShareConfirm.disabled=false;
      leaveShareCancel.disabled=false;
    }
  }
};
$('backup-check').addEventListener('close',()=>{
  if(!$('backup-check').open){backupVersion++;verifiedBackup=null;}
});
const libraryBackupDialog=$('library-backup-export');
const libraryBackupDownload=$('library-backup-export-download');
async function exportLibraryBackup(){
  if(account||!writer||libraryBackupDialog.open)return;
  const request={epoch};
  libraryBackup=request;
  const current=()=>libraryBackup===request&&request.epoch===epoch&&libraryBackupDialog.open;
  const status=$('library-backup-export-status');
  libraryBackupDownload.hidden=true;
  status.textContent='正在创建备份…';
  libraryBackupDialog.showModal();
  try{
    const {createLocalBackup}=await import('./backup-export.js');
    if(!current())return;
    const snapshot=JSON.parse(wasm.export_local_library_backup());
    const blob=await createLocalBackup(snapshot,wasm);
    if(!current())return;
    request.blob=blob;
    status.textContent='备份已准备好，可用于恢复飞记 Mac 的本地文档库。';
    libraryBackupDownload.hidden=false;
  }catch{
    if(current())status.textContent='无法创建文档库备份，请重试。';
  }
}
libraryBackupDialog.addEventListener('close',()=>{
  if(!libraryBackupDialog.open)libraryBackup=null;
});
libraryBackupDownload.onclick=()=>{
  if(!libraryBackup?.blob||libraryBackup.epoch!==epoch)return;
  const date=new Intl.DateTimeFormat('sv-SE',{dateStyle:'short',timeStyle:'short'}).format(new Date()).replaceAll(':','');
  download(`Fei Notes ${date}.feinotesbackup`,libraryBackup.blob,libraryBackup.blob.type);
};
$('backup-file').addEventListener('change',async event=>{
  const file=event.currentTarget.files[0];
  event.currentTarget.value='';
  if(!file)return;
  const dialog=$('backup-check'),summary=$('backup-summary'),result=$('backup-result'),version=++backupVersion;
  const current=()=>dialog.open&&version===backupVersion;
  verifiedBackup=null;
  $('open-backup-preview').hidden=true;
  $('open-backup-preview').disabled=false;
  $('backup-name').textContent=file.name;
  summary.replaceChildren();summary.hidden=true;
  result.textContent='正在校验备份文件…';
  if(!dialog.open)dialog.showModal();
  try{
    const value=await inspectMacBackup(file,wasm,current);
    if(!current())return;
    const rows=[['文档库',value.kind],['文件',value.files],
      ['附件文件',value.assets],['历史版本文件',value.history],['共享文档库',value.shared]];
    for(const [term,value] of rows){
      const label=document.createElement('dt'),detail=document.createElement('dd');
      label.textContent=term;detail.textContent=value;summary.append(label,detail);
    }
    summary.hidden=false;
    result.textContent='备份文件及文件清单校验通过。';
    verifiedBackup={file,archive:value,current};
    $('open-backup-preview').hidden=false;
  }catch(error){
    if(current())result.textContent=error instanceof BackupValidationError?error.message:'无法校验此备份，请重试。';
  }
});

const backupPreview=$('backup-preview'),restoreButton=$('restore-backup');
const backupRestore=$('backup-restore'),restoreConfirm=$('backup-restore-confirm');
const restoreOpen=$('backup-restore-open'),restoreCancel=$('backup-restore-cancel');
const restoreStatus=$('backup-restore-status');
let previewRestore,restoreSaving=false;

function libraryAddress(id){
  const address=new URL('.',location.href);
  if(demo)address.searchParams.set('demo','1');
  if(ownedDemo)address.searchParams.set('owned-preview','1');
  if(sharedEditPreview)address.searchParams.set('shared-edit-preview','1');
  if(sharedConflictPreview)address.searchParams.set('share-conflict-preview','1');
  if(id)address.searchParams.set('local-library',id);
  return address.href;
}

async function openLibraryList(){
  const dialog=$('browser-libraries'),list=$('browser-library-list'),status=$('browser-library-status');
  if(dialog.open)return;
  list.replaceChildren();
  status.textContent='';
  dialog.showModal();
  const add=(label,id)=>{
    const row=document.createElement('li');
    if(id===restoredId){
      row.textContent=label+' · 当前';
    }else{
      const link=document.createElement('a');
      link.textContent=label;
      link.href=libraryAddress(id);
      link.target='_blank';
      link.rel='noopener';
      row.append(link);
    }
    list.append(row);
  };
  add('主文档库',null);
  try{
    const libraries=await listRestoredLibraries(db,restoredPrefix);
    if(!dialog.open)return;
    libraries.sort((a,b)=>b.created-a.created);
    for(const library of libraries){
      if(libraryIdPattern.test(library?.id))add(library.title,library.id);
    }
  }catch{if(dialog.open)status.textContent='无法读取此浏览器的文档库列表。';}
}

backupPreview.addEventListener('close',()=>{
  if(!backupPreview.open){
    backupPreview.querySelector('iframe')?.remove();
    previewRestore=null;
  }
});
$('open-backup-preview').onclick=async event=>{
  const selection=verifiedBackup,button=event.currentTarget;
  if(!selection?.current()||button.disabled)return;
  const result=$('backup-result');
  button.disabled=true;
  result.textContent='正在读取备份文档库…';
  try{
    const {readBackupPreview}=await import('./backup-preview.js');
    const data=await readBackupPreview(selection.file,selection.archive,selection.current);
    if(!selection.current())return;
    const input=JSON.stringify(data);
    const frame=document.createElement('iframe'),status=$('backup-preview-status');
    frame.title='备份的个人文档库';
    status.hidden=true;
    restoreButton.hidden=true;
    previewRestore=null;
    let state;
    if(selection.archive.localOnly&&selection.archive.shared===0){
      try{
        state=wasm.prepare_mac_backup_restore(input);
        const name=selection.file.name.replace(/\.feinotesbackup$/i,'').trim()||'恢复的文档库';
        const summary={documents:0,folders:0,trash:0};
        for(const entry of data.source.entries){
          if(entry.trash)summary.trash++;
          else if(typeof entry.meta.folder==='string')summary.folders++;
          else if(entry.document)summary.documents++;
        }
        previewRestore={state,preferences:data.source.preferences,title:name,summary};
        restoreButton.hidden=false;
      }catch(error){
        status.hidden=false;
        status.textContent='无法恢复为可编辑文档库。'+(typeof error==='string'?error:'');
      }
    }else{
      status.hidden=false;
      status.textContent='此备份可预览个人文档。iCloud 或共享库暂不支持可编辑恢复。';
    }
    state??=wasm.prepare_mac_backup_preview(input);
    frame.addEventListener('load',async()=>{
      if(!backupPreview.open||!frame.isConnected)return;
      try { await frame.contentWindow.openBackupPreview(state); }
      catch { if(frame.isConnected){frame.remove();status.hidden=false;status.textContent='无法打开备份的只读工作区。';} }
    },{once:true});
    frame.src='./backup-view.html';
    backupPreview.append(frame);
    backupPreview.showModal();
    result.textContent='备份文件及文件清单校验通过。';
  }catch{if(selection.current())result.textContent='无法预览此备份，请重试。';}
  finally{if(selection.current())button.disabled=false;}
};
function showRestoreState(item){
  restoreConfirm.hidden=!!item.id;
  restoreOpen.hidden=!item.id;
  restoreStatus.textContent=item.id?'文档库已保存。将在新标签页打开。':'';
  if(item.id)restoreOpen.href=libraryAddress(item.id);
  else restoreOpen.removeAttribute('href');
}
restoreButton.onclick=()=>{
  if(!previewRestore||!backupPreview.open||backupRestore.open)return;
  const summary=$('backup-restore-summary');
  summary.replaceChildren();
  const {title,summary:{documents,folders,trash}}=previewRestore;
  for(const [label,value] of [['文档库',title],['文档',documents],['文件夹',folders],['回收站项目',trash]]){
    const term=document.createElement('dt'),detail=document.createElement('dd');
    term.textContent=label;detail.textContent=value;summary.append(term,detail);
  }
  showRestoreState(previewRestore);
  backupRestore.showModal();
};
backupRestore.addEventListener('cancel',event=>{if(restoreSaving)event.preventDefault();});
restoreConfirm.onclick=async()=>{
  const item=previewRestore;
  if(!item||item.id||restoreSaving)return;
  restoreSaving=true;
  restoreConfirm.disabled=restoreCancel.disabled=true;
  restoreStatus.textContent='正在保存恢复的文档库…';
  try{
    const id=crypto.randomUUID(),details={id,title:item.title,created:Date.now(),preferences:item.preferences};
    await saveRestoredLibrary(db,restoredPrefix+id,item.state,details);
    item.id=id;
    showRestoreState(item);
  }catch{restoreStatus.textContent='无法保存恢复的文档库，请检查浏览器的可用存储空间后重试。';}
  finally{restoreSaving=false;restoreConfirm.disabled=restoreCancel.disabled=false;}
};
function selectImport(data){
  if(!canEdit()){message('当前文档库为只读，无法导入。');return;}
  if(importing)return;
  const {kind,parent}=JSON.parse(data);
  const directory=kind==='folder'||kind==='quiet-library';
  const input=$(directory?'import-folder':'import-files');
  input.value='';
  if(!directory)input.accept=kind==='xml'?'.xml':kind==='markdown'?'.md,.mdown,.markdown,.mark,.txt':'.md,.mdown,.markdown,.mark,.txt,.xml';
  importRequest={parent,directory,epoch,input,kind};
  message('');
  try{input.showPicker();}
  catch(error){
    if(error.name==='NotAllowedError'){input.click();return;}
    importRequest=null;
    message('无法打开文件选择窗口，请重试。');
  }
}
async function migrateQuietLibrary(files,request){
  const available=new Map();
  for(const file of files){
    const relative=file.webkitRelativePath.split('/').slice(1).join('/');
    if(relative)available.set(relative,file);
  }
  const folders=available.has('.quiet/format.json');
  if(!folders&&(!available.has('format.json')||!available.has('index.json'))){
    message('请选择包含 Quiet.md 文档库索引的文件夹。');
    return;
  }
  const selected=[...available].filter(([path])=>{
    if(folders){
      if(path==='.quiet/format.json'||path==='.quiet/index.json'||path.startsWith('.quiet/trash/'))return true;
      if(path.startsWith('.assets/'))return true;
      return path.split('/').every(part=>!part.startsWith('.'));
    }
    return path==='format.json'||path==='index.json'||/^(docs|trash|assets)\//.test(path);
  });
  if(selected.length>10_000||selected.reduce((size,[,file])=>size+file.size,0)>64*1024*1024){
    message('此版本最多迁移 64 MB 或 10,000 个文件。');
    return;
  }
  importing=true;
  message('正在迁移 Quiet.md 文档库…');
  try{
    const staged=Object.create(null);
    for(const [path,file] of selected){
      staged[path]=bytesB64(new Uint8Array(await file.arrayBuffer()));
      if(epoch!==request.epoch||!canEdit())return;
    }
    await run(async()=>{
      if(epoch!==request.epoch||!canEdit())return;
      let count;
      try{count=wasm.import_quiet_library(JSON.stringify(staged));}
      catch(error){
        const detail=String(error);
        if(detail.includes('composition-pending'))message('请先完成当前的文字输入，再迁移。');
        else if(detail.includes('当前文档库已有同一份文档'))message('当前文档库已有同一份文档，本次没有迁入任何内容。');
        else if(detail.includes('缺少文件的附件')||detail.includes('缺少内容的文档'))message('源文档库缺少文件，本次没有迁入任何内容。');
        else message('无法完整读取所选的 Quiet.md 文档库，本次没有迁入任何内容。');
        return;
      }
      if(count){await persist();if(account)void sync();}
      if(epoch===request.epoch)message(count?'已迁移 '+count+' 项，包括回收站中的内容。':'这个 Quiet.md 文档库没有需要迁移的内容。');
    });
  }catch(error){
    if(!error?.localSaveFailed&&epoch===request.epoch)message('无法读取所选文件夹，请重试。');
  }finally{importing=false;}
}
async function importSelection(event){
  const input=event.currentTarget;
  const request=importRequest;
  importRequest=null;
  if(importing||request?.input!==input||request.epoch!==epoch||!canEdit())return;
  if(request.kind==='quiet-library'){
    const files=Array.from(input.files);
    input.value='';
    await migrateQuietLibrary(files,request);
    return;
  }
  const available=new Map();
  for(const file of input.files){
    const path=request.directory?file.webkitRelativePath:file.name;
    if(!path)continue;
    if(!request.directory||!path.split('/').slice(1).some(segment=>segment.startsWith('.')||['node_modules','target'].includes(segment))){
      available.set(path,file);
    }
  }
  input.value='';
  await importDocuments(available,request);
}
async function importDocuments(available,request){
  if(!canEdit()||request.epoch!==epoch)return;
  const documents=[...available].filter(([,file])=>(request.kind!=='xml'&&markdownExt.test(file.name))||(request.kind!=='markdown'&&xmlExt.test(file.name)));
  if(!documents.length){message('请选择 Markdown、文本或 DocxXML 文档。');return;}
  if(documents.length>2000){message('一次最多导入 2,000 份文档。');return;}
  importing=true;
  let failed=0,budget=64*1024*1024;
  message('正在导入文档…');
  try{
    const selected={directory:request.directory,documents:[]};
    const text=new TextDecoder('utf-8',{fatal:true});
    for(const [path,file] of documents){
      if(file.size>32*1024*1024||file.size>budget){failed++;continue;}
      try{
        selected.documents.push({path,text:text.decode(await file.arrayBuffer())});
        budget-=file.size;
      }catch{failed++;}
      if(epoch!==request.epoch)return;
    }
    const selection=JSON.stringify(selected);
    const paths=JSON.parse(wasm.import_attachment_paths(selection));
    const assets=Object.create(null);
    for(const path of paths){
      const file=available.get(path);
      if(!file||!file.size||file.size>budget)continue;
      try{
        assets[path]=bytesB64(new Uint8Array(await file.arrayBuffer()));
        budget-=file.size;
      }catch{}
      if(epoch!==request.epoch)return;
    }
    await run(async()=>{
      if(epoch!==request.epoch||!canEdit())return;
      const imported=JSON.parse(wasm.import_files(selection,JSON.stringify(assets),request.parent??undefined,request.open));
      if(imported.opened){
        await persist();
        if(epoch!==request.epoch)return;
      }
      failed+=imported.failed;
      let summary=imported.count?'已导入 '+imported.count+' 份文档。':'导入未完成。';
      if(failed)summary+=' '+failed+' 份文档格式无效、超出容量或无法读取。';
      if(imported.missing)summary+=' '+imported.missing+' 个附件引用未找到对应文件。';
      message(summary);
    });
  }catch(error){
    if(epoch===request.epoch&&!error?.localSaveFailed)message(String(error).includes('composition-pending')?'请先完成当前的文字输入，再导入。':'导入未完成，请检查文件和目标文件夹后重试。');
  }finally{importing=false;}
}
for(const input of [$('import-files'),$('import-folder')]){
  input.addEventListener('change',importSelection);
  input.addEventListener('cancel',()=>{if(importRequest?.input===input)importRequest=null;});
}
const dropDialog=$('drop-import');
let pendingDrop;
function dropDestination(event){
  if(!ready||!canEdit()||importing||document.querySelector('dialog[open]'))return null;
  const destination=wasm.document_drop_destination(event.offsetX,event.offsetY);
  return destination?JSON.parse(destination):null;
}
async function collectDroppedFiles(entries,request){
  const available=new Map();
  async function visit(entry,path){
    if(request.epoch!==epoch||!canEdit())return;
    if(entry.isDirectory){
      const reader=entry.createReader();
      while(request.epoch===epoch&&canEdit()){
        const children=await new Promise((resolve,reject)=>reader.readEntries(resolve,reject));
        if(!children.length)break;
        for(const child of children){
          if(child.name.startsWith('.')||['node_modules','target'].includes(child.name))continue;
          await visit(child,path+'/'+child.name);
        }
      }
      return;
    }
    const file=entry instanceof File?entry:await new Promise((resolve,reject)=>entry.file(resolve,reject));
    if(available.has(path))throw new Error('duplicate-drop-path');
    available.set(path,file);
  }
  for(const entry of entries)await visit(entry,entry.name);
  return available;
}
async function importDrop(entries,request){
  if(importing||!canEdit()||request.epoch!==epoch)return;
  importing=true;
  message('正在读取拖入的文件…');
  try{
    await importDocuments(await collectDroppedFiles(entries,request),request);
  }catch(error){
    if(request.epoch===epoch)message(error.message==='duplicate-drop-path'?
      '拖入的文件中有重复路径，请分开导入。':'无法读取拖入的文件，请重试。');
  }finally{importing=false;}
}
function canvasFileDrop(event){
  return event.target instanceof HTMLCanvasElement&&event.dataTransfer?.types.includes('Files');
}
document.addEventListener('dragover',event=>{
  if(!canvasFileDrop(event))return;
  event.preventDefault();
  try{event.dataTransfer.dropEffect=dropDestination(event)?'copy':'none';}
  catch{event.dataTransfer.dropEffect='none';}
});
document.addEventListener('drop',event=>{
  if(!canvasFileDrop(event))return;
  event.preventDefault();
  let destination;
  try{destination=dropDestination(event);}catch{return;}
  if(!destination)return;
  // Browser file entries can only be captured while the drop event is running.
  const transfer=event.dataTransfer;
  const entries=Array.from(transfer.items).filter(item=>item.kind==='file')
    .map(item=>item.webkitGetAsEntry?.()??item.getAsFile()).filter(Boolean);
  const sources=entries.length?entries:Array.from(transfer.files);
  if(!sources.length)return;
  const request={parent:destination.parent,kind:'documents',directory:sources.some(entry=>entry.isDirectory),epoch,open:false};
  if(destination.confirm){
    pendingDrop={sources,request};
    $('drop-import-destination').textContent='将拖入的文档导入「'+destination.title+'」。';
    dropDialog.showModal();
  }else void importDrop(sources,request);
});
dropDialog.addEventListener('close',()=>{pendingDrop=null;});
$('drop-import-confirm').addEventListener('click',()=>{
  const selection=pendingDrop;
  dropDialog.close();
  if(selection)void importDrop(selection.sources,selection.request);
});
const boardPictures=$('board-pictures');
boardPictures.addEventListener('cancel',()=>{boardPictureRequest=null;});
boardPictures.addEventListener('change',async()=>{
  const request=boardPictureRequest;
  boardPictureRequest=null;
  if(!request||request.epoch!==epoch||!canEdit())return;
  const selection=Array.from(boardPictures.files);
  if(selection.length>16||selection.reduce((total,file)=>total+file.size,0)>32*1024*1024||
    selection.some(file=>file.size>20*1024*1024)){
    message('最多可选 16 张图片，单张不超过 20 MB，总计不超过 32 MB。');return;
  }
  try{
    const pictures=[];
    for(const file of selection){
      const dot=file.name.lastIndexOf('.');
      const ext=dot<0?'':file.name.slice(dot+1).toLowerCase();
      if(!['jpg','jpeg','png','gif','webp','bmp'].includes(ext))continue;
      pictures.push({name:file.name.slice(0,dot),ext,bytes:bytesB64(new Uint8Array(await file.arrayBuffer()))});
      if(epoch!==request.epoch)return;
    }
    if(!pictures.length){message('请选择支持的图片文件。');return;}
    if(epoch===request.epoch&&canEdit())wasm.insert_board_pictures(request.token,JSON.stringify(pictures));
  }catch{if(epoch===request.epoch)message('无法插入图片，请检查文件后重试。');}
});
const attachmentFiles=$('attachment-files');
attachmentFiles.addEventListener('cancel',()=>{attachmentRequest=null;});
attachmentFiles.addEventListener('change',async()=>{
  const request=attachmentRequest;
  attachmentRequest=null;
  if(!request||request.epoch!==epoch||!canEdit())return;
  const selection=Array.from(attachmentFiles.files);
  if(!selection.length)return;
  if(selection.length>16||selection.reduce((total,file)=>total+file.size,0)>64*1024*1024){
    message('一次最多选择 16 个附件，合计不超过 64 MB。');
    return;
  }
  let files;
  try{
    files=[];
    for(const file of selection){
      const bytes=bytesB64(new Uint8Array(await file.arrayBuffer()));
      if(request.epoch!==epoch)return;
      files.push({name:file.name,bytes});
    }
  }catch{if(request.epoch===epoch)message('无法读取选中的附件。');return;}
  await run(async()=>{
    if(request.epoch!==epoch||!canEdit())return;
    if(!wasm.insert_attachments(request.token,JSON.stringify(files))){
      message('当前文档或插入位置已变化，请重新选择文件。');
      return;
    }
    await persist();
    if(account)void sync();
  });
});

function applyAppearance(preferences){
  try{
    const {appearance}=JSON.parse(preferences||'{}');
    document.documentElement.style.colorScheme=appearance==='dark'||appearance==='light'?appearance:'';
  }catch{}
}
let tabItemTitle='', restoredLibraryTitle='';
function updateTabTitle(){
  const app=document.documentElement.lang==='en'?'Fei Notes':'飞记';
  const title=tabItemTitle||restoredLibraryTitle;
  document.title=title?title+' · '+app:app;
}
function applyLanguage(language){
  const lang=language==='zh'?'zh-CN':language;
  if(document.documentElement.lang===lang)return;
  document.documentElement.lang=lang;
  updateTabTitle();
  for(const [id,label] of Object.entries(cloudLabels)){
    if(id!=='apple-sign-in-button'||!authorization)$(id).textContent=cloudText(label);
  }
  authorization?.refreshLabel();
  status(saveStatusText,saveStatusPhase);
  $('cloud-status').textContent=cloudText(navigator.onLine?connectionText:'当前离线，请联网后重新连接 iCloud。');
  updateConnectionState();
}
function printDocument(html){
  closePrintFrame?.();
  const frame=document.createElement('iframe');
  frame.setAttribute('sandbox','allow-same-origin allow-modals');
  frame.setAttribute('aria-hidden','true');
  frame.tabIndex=-1;
  Object.assign(frame.style,{position:'fixed',top:'0',left:'-9999px',width:'668px',height:'1000px',border:'0'});
  const errorText=()=>document.documentElement.lang.startsWith('en')?'Unable to prepare the document for printing.':'无法准备打印，请重试。';
  const close=()=>{
    clearTimeout(timeout);
    frame.remove();
    if(closePrintFrame===close)closePrintFrame=undefined;
  };
  const timeout=setTimeout(()=>{close();message(errorText());},20000);
  closePrintFrame=close;
  frame.addEventListener('load',async()=>{
    try{
      const page=frame.contentDocument, target=frame.contentWindow;
      if(!page||!target||!page.querySelector('h1.title'))throw new Error('Missing print document');
      await page.fonts.ready;
      if(closePrintFrame!==close)return;
      clearTimeout(timeout);
      target.addEventListener('afterprint',close,{once:true});
      target.print();
    }catch{
      if(closePrintFrame===close){close();message(errorText());}
    }
  },{once:true});
  frame.srcdoc=html;
  document.body.append(frame);
}

window.addEventListener('feinotes',({detail:{type,data}})=>{
  if(type==='ready'){
    ready=true;applyLanguage(data);
    if(account)status('iCloud 离线缓存','offline');
    else status('已保存到此浏览器');
    updateConnectionState();
    const loading=$('loading'),restoring=restoreCloud();
    if(restoring&&loading){
      $('loading-label').textContent=cloudText('正在打开文档库…');
      void restoring.finally(()=>loading.remove());
    }else loading?.remove();
    if(pendingObject)queueMicrotask(openPendingObject);
  }
  else if(type==='tab-item-title'){tabItemTitle=data;updateTabTitle();}
  else if(type==='changed'&&canEdit())scheduleSave();
  else if(type==='trash-opened'&&canEdit())queueMicrotask(()=>{
    if(account){void sync();return;}
    const revision=epoch;
    void run(async()=>{
      if(revision===epoch&&!account&&!document.hidden&&wasm.expire_trash(false))await persist();
    });
  });
  else if(type==='state'&&writer)void run(persist);
  else if(type==='trash-purged'&&canEdit()){
    const revision=epoch;
    markUnsaved();
    void run(async()=>{
      if(revision!==epoch||!await persist()||revision!==epoch)return;
      wasm.confirm_trash_saved(data);
      if(account)void sync();
    });
  }
  else if(type==='account')queueMicrotask(openAccount);
  else if(type==='switch-local')queueMicrotask(()=>confirmSwitchToLocal(data));
  else if(type==='invitation')queueMicrotask(showInvitation);
  else if(type==='document-body'||type==='document-opened'||type==='document-export-body'||type==='document-print-body'){
    const target=JSON.parse(data);
    const action={'document-body':'open','document-export-body':'export','document-print-body':'print'}[type]??'refresh';
    if(type!=='document-opened'||openingBody!==target.id)
      queueMicrotask(()=>void loadDocumentBody(target,action));
  }
  else if(type==='shared'||type==='shared-request')queueMicrotask(()=>{
    sharedWanted=true;
    const prompt=type==='shared-request';
    void refreshShared({prompt,validateOnly:!prompt&&checkedAt!==0});
  });
  else if(type==='cancel-shared-return')sharedReturn=null;
  else if(type==='shared-reload-ready'){
    if(data==='1'&&account&&reloadDestination?.key===key)sharedReturn=reloadDestination;
    reloadDestination=null;
  }
  else if(type==='sync')queueMicrotask(syncNow);
  else if(type==='check-trial')queueMicrotask(()=>void refreshTrial());
  else if(type==='recovery')queueMicrotask(openRecovery);
  else if(type==='copy-object-link')copyObjectLink(JSON.parse(data));
  else if(type==='open-object-tab')openObjectTab(JSON.parse(data));
  else if(type==='object-link-unavailable')message('暂时无法确认此对象的共享范围，请同步后重试。');
  else if(type==='share-members')queueMicrotask(()=>openShareMembers(JSON.parse(data)));
  else if(type==='leave-shares')queueMicrotask(()=>confirmLeaveShares(JSON.parse(data)));
  else if(type==='check-backup')$('backup-file').click();
  else if(type==='export-backup')queueMicrotask(exportLibraryBackup);
  else if(type==='list-libraries')queueMicrotask(openLibraryList);
  else if(type==='import')selectImport(data);
  else if(type==='board-pictures'&&canEdit()){
    boardPictureRequest={token:data,epoch};
    boardPictures.value='';
    boardPictures.click();
  }
  else if(type==='pick-attachments'&&canEdit()){
    attachmentRequest={token:data,epoch};
    attachmentFiles.value='';
    attachmentFiles.click();
  }
  else if(type==='preview-attachment')attachmentPreview.show(JSON.parse(data));
  else if(type==='request-cloud-attachment')queueMicrotask(()=>void requestCloudAttachment(JSON.parse(data)));
  else if(type==='print-document')printDocument(data);
  else if(type==='close-print-document')closePrintFrame?.();
  else if(type==='board-clipboard-feedback'){
    if(data || $('message').textContent===boardClipboardMessage)message(data);
    boardClipboardMessage=data;
  }
  else if(type==='download'){const {name,content,mime}=JSON.parse(data);download(name,content,mime);}
  else if(type==='download-binary'){const {name,bytes,mime}=data;download(name,bytes,mime);}
  else if(type==='download-attachment'){
    const {name,content}=JSON.parse(data);
    download(name,b64Bytes(content),'application/octet-stream');
  }
  else if(type==='preferences'){
    applyAppearance(data);
    const {language,english}=JSON.parse(data);
    applyLanguage(language||(english?'en':'zh'));
    try{localStorage.setItem(preferencesKey,data);}catch{}
  }
});
async function switchToLocal(){
  if(sessionVault){
    authGeneration++;
    sessionVault.clear(cloudSessionId);
    cloudSessionId=undefined;
  }
  await changeAccount(null);
  requireSignIn('已退出 iCloud。',false);
  if(!cloudReadOnly)message('原账号的待同步修改保留在此浏览器。');
}
$('signout').onclick=async()=>{
  try{
    await switchToLocal();
    if($('settings').open)openAccount();
  }catch(e){fail(e);}
};
const switchLocalDialog=$('switch-local'),switchLocalConfirm=$('switch-local-confirm');
const switchLocalText=(zh,en)=>switchLocalDialog.lang==='en'?en:zh;
function confirmSwitchToLocal(language){
  if(!account||restoredWorkspace||switchLocalDialog.open)return;
  switchLocalDialog.lang=language==='en'?'en':'zh-CN';
  $('switch-local-title').textContent=switchLocalText('关闭 iCloud 同步？','Turn off iCloud sync?');
  $('switch-local-detail').textContent=cloudReadOnly?switchLocalText(
    '此浏览器的其他飞记标签页也会退出。你的 iCloud 文档仍可在飞记 App 中查看。',
    'Other Fei Notes tabs in this browser will also be signed out. Your iCloud documents are still available in the Fei Notes app.'):
    switchLocalText('飞记将打开此浏览器的本机文档库。iCloud 文档和待同步的修改仍保留在当前账号的文档库里。',
    'Fei Notes will open your local library in this browser. Your iCloud documents and pending changes will stay in the current account’s library.');
  $('switch-local-cancel').textContent=switchLocalText('取消','Cancel');
  switchLocalConfirm.textContent=switchLocalText('切换到本机','Switch to Local');
  $('switch-local-status').textContent='';
  switchLocalDialog.showModal();
}
switchLocalDialog.addEventListener('cancel',event=>{if(switchLocalConfirm.disabled)event.preventDefault();});
switchLocalConfirm.onclick=async()=>{
  if(switchLocalConfirm.disabled||!account||!switchLocalDialog.open)return;
  const cancel=$('switch-local-cancel'),notice=$('switch-local-status');
  switchLocalConfirm.disabled=cancel.disabled=true;
  notice.textContent=switchLocalText('正在打开本机文档库…','Opening the local library…');
  try{
    await switchToLocal();
    switchLocalDialog.close();
  }catch{
    notice.textContent=switchLocalText('无法切换文档库，请检查本地保存状态后重试。',
      'Couldn’t switch libraries. Check that your changes are saved, then try again.');
  }finally{switchLocalConfirm.disabled=cancel.disabled=false;}
};
$('sync').onclick=syncNow;
$('download-recovery').onclick=()=>download('飞记恢复副本.xml',$('recovery-content').value,'application/xml;charset=utf-8');
function restoreCloud(){
  if(!transport&&navigator.onLine&&sessionVault?.read())
    return setupCloud({restoreOnly:true}).catch(e=>showConnection(e?.userMessage||'暂时无法恢复 iCloud 会话。'));
}
window.addEventListener('storage',event=>{
  if(sessionVault&&cloudSessionId&&(event.key===cloudSessionKey||event.key===null)&&
    sessionVault.read()?.clientId!==cloudSessionId)
    requireSignIn('此浏览器的 iCloud 会话已结束，请重新连接。',false);
});
window.addEventListener('online',()=>{
  if(!account&&!dirty&&saveStatusPhase==='offline')status('已保存到此浏览器');
  if($('settings').open)openAccount();else updateConnectionState();
  restoreCloud();
  if(transport)void refreshTrial();
  void sync({afterCurrent:true});if(sharedWanted||transport)void refreshShared();
});
window.addEventListener('offline',()=>{
  networkRevision++;
  cloudSyncPhase='';
  if(ready)wasm.set_cloud_trial('unavailable');
  if($('settings').open)openAccount();else updateConnectionState();
  if(!dirty&&saveStatusPhase!=='failed')status(account?'离线待同步':'离线 · 已保存到此浏览器','offline');
  resetShared(true);
});
function checkVisible(){
  if(document.hidden)return;
  void sync();
  if(transport&&checkedAt&&Date.now()-checkedAt>=30000)void refreshShared({validateOnly:true});
}
document.addEventListener('visibilitychange',()=>{
  if(document.hidden){
    if(hasUnsavedChanges())void run(persist);
  }else checkVisible();
});
window.addEventListener('beforeunload',event=>{
  if(!hasUnsavedChanges())return;
  void run(persist);
  event.preventDefault();
  event.returnValue='';
});
window.addEventListener('pagehide',rememberSharedOnReload);
setInterval(checkVisible,15000);
if(presencePreview){
  document.addEventListener('visibilitychange',pollPresence);
  window.addEventListener('focus',pollPresence);
  window.addEventListener('blur',()=>{void publishPresence(null,null);});
  window.addEventListener('pagehide',clearPresence);
  setInterval(pollPresence,2000);
}
try{
  db=await openDatabase();
  let recovery;
  if(restoredWorkspace){
    if(!libraryIdPattern.test(restoredId))throw new Error('恢复的文档库地址无效。');
    key=restoredPrefix+restoredId;
    recovery=await readLibrary(db,'metadata:'+key);
    if(!recovery||recovery.id!==restoredId)throw new Error('此浏览器中找不到恢复的文档库。');
    restoredLibraryTitle=recovery.title;
    updateTabTitle();
  }else{
    const last=demo?'':await readLibrary(db,lastLibraryKey);
    if(last){
      const previous=JSON.parse(last);
      if(!cloudReadOnly||!previous.account){key=previous.key;account=previous.account;}
    }
  }
  useLock(await acquire(key));
  if(!writer)message('文档库已在另一个标签页打开。此标签页可以查看文档。');
  let state=await readLibrary(db,key);
  if(recovery&&!state)throw new Error('恢复的文档库数据不可用。');
  let preferences=recovery?.preferences||'';
  try{preferences=localStorage.getItem(preferencesKey)||preferences;}catch{}
  applyAppearance(preferences);
  await loadWorkspaceEngine();
  const ownedFixture=ownedDemo&&!recovery?JSON.parse(wasm.demo_owned_shares(sharedEditPreview,sharedConflictPreview)):null;
  demoShares=demo&&!recovery?(ownedFixture?JSON.stringify(ownedFixture.rooms):wasm.demo_shares()):undefined;
  if(sharedEditPreview&&!recovery){
    const fixtureKey=key+'-source';
    const source=await readLibrary(db,fixtureKey);
    if(source)demoShares=source;
    else if(writer)await writeLibrary(db,fixtureKey,demoShares);
  }
  if(demo&&!state){
    state=ownedFixture?.library??wasm.demo_library();
    if(writer)await writeLibrary(db,key,state);
  }
  if(demo)checkedAt=Date.now();
  reloadDestination=pendingObject?null:sharedReloadTarget();
  wasm.start(state,canEdit(),preferences,invitationPreview&&!recovery,demoShares,sharedEditPreview&&!recovery,recovery?.title,!!reloadDestination);
  if(account){
    lastSuccessfulSync=await readSyncTime(key);
    showConnection('请登录 Apple 账号恢复 iCloud 同步。');
  }
  if('serviceWorker'in navigator)navigator.serviceWorker.register('./sw.js').catch(console.warn);
  if(incomingInvitation){
    showInvitation(incomingInvitation);
    void inspectInvitation();
  }
}catch(e){
  if($('loading')){
    $('loading').dataset.error='';
    $('loading-label').textContent=cloudText('无法打开文档库，请刷新后重试。');
  }else fail(e);
}
