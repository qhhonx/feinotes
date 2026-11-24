import {b64Bytes,bytesB64} from './storage.js';
import {createPresenceReader} from './shared-presence.js';
import {commitSharedEdits,retentionFence} from './shared-write.js';
let loadingSDK;
export function loadCloudKit() {
  if(window.CloudKit)return Promise.resolve(window.CloudKit);
  if(loadingSDK)return loadingSDK;
  loadingSDK=new Promise((resolve,reject)=>{
    const script=document.createElement('script'); script.src='https://cdn.apple-cloudkit.com/ck/2/cloudkit.js';
    const failed=()=>{clearTimeout(timer);script.remove();reject(new Error('无法加载 Apple CloudKit，请检查网络后重试'));};
    const timer=setTimeout(failed,15000);
    script.onload=()=>{clearTimeout(timer);if(window.CloudKit)resolve(window.CloudKit);else failed();};
    script.onerror=failed;
    document.head.append(script);
  }).finally(()=>{loadingSDK=null;});
  return loadingSDK;
}
function check(response) {
  if(response.hasErrors||response.errors?.length)throw response.errors?.[0]??new Error('CloudKit 未返回错误详情');
  if(response.serverErrorCode)throw response;
  return response;
}
const errorCode=error=>error?.ckErrorCode||error?.serverErrorCode||error?.code;
export function isProductionLibrary(config){
  return config?.containerIdentifier==='iCloud.app.feinotes.mac'&&
    config.environment==='production'&&config.zoneName==='QuietLibraryEdits-v4'&&
    typeof config.apiToken==='string'&&/^[A-Za-z0-9_-]+$/.test(config.apiToken);
}
// Each transport keeps its own auth session. CloudKit uploads assets before
// saving the record; changing the UI account must not retarget that later save.
export function bindAccountContainer(container,CloudKit=window.CloudKit){
  const config=container.getConfig(),auth=config.apiTokenAuth;
  if(!auth?.ckWebAuthToken)throw new Error('Apple 登录会话已失效，请重新登录');
  // Keep the original UI container and its sign-in listeners alive. Only this
  // private transport uses the newly configured, non-persisted session.
  CloudKit.configure({locale:'zh-cn',containers:[{...config,apiTokenAuth:{...auth,persist:false}}]});
  return CloudKit.getDefaultContainer();
}
export function createPrivateTransport(config,CloudKit,session,clientId,guard,{checkShared=false,readShared=false,previewInvitations=false,allowInvitationAcceptance=false,previewLeaveShares=false,previewPresence=false,previewPresenceWrites=false,previewSharedWrites=false,lazyBodies=false,readOnly=false,sessionRequest}={}){
  if(!readOnly||!isProductionLibrary(config))throw new Error('此 Web 版本仅支持读取飞记正式 iCloud 文档库');
  const origin='https://api.apple-cloudkit.com';
  const root=`/database/1/${encodeURIComponent(config.containerIdentifier)}/${config.environment}/`;
  const sharedReading=readShared;
  const invitations=previewInvitations&&sharedReading;
  const leaving=!readOnly&&previewLeaveShares&&sharedReading;
  const sharedWriting=!readOnly&&previewSharedWrites&&sharedReading;
  const sharedDiscovery=checkShared||sharedReading;
  let allowedSharedZones=new Set();
  let ownedSharedZones=new Map();
  let invitationRequest;
  let leaveRequest;
  let presenceRequest;
  let contentWrite;
  const leftSharedZones=new Set();
  const shareCursors=new Map();
  const shareBodies=new Map();
  const shareTitles=new Map();
  const sharePermissions=new Map();
  // Keep recently opened bodies available without keeping every visited share
  // in memory. The workspace retains its own pending edits independently.
  function rememberBodies(key, bodies, requested){
    const preserve=new Set([catalogueScope,...requested]);
    const activeBytes=(entries)=>Array.from(entries.values()).reduce((n,entry)=>n+entry.edit.data.length,0);
    let bytes=activeBytes(bodies);
    if(bytes>24*1024*1024){
      for(const [id,entry] of bodies){
        if(preserve.has(entry.edit.note))continue;
        bodies.delete(id);
        bytes-=entry.edit.data.length;
        if(bytes<=24*1024*1024)break;
      }
    }
    shareBodies.delete(key);
    shareBodies.set(key,bodies);
    let total=Array.from(shareBodies.values()).reduce((n,entries)=>n+activeBytes(entries),0);
    for(const [cachedKey,entries] of shareBodies){
      if(total<=48*1024*1024||cachedKey===key)break;
      shareBodies.delete(cachedKey);
      total-=activeBytes(entries);
    }
  }
  const send=(address,options)=>{
    if(!guard())throw new Error('账号已变化，本次同步已取消');
    const url=new URL(address);
    const method=(options?.method??'GET').toUpperCase();
    const sharedPath=url.pathname.startsWith(root+'shared/');
    const privateRequest=url.pathname.startsWith(root+'private/');
    const action=sharedPath||privateRequest?url.pathname.slice((root+(sharedPath?'shared/':'private/')).length):'';
    const writeBody=method==='POST'&&typeof options?.body==='string'&&
      ((contentWrite&&action)||privateRequest&&['assets/upload','records/modify'].includes(action))?JSON.parse(options.body):null;
    const sharedWrite=contentWrite&&writeBody&&contentWrite.owned===privateRequest&&permitsSharedContentWrite(contentWrite,action,writeBody);
    const privateSharedContent=privateRequest&&/^QuietShared(?:Note|Folder)-v1-/.test(writeBody?.zoneID?.zoneName)&&
      (action==='assets/upload'||writeBody.operations?.some(item=>item.record?.recordType==='QuietNoteEdit'));
    if(privateSharedContent&&!sharedWrite)throw new Error('共享写入请求已失效');
    const assetWrite=contentWrite&&method==='POST'&&options?.body instanceof Blob&&url.origin!==origin;
    if(assetWrite&&(!contentWrite.available()||url.protocol!=='https:'||url.username||url.password||
      !/(^|\.)(icloud-content\.com(?:\.cn)?|apzones\.com)$/.test(url.hostname)))throw new Error('共享附件上传请求已失效');
    const invitationPath=['resolve','accept'].find(action=>url.pathname===root+'public/records/'+action);
    let allowedInvitation=false;
    if(invitationPath&&invitationRequest?.action===invitationPath&&method==='POST'&&typeof options?.body==='string'){
      const body=JSON.parse(options.body);
      allowedInvitation=Object.keys(body).length===1&&body.shortGUIDs?.length===1&&
        Object.keys(body.shortGUIDs[0]).length===1&&body.shortGUIDs[0].value===invitationRequest.guid;
    }
    const sharedZoneList=sharedDiscovery&&url.pathname===root+'shared/zones/list'&&method==='GET'&&options?.body==null;
    let sharedMembers=false;
    if(sharedReading&&url.pathname===root+'shared/records/lookup'&&method==='POST'&&typeof options?.body==='string'){
      const {zoneID,records}=JSON.parse(options.body);
      sharedMembers=allowedSharedZones.has(zoneKey(zoneID))&&Array.isArray(records)&&records.length===1&&
        records[0]?.recordName===zoneShareName;
    }
    let sharedChanges=false;
    if(sharedReading&&url.pathname===root+'shared/changes/zone'&&method==='POST'&&typeof options?.body==='string'){
      const zones=JSON.parse(options.body).zones;
      sharedChanges=Array.isArray(zones)&&zones.length===1&&allowedSharedZones.has(zoneKey(zones[0]?.zoneID));
    }
    let sharedLeave=false;
    if(leaving&&leaveRequest&&url.pathname===root+'shared/records/modify'&&method==='POST'&&typeof options?.body==='string'){
      const {zoneID,operations}=JSON.parse(options.body);
      const operation=operations?.[0];
      sharedLeave=zoneKey(zoneID)===leaveRequest&&allowedSharedZones.has(leaveRequest)&&
        Array.isArray(operations)&&operations.length===1&&operation?.operationType==='forceDelete'&&
        operation.record?.recordName===zoneShareName&&Object.keys(operation.record).length===1;
    }
    let presenceWrite=false;
    if(presenceRequest&&url.pathname===presenceRequest.path&&method==='POST'&&typeof options?.body==='string'){
      const {zoneID,operations}=JSON.parse(options.body);
      const {operationType,record}=operations?.[0]??{};
      if(record?.recordType==='QuietPresence'){
        presenceWrite=operations.length===1&&operationType==='forceReplace'&&presenceRequest.available()&&
          zoneKey(zoneID)===zoneKey(presenceRequest.zone)&&record.recordName===presenceRequest.record.recordName&&
          JSON.stringify(record.fields)===JSON.stringify(presenceRequest.record.fields);
        if(!presenceWrite)throw new Error('共享编辑状态请求已失效');
      }
    }
    if(readOnly){
      const privateRead=privateRequest&&method==='POST'&&
        ['zones/lookup','changes/zone','records/lookup'].includes(action);
      const invitationRead=invitationPath==='resolve'&&allowedInvitation;
      if(url.origin!==origin||url.username||url.password||
        !(privateRead||sharedZoneList||sharedChanges||sharedMembers||invitationRead))
        throw new Error('此 Web 预览只允许读取 iCloud');
    }
    if(sharedPath||invitationPath||url.searchParams.has('ckWebAuthToken')||url.searchParams.has('ckAPIToken')){
      if(url.origin!==origin||(!privateRequest&&!sharedZoneList&&!sharedChanges&&!sharedMembers&&!allowedInvitation&&!sharedLeave&&!presenceWrite&&!sharedWrite))throw new Error('CloudKit 会话超出私有文档库范围');
      url.searchParams.set('clientId',clientId);
      if(allowedInvitation)url.searchParams.set('ckWebAuthToken',session);
    }
    const request={...options,credentials:'omit',redirect:'error',
      ...(sharedZoneList||sharedChanges||sharedMembers||allowedInvitation||sharedLeave||presenceWrite||sharedWrite?{cache:'no-store'}:{}),
      ...(allowedInvitation||sharedLeave||presenceWrite||sharedWrite||assetWrite?{signal:AbortSignal.timeout(transport.timeoutMs)}:{})};
    if(sessionRequest)return sessionRequest(url,{...request,cache:'no-store'},fetch);
    return fetch(url,request);
  };
  const logger={debug(){},info(){},log(){},warn(){},error(){}};
  CloudKit.configure({locale:'zh-cn',services:sessionRequest?{fetch:send,logger}:{fetch:send},containers:[{
    containerIdentifier:config.containerIdentifier,environment:config.environment,
    apiTokenAuth:{apiToken:config.apiToken,ckWebAuthToken:session,persist:false}
  }]});
  const container=CloudKit.getContainer(config.containerIdentifier);
  const transport=new CloudTransport(config,container,guard);
  transport.readOnly=readOnly;
  if(invitations){
    const invitation=async(action,guid)=>{
      if(!guard())throw new Error('账号已变化，请重新检查邀请');
      if(invitationRequest)throw new Error('正在处理邀请，请稍候');
      invitationRequest={action,guid};
      try{
        const invoke=action==='resolve'?'fetchRecordInfos':'acceptShares';
        const response=check(await deadline(container[invoke]([guid]),transport.timeoutMs));
        if(!guard())throw new Error('账号已变化，请重新检查邀请');
        if(response.results?.length!==1)throw new Error('Apple 未返回这条邀请的结果');
        return check(response.results[0]);
      }finally{invitationRequest=undefined;}
    };
    transport.resolveInvitation=async value=>{
      const {guid,region}=parseInvitation(value);
      let info;
      try{info=shareInfo(await invitation('resolve',guid),config,guid);}
      catch(error){
        if(!guard()||!isExpiredSession(error))throw error;
        info=await findAcceptedInvitation(guid);
        if(!info)throw error;
      }
      let confirmed=false;
      return {...info,region,accept:async()=>{
        if(!guard())throw new Error('账号已变化，请重新检查邀请');
        if(info.status==='ACCEPTED'){leftSharedZones.delete(zoneKey(info.zone));return info.scope;}
        if(readOnly||!allowInvitationAcceptance)throw new Error('请先在飞记 App 接受邀请');
        if(confirmed)throw new Error('请重新检查这条邀请后再确认');
        confirmed=true;
        let receipt,acceptError;
        try{receipt=await invitation('accept',guid);}
        catch(error){
          if(!guard()||isExpiredSession(error))throw error;
          // A lost acceptance reply can leave the membership active. Verify
          // the same invitation before asking the recipient to retry.
          acceptError=error;
        }
        if(receipt?.zoneID&&zoneKey(receipt.zoneID)!==zoneKey(info.zone))throw new Error('Apple 尚未确认此共享已生效，请重新检查邀请');
        const accepted=shareInfo(await invitation('resolve',guid),config,guid);
        if(zoneKey(accepted.zone)!==zoneKey(info.zone))throw new Error('Apple 尚未确认此共享已生效，请重新检查邀请');
        if(accepted.status!=='ACCEPTED')throw acceptError??new Error('Apple 尚未确认此共享已生效，请重新检查邀请');
        leftSharedZones.delete(zoneKey(accepted.zone));
        return accepted.scope;
      }};
    };
  }
  if(checkShared&&sharedDiscovery)transport.inspectSharedZones=async()=>{
    const zones=await acceptedSharedZones(container,guard,transport.timeoutMs);
    return {documents:zones.filter(zone=>!zone.folder).length,folders:zones.filter(zone=>zone.folder).length};
  };
  async function currentGuestZones(){
    const zones=(await acceptedSharedZones(container,guard,transport.timeoutMs))
      .filter(zone=>!leftSharedZones.has(zoneKey(zone.zoneID)));
    allowedSharedZones=new Set(zones.map(zone=>zoneKey(zone.zoneID)));
    for(const cache of [shareCursors,shareBodies,shareTitles,sharePermissions]){
      for(const key of cache.keys())if(key.startsWith('shared:')&&!allowedSharedZones.has(key.slice(7)))cache.delete(key);
    }
    return zones;
  }
  async function findAcceptedInvitation(guid){
    const account=transport.owner;
    if(!account)return null;
    const zones=(await currentGuestZones()).filter(zone=>zone.zoneID.ownerRecordName!==account);
    // An error from public/records/resolve does not remove access to existing shares.
    // Look up only share metadata and require Apple's exact invitation identifier.
    for(let i=0;i<zones.length;i+=4){
      const batch=await Promise.all(zones.slice(i,i+4).map(async zone=>{
        const {zoneID,root,folder}=zone;
        const scope={owner:zoneID.ownerRecordName,root,folder};
        const access=shareAccess(scope,account);
        try{
          const share=await access.share();
          if((share.shortGUID?.value??share.shortGUID)!==guid)return null;
          const current=share.currentUserParticipant;
          const member=current?.userIdentity?.userRecordName?current:
            share.participants?.find(item=>item.userIdentity?.userRecordName===account);
          if(member?.userIdentity?.userRecordName!==account||member.acceptanceStatus!=='ACCEPTED'||
             !['READ_ONLY','READ_WRITE'].includes(member.permission))return null;
          return {status:'ACCEPTED',permission:member.permission,title:shareTitle(share),scope,zone:zoneID};
        }catch(error){
          if(!guard()||isExpiredSession(error))throw error;
          return null;
        }
      }));
      const match=batch.find(Boolean);
      if(match)return match;
    }
    return null;
  }
  const readRoom=async (scope,requested=[],current=()=>true,checkedPermission,assetPaths)=>{
    const prefix=scope.owned?'private:':'shared:';
    const id={zoneName:scope.zoneID.zoneName,ownerRecordName:scope.owner??scope.zoneID.ownerRecordName};
    const key=prefix+zoneKey(id);
    const access=shareAccess({...scope,owner:id.ownerRecordName},scope.owned?id.ownerRecordName:transport.owner);
    const active=()=>current()&&access.available();
    if(!active())throw new Error('共享访问已变更，请刷新后重试');
    let title;
    if(lazyBodies){
      if(!scope.folder&&!shareTitles.has(key)){
        try{shareTitles.set(key,shareTitle(await access.share()));}
        catch(error){if(!active()||isExpiredSession(error))throw error;}
      }
      title=shareTitles.get(key)??'';
    }
    const selection=lazyBodies?{notes:new Set(requested),assetPaths,saved:shareBodies.get(key)??new Map(),active}:null;
    const room=await readShareZone(transport,scope,(cursor)=>{
      if(cursor)shareCursors.set(key,cursor);else shareCursors.delete(key);
    },selection);
    if(!active())throw new Error('共享访问已变更，请刷新后重试');
    if(lazyBodies){
      room.listing=title;
      rememberBodies(key,selection.result,requested);
    }
    if(!sharedWriting){delete room.generation;return room;}
    const writeAllowed=checkedPermission??await transport.canWriteShare(room,transport.owner);
    if(!active())throw new Error('共享访问已变更，请刷新后重试');
    sharePermissions.set(key,writeAllowed);
    return {...room,editable:room.generation!==undefined&&writeAllowed};
  };
  if(invitations)transport.readInvitedShare=async({owner,root,folder,owned=false},account)=>{
    if(!account||transport.owner!==account||owned||owner===account)throw new Error('共享访问已变更，请重新检查邀请');
    const key=zoneKey({zoneName:sharedZoneName(root,folder),ownerRecordName:owner});
    const zone=(await currentGuestZones()).find(candidate=>zoneKey(candidate.zoneID)===key);
    const available=()=>guard()&&allowedSharedZones.has(key)&&!leftSharedZones.has(key);
    if(!zone||!available())throw new Error('共享访问已变更，请重新检查邀请');
    const room=await readRoom(zone);
    if(!available())throw new Error('共享访问已变更，请重新检查邀请');
    return room;
  };
  if(sharedReading)transport.readAcceptedShares=async(notesFor=()=>[])=>{
    const zones=await currentGuestZones();
    const rooms=[];let failed=0;
    for(const zone of zones){
      try{rooms.push(await readRoom(zone,notesFor({...zone,owner:zone.zoneID.ownerRecordName})));}
      catch(error){if(!guard()||isExpiredSession(error))throw error;failed++;}
    }
    return {rooms:rooms.filter(room=>!leftSharedZones.has(zoneKey({zoneName:sharedZoneName(room.root,room.folder),ownerRecordName:room.owner}))),failed};
  };
  if(sharedReading)transport.acceptedShareScopes=async()=>{
    const zones=await currentGuestZones();
    return zones.map(({zoneID,root,folder})=>({owner:zoneID.ownerRecordName,root,folder}));
  };
  if(leaving)transport.leaveShare=async({owner,root,folder,owned=false},account)=>{
    const zoneID={zoneName:sharedZoneName(root,folder),ownerRecordName:owner};
    const key=zoneKey(zoneID);
    if(!guard()||owned||!account||owner===account||!allowedSharedZones.has(key))
      throw new Error('共享访问已变更，请刷新后重试');
    if(leaveRequest)throw new Error('正在退出共享，请稍候');
    leaveRequest=key;
    try{
      const before=await currentGuestZones();
      if(before.some(zone=>zoneKey(zone.zoneID)===key)){
        let failure;
        try{check(await deadline(container.sharedCloudDatabase.deleteRecords(zoneShareName,{zoneID}),transport.timeoutMs));}
        catch(error){failure=error;}
        const after=await currentGuestZones();
        if(after.some(zone=>zoneKey(zone.zoneID)===key))
          throw failure||new Error('iCloud 尚未确认退出，请刷新共享文档后重试');
      }
      if(!guard())throw new Error('账号已变化，本次操作已取消');
      leftSharedZones.add(key);
      allowedSharedZones.delete(key);
    }finally{leaveRequest=undefined;}
  };
  if(sharedReading)transport.ownedShareScopes=async(candidates,owner)=>{
    if(!/^_[a-f0-9]{16,128}$/i.test(owner)){ownedSharedZones.clear();return [];}
    const result=[];
    for(const {root,folder} of candidates){
      if(!uuid.test(root)||[assetScope,catalogueScope].includes(root)||typeof folder!=='boolean')continue;
      const zoneID={zoneName:sharedZoneName(root,folder)};
      try{
        const share=await shareRecord(transport,transport.db,zoneID,id=>ownedZone(id,zoneID.zoneName,owner),lazyBodies?['cloudkit.title']:[]);
        if(lazyBodies)shareTitles.set('private:'+zoneKey({...zoneID,ownerRecordName:owner}),shareTitle(share));
        result.push({owner,root,folder,owned:true});
      }catch(error){if(!guard()||isExpiredSession(error))throw error;}
    }
    ownedSharedZones=new Map(result.map(({root,folder})=>[sharedZoneName(root,folder),owner]));
    const keys=new Set(result.map(({root,folder})=>'private:'+zoneKey({zoneName:sharedZoneName(root,folder),ownerRecordName:owner})));
    for(const cache of [shareCursors,shareBodies,shareTitles,sharePermissions]){
      for(const key of cache.keys())if(key.startsWith('private:')&&!keys.has(key))cache.delete(key);
    }
    return result;
  };
  if(sharedReading)transport.readOwnedShares=async(scopes,account,notesFor=()=>[])=>{
    const rooms=[];let failed=0;
    for(const scope of scopes){
      const zoneID={zoneName:sharedZoneName(scope.root,scope.folder)};
      if(scope.owned!==true||scope.owner!==account||ownedSharedZones.get(zoneID.zoneName)!==account)continue;
      try{rooms.push(await readRoom({...scope,zoneID},notesFor(scope)));}
      catch(error){if(!guard()||isExpiredSession(error))throw error;failed++;}
    }
    return {rooms,failed};
  };
  function shareAccess({owner,root,folder,owned=false},account){
    const zoneID={zoneName:sharedZoneName(root,folder),ownerRecordName:owner};
    const available=()=>guard()&&(owned?owner===account&&ownedSharedZones.get(zoneID.zoneName)===account:allowedSharedZones.has(zoneKey(zoneID)));
    const db=owned?transport.db:container.sharedCloudDatabase;
    const zone=owned?{zoneName:zoneID.zoneName}:zoneID;
    const matches=id=>owned?ownedZone(id,zoneID.zoneName,owner):zoneKey(id)===zoneKey(zoneID);
    return {available,db,zone,matches,async share(){
      if(!available())throw new Error('共享访问已变更，请刷新后重试');
      const record=await shareRecord(transport,db,zone,matches,lazyBodies?['cloudkit.title']:[]);
      if(!available())throw new Error('共享访问已变更，请刷新后重试');
      return record;
    }};
  }
  if(sharedWriting){
    transport.canWriteShare=async(scope,account)=>{
      if(!account||transport.owner!==account)return false;
      const access=shareAccess(scope,account);
      if(!access.available())return false;
      const share=await access.share();
      return scope.owned===true||share.participants?.some(member=>member.userIdentity?.userRecordName===account&&
        member.acceptanceStatus==='ACCEPTED'&&member.permission==='READ_WRITE')===true;
    };
    transport.writeShare=async(scope,account,edits,expectedGeneration,stillCurrent)=>{
      if(contentWrite)throw new Error('正在同步另一处共享改动');
      const access=shareAccess(scope,account);
      const available=()=>access.available()&&transport.owner===account&&stillCurrent();
      if(!available()||!await transport.canWriteShare(scope,account)||!available())throw new Error('共享编辑权限已变更');
      if(contentWrite)throw new Error('正在同步另一处共享改动');
      if(edits.some(edit=>!scope.folder&&![scope.root,assetScope,catalogueScope].includes(edit.note)))
        throw new Error('共享改动超出授权文档范围');
      contentWrite={zone:access.zone,owned:scope.owned===true,available,notes:new Map(edits.map(edit=>[edit.id,edit.note]))};
      try{
        return await commitSharedEdits({db:access.db,zoneID:access.zone,matchesZone:access.matches,expectedGeneration,available},edits);
      }finally{contentWrite=undefined;}
    };
    transport.readShareForWrite=async(scope,account,notes=[])=>{
      const access=shareAccess(scope,account);
      if(!access.available())throw new Error('共享访问已变更，请刷新后重试');
      const room=await readRoom({...scope,zoneID:access.zone},notes);
      if(!access.available())throw new Error('共享访问已变更，请刷新后重试');
      return room;
    };
  }
  if(sharedReading&&lazyBodies)transport.readSharedDocument=async(scope,note,account,current,assetPaths)=>{
    if(!uuid.test(note)||note===catalogueScope||note!==assetScope&&!scope.folder&&scope.root!==note)throw new Error('共享文档范围不匹配');
    if(!scope.owned)await currentGuestZones();
    const access=shareAccess(scope,account);
    const room=await readRoom({...scope,zoneID:access.zone},[note],current,undefined,assetPaths);
    if(assetPaths&&!scope.owned){
      await currentGuestZones();
      if(!access.available())throw Object.assign(new Error('共享访问已变更，请刷新后重试'),{code:'SHARED_ACCESS_CHANGED'});
    }
    return room;
  };
  if(sharedReading)transport.readUpdatedShare=async(scope,account,note)=>{
    const access=shareAccess(scope,account);
    if(!access.available())throw new Error('共享访问已变更，请刷新后重试');
    const key=(scope.owned?'private:':'shared:')+zoneKey({...access.zone,ownerRecordName:scope.owner});
    const syncToken=shareCursors.get(key);
    // Member permissions can change without a content edit or a zone delta.
    const permission=sharedWriting?await transport.canWriteShare(scope,account):undefined;
    const permissionChanged=sharedWriting&&permission!==sharePermissions.get(key);
    if(!access.available())throw new Error('共享访问已变更，请刷新后重试');
    let roleOnly=false;
    if(syncToken){
      try{
        const query={zoneID:access.zone,syncToken,desiredKeys:['note'],resultsLimit:30};
        const zone=await readSharedPage(transport,access.db,query,access.matches);
        if(!access.available())throw new Error('共享访问已变更，请刷新后重试');
        const records=(zone.records??[]).map(record=>validateSharedRecord(record,access.matches));
        const onlyAuxiliary=records.every(record=>sharedAuxiliary(record)&&(!sharedWriting||record.recordType==='QuietPresence'));
        if(onlyAuxiliary&&!zone.moreComing){
          if(zone.syncToken)shareCursors.set(key,zone.syncToken);
          if(!permissionChanged)return null;
          roleOnly=records.every(record=>record.recordType==='QuietPresence');
        }
      }catch(error){if(!access.available()||!expiredCursor(error))throw error;}
    }
    const room=await readRoom({...scope,zoneID:access.zone},note?[note]:[],access.available,permission);
    if(!access.available())throw new Error('共享访问已变更，请刷新后重试');
    return roleOnly?{...room,roleOnly}:room;
  };
  if(sharedReading)transport.readShareMembers=async(scope,account)=>{
    const share=await shareAccess(scope,account).share();
    if(!Array.isArray(share.participants))throw new Error('iCloud 未返回可查看的成员信息');
    return share.participants.slice(0,100).filter(member=>member?.acceptanceStatus!=='REMOVED').map(member=>{
      const identity=member?.userIdentity;
      const name=[identity?.nameComponents?.givenName,identity?.nameComponents?.familyName]
        .filter(part=>typeof part==='string').join(' ').trim().slice(0,100);
      return {name,self:!!account&&identity?.userRecordName===account,owner:member?.type==='OWNER',
        permission:member?.permission==='READ_WRITE'?'write':member?.permission==='READ_ONLY'?'read':'unknown',
        status:member?.acceptanceStatus==='ACCEPTED'?'accepted':['PENDING','INVITED'].includes(member?.acceptanceStatus)?'pending':'unknown'};
    });
  };
  if(sharedReading&&previewPresence)transport.presenceReader=({scope,note},account,active)=>{
    if(!account||!uuid.test(note)||[assetScope,catalogueScope].includes(note)||!scope.folder&&scope.root!==note)return null;
    const access=shareAccess(scope,account);
    const available=()=>active()&&access.available();
    if(!available())return null;
    return createPresenceReader({account,note,available,readShare:access.share,readPage:async syncToken=>{
      const query={zoneID:access.zone,desiredKeys:['state'],desiredRecordTypes:['QuietPresence'],resultsLimit:100,
        ...(syncToken?{syncToken}:{})};
      const reply=check(await deadline(access.db.fetchRecordZoneChanges([query]),transport.timeoutMs));
      if(!available())throw new Error('共享访问已变更，请刷新后重试');
      const zone=reply.zones?.[0];
      if(reply.zones?.length!==1||!access.matches(zone?.zoneID)||zone.records?.some(record=>record.zoneID&&!access.matches(record.zoneID)))
        throw new Error('CloudKit 返回了其它共享记录区');
      return check(zone);
    }});
  };
  if(!readOnly&&sharedReading&&previewPresence&&previewPresenceWrites){
    const peer=crypto.getRandomValues(new BigUint64Array(1))[0].toString();
    const idle=bytesB64(new TextEncoder().encode(JSON.stringify({note:null,anchor:null,head:null})));
    transport.presenceWriter=({scope,note},account,editing)=>{
      if(!account||transport.owner!==account||!uuid.test(scope.root)||!uuid.test(note)||
         [assetScope,catalogueScope].includes(note)||!scope.folder&&scope.root!==note)return null;
      const access=shareAccess(scope,account);
      if(!access.available())return null;
      return async state=>{
        const available=()=>access.available()&&(state===null||editing());
        if(!available())return;
        if(state!==null){
          if(typeof state!=='string'||state.length>5464)throw new Error('共享编辑状态无效');
          const bytes=b64Bytes(state);
          if(bytes.length>4096||JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes))?.note!==note)
            throw new Error('共享编辑状态超出当前文档');
        }
        const share=await access.share();
        const canEdit=scope.owned||share.participants?.some(member=>member.userIdentity?.userRecordName===account&&
          member.acceptanceStatus==='ACCEPTED'&&member.permission==='READ_WRITE');
        if(!canEdit||!available())return;
        if(presenceRequest)throw new Error('正在提交共享编辑状态');
        const record={recordName:peer,recordType:'QuietPresence',fields:{
          name:{value:'Collaborator',type:'STRING'},state:{value:state??idle,type:'BYTES'},
        }};
        const path=root+(scope.owned?'private':'shared')+'/records/modify';
        presenceRequest={path,available,zone:access.zone,record};
        try{
          const batch=access.db.newRecordsBatch({zoneID:access.zone,atomic:true}).forceReplace(record);
          const result=check(await deadline(batch.commit(),transport.timeoutMs));
          const saved=result.records?.[0];
          if(result.records?.length!==1||saved.recordName!==peer||saved.zoneID&&!access.matches(saved.zoneID))
            throw new Error('iCloud 未确认共享编辑状态');
        }finally{presenceRequest=undefined;}
      };
    };
  }
  return transport;
}
export function isExpiredSession(error){
  return [error?.ckErrorCode,error?.serverErrorCode,error?.code].includes('AUTHENTICATION_REQUIRED')||error?.status===421;
}
function privateOwner(zone){
  const valid=owner=>typeof owner==='string'&&/^_[a-f0-9]{16,128}$/i.test(owner);
  const owners=new Set([zone.zoneID?.ownerRecordName,...(zone.records||[]).map(record=>record.zoneID?.ownerRecordName)].filter(valid));
  if(owners.size>1)throw new Error('CloudKit 返回了多个私有区所有者');
  return owners.values().next().value||null;
}
function assetAddress(value){
  const url=new URL(value.replace('${f}','update.bin'));
  if(url.protocol!=='https:'||url.username||url.password||!/(^|\.)(icloud-content\.com(?:\.cn)?|icloud\.com(?:\.cn)?|apzones\.com|apple-cloudkit\.com)$/.test(url.hostname))throw new Error('CloudKit 返回了未知的附件地址');
  return url;
}
function deadline(promise,milliseconds){
  let timer;
  return Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('CloudKit 请求超时，修改仍保留，请重试')),milliseconds);})]).finally(()=>clearTimeout(timer));
}
const zoneKey=zone=>JSON.stringify([zone?.ownerRecordName,zone?.zoneName]);
function permitsSharedContentWrite({zone,available,notes},action,body){
  if(!available())return false;
  if(action==='zones/lookup')return body.zones?.length===1&&zoneKey(body.zones[0])===zoneKey(zone);
  if(zoneKey(body.zoneID)!==zoneKey(zone))return false;
  if(action==='records/lookup'){
    const name=body.records?.[0]?.recordName;
    return body.records?.length===1&&(name===retentionFence||notes.has(name));
  }
  if(action==='assets/upload')return body.tokens?.length>0&&body.tokens.length<=notes.size&&
    body.tokens.every(token=>notes.has(token.recordName)&&token.recordType==='QuietNoteEdit'&&token.fieldName==='update');
  if(action!=='records/modify'||body.atomic!==true||!Array.isArray(body.operations)||body.operations.length<2||body.operations.length>notes.size+1)return false;
  const fence=body.operations.at(-1);
  return ['create','update'].includes(fence.operationType)&&fence.record?.recordName===retentionFence&&
    fence.record.recordType==='QuietNoteEdit'&&uuid.test(fence.record.fields?.note?.value)&&Object.keys(fence.record.fields).length===1&&
    body.operations.slice(0,-1).every(({operationType,record})=>operationType==='create'&&record?.recordType==='QuietNoteEdit'&&
      notes.has(record.recordName)&&record.fields?.note?.value===notes.get(record.recordName));
}
const zoneShareName='cloudkit.zoneshare';
const sharedZoneName=(root,folder)=>`QuietShared${folder?'Folder':'Note'}-v1-${root}`;
const ownedZone=(zone,name,owner)=>zone?.zoneName===name&&[undefined,'_defaultOwner',owner].includes(zone.ownerRecordName);
const shareTitle=share=>typeof share.fields?.['cloudkit.title']?.value==='string'?share.fields['cloudkit.title'].value.slice(0,512):'';
async function shareRecord(transport,db,zoneID,matches,desiredKeys=[]){
  if(!transport.guard())throw new Error('账号已变化，本次同步已取消');
  const result=check(await deadline(db.fetchRecords(zoneShareName,{zoneID,desiredKeys}),transport.timeoutMs));
  if(!transport.guard())throw new Error('账号已变化，本次同步已取消');
  const share=result.records?.length===1?result.records[0]:null;
  if(share?.recordType?.toLowerCase()!=='cloudkit.share'||share.recordName?.toLowerCase()!==zoneShareName.toLowerCase()||
     (share.zoneID&&!matches(share.zoneID)))throw new Error('iCloud 未返回预期的共享记录');
  return share;
}
const assetScope='00000000-0000-0000-0000-000000000002';
const catalogueScope='00000000-0000-0000-0000-000000000001';

function needsAttachment(record, paths) {
  if(!paths||record.fields?.note?.value!==assetScope)return true;
  const index=record.fields?.assetIndex?.value;
  // Older records have the header only inside the payload. Keep them readable.
  if(typeof index!=='string'||index.length>5464)return true;
  try{
    const {path}=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(b64Bytes(index)));
    return typeof path!=='string'||!path.startsWith('assets/')||paths.has(path);
  }catch{return true;}
}
const uuid=/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/;
export function parseInvitation(value){
  const link=value.trim();
  const branded=/^(?:https:\/\/feinotes\.app\/share\/\?|feinotes:\/\/share\/\?)([cg])=([A-Za-z0-9_-]{1,256})$/.exec(link);
  if(branded)return {region:branded[1]==='c'?'china':'global',guid:branded[2]};
  const apple=/^https:\/\/(?:www\.)?icloud\.com(\.cn)?\/share\/([A-Za-z0-9_-]{1,256})(?:#[^\s]*)?$/.exec(link);
  if(apple&&link.length<=4096)return {region:apple[1]?'china':'global',guid:apple[2]};
  throw new Error('请粘贴完整的飞记或 iCloud 共享邀请链接');
}
function sharedScope(zone){
  const {zoneName,ownerRecordName}=zone??{};
  const match=/^QuietShared(Note|Folder)-v1-(.+)$/.exec(zoneName);
  if(!match||!uuid.test(match[2])||[assetScope,catalogueScope].includes(match[2])||
     typeof ownerRecordName!=='string'||!ownerRecordName||new TextEncoder().encode(ownerRecordName).length>256||
     /[\x00-\x1f\x7f]/.test(ownerRecordName)||ownerRecordName==='_defaultOwner')return null;
  return {zoneID:{zoneName,ownerRecordName},root:match[2],folder:match[1]==='Folder'};
}
function shareInfo(info,config,guid){
  const zone=sharedScope(info.zoneID);
  const share=info.share;
  if(!zone||info.containerIdentifier!==config.containerIdentifier||info.environment!==config.environment||
     (info.shortGUID?.value??info.shortGUID)!==guid||share?.recordType?.toLowerCase()!=='cloudkit.share'||
     share.recordName?.toLowerCase()!=='cloudkit.zoneshare'||
     (share.zoneID&&zoneKey(share.zoneID)!==zoneKey(zone.zoneID))){
    throw new Error('此邀请与当前飞记开发环境的共享文档不匹配');
  }
  const {participantStatus:status,participantPermission:permission}=info;
  if(info.participantType==='OWNER')throw new Error('这是你创建的共享，请在所有者的文档库中打开');
  if(!['INVITED','ACCEPTED'].includes(status)||!['READ_ONLY','READ_WRITE'].includes(permission)){
    throw Object.assign(new Error('此 Apple 账号没有可接受的共享邀请'),{serverErrorCode:'INVITATION_ACCOUNT_REQUIRED'});
  }
  const title=share.fields?.['cloudkit.title']?.value;
  return {status,permission,title:typeof title==='string'?title.slice(0,100):'',
    scope:{owner:zone.zoneID.ownerRecordName,root:zone.root,folder:zone.folder},zone:zone.zoneID};
}
async function acceptedSharedZones(container,guard,timeout){
  if(!guard())throw new Error('账号已变化，本次检查已取消');
  const response=check(await deadline(container.sharedCloudDatabase.fetchAllRecordZones(),timeout));
  if(!guard())throw new Error('账号已变化，本次检查已取消');
  if(!Array.isArray(response.zones))throw new Error('CloudKit 未返回共享记录区列表');
  const zones=new Map();
  for(const zone of response.zones){
    if(zone.deleted)continue;
    const scope=sharedScope(check(zone).zoneID);
    if(scope)zones.set(zoneKey(scope.zoneID),scope);
  }
  return [...zones.values()];
}
const expiredCursor=error=>[error?.serverErrorCode,errorCode(error)].includes('CHANGE_TOKEN_EXPIRED');
const sharedAuxiliary=record=>record.recordType?.toLowerCase()==='cloudkit.share'||record.recordType==='QuietPresence';
function digestHex(record){
  const digest=record.fields?.digest?.value;
  if(typeof digest!=='string')return null;
  try{return Array.from(b64Bytes(digest),byte=>byte.toString(16).padStart(2,'0')).join('');}
  catch{return null;}
}
async function readSharedPage(transport,db,query,matches){
  if(!transport.guard())throw new Error('账号已变化，本次同步已取消');
  const reply=check(await deadline(db.fetchRecordZoneChanges([query]),transport.timeoutMs));
  if(!transport.guard())throw new Error('账号已变化，本次同步已取消');
  if(reply.zones?.length!==1||!matches(reply.zones[0].zoneID))throw new Error('CloudKit 返回了其它共享记录区');
  return check(reply.zones[0]);
}
function validateSharedRecord(result,matches){
  const record=check(result);
  if(record.zoneID&&!matches(record.zoneID))throw new Error('CloudKit 共享记录归属不一致');
  return record;
}
async function readShareZone(transport,{zoneID,root,folder,owner=zoneID.ownerRecordName,owned=false},rememberCursor,selection){
  const db=owned?transport.db:transport.container.sharedCloudDatabase;
  const matches=id=>owned?ownedZone(id,zoneID.zoneName,owner):zoneKey(id)===zoneKey(zoneID);
  const {records,cursor,generation}=await sharedRecordInventory(transport,db,zoneID,matches,root,folder);
  const edits=[];
  const needed=[];
  if(selection)selection.result=new Map();
  for(const record of records){
    const cached=selection?.saved.get(record.recordName);
    if(cached){
      if(cached.digest!==record.fields.digest?.value||cached.edit.note!==record.fields.note.value)
        throw new Error('iCloud 已确认的共享记录内容发生变化');
      edits.push(cached.edit);
      selection.result.set(record.recordName,cached);
    }else if(!selection||record.fields.note.value===catalogueScope||
             selection.notes.has(record.fields.note.value)&&needsAttachment(record,selection.assetPaths)){
      needed.push(record);
    }
  }
  let size=edits.reduce((total,edit)=>total+edit.data.length,0);
  for(let i=0;i<needed.length;i+=4){
    if(!transport.guard()||selection&&!selection.active())throw new Error('账号或共享访问已变化，本次同步已取消');
    const page=needed.slice(i,i+4);
    const batch=await Promise.all(page.map(record=>transport.download(record,true)));
    size+=batch.reduce((total,edit)=>total+edit.data.length,0);
    if(size>64*1024*1024)throw new Error('共享记录区超出预览容量');
    edits.push(...batch);
    if(selection)batch.forEach((edit,i)=>selection.result.set(edit.id,{edit,digest:page[i].fields.digest.value}));
  }
  if(!transport.guard())throw new Error('账号已变化，本次同步已取消');
  rememberCursor(cursor);
  return {owner,root,folder,...(owned?{owned:true}:{}),edits,generation};
}
async function sharedRecordInventory(transport,db,zoneID,matches,root,folder){
  const records=new Map(),cursors=new Set();
  let syncToken,restarted=false,generation=null;
  for(let page=0;page<200;page++){
    const query={zoneID,resultsLimit:30,...(syncToken?{syncToken}:{})};
    let zone;
    try{zone=await readSharedPage(transport,db,query,matches);}
    catch(error){
      if(!transport.guard()||!syncToken||restarted||!expiredCursor(error))throw error;
      // Restart the entire inventory once. An expired cursor cannot confirm
      // which records survived the Mac's latest compaction.
      restarted=true;
      syncToken=undefined;
      generation=null;
      records.clear();
      cursors.clear();
      continue;
    }
    for(const result of zone.records??[]){
      const record=validateSharedRecord(result,matches);
      if(record.recordName===retentionFence){
        const value=record.fields?.note?.value;
        // Native CloudKit writes Swift's uppercase UUID strings. Keep the
        // original value for the exact generation comparison during writes.
        generation=record.deleted?null:typeof value==='string'&&uuid.test(value.toLowerCase())?value:undefined;
        continue;
      }
      if(record.deleted){records.delete(record.recordName);continue;}
      if(sharedAuxiliary(record))continue;
      const scope=record.fields?.note?.value;
      if(typeof record.recordName!=='string'||!record.recordName||!uuid.test(scope)||
        !(folder||scope===root||scope===assetScope||scope===catalogueScope))throw new Error('CloudKit 共享记录超出授权文档范围');
      records.set(record.recordName,record);
    }
    if(!zone.moreComing)return {records:[...records.values()],cursor:zone.syncToken,generation};
    if(!zone.syncToken||cursors.has(zone.syncToken))throw new Error('共享区分页没有继续');
    syncToken=zone.syncToken;
    cursors.add(syncToken);
  }
  throw new Error('共享区记录较多，请重试');
}
export class CloudTransport {
  constructor(config,container,guard,timeoutMs=30000){this.timeoutMs=timeoutMs;this.guard=guard;this.config=config;this.container=container;this.db=container.privateCloudDatabase;this.zoneID={zoneName:config.zoneName};}
  #requireCurrent(current,action='同步'){
    if(!this.guard())throw new Error(`账号已变化，本次${action}已取消`);
    if(!current())throw new Error(`连接已变化，本次${action}已取消`);
  }
  async readTrial(current){
    const verifyCurrent=()=>this.#requireCurrent(current,'检查试用');
    verifyCurrent();
    const owner=this.owner;
    if(!owner)throw new Error('尚未确认 iCloud 文档库账户');
    const channel=/-QA-([a-f0-9-]{36})$/i.exec(this.zoneID.zoneName)?.[1];
    const zoneName='FeiCloudAccess-v1'+(channel&&uuid.test(channel.toLowerCase())?'-QA-'+channel.toLowerCase():'');
    let reply;
    try{
      reply=check(await deadline(this.db.fetchRecords(['trial'],{zoneID:{zoneName},desiredKeys:['version']}),this.timeoutMs));
    }catch(error){
      verifyCurrent();
      if(['ZONE_NOT_FOUND','NOT_FOUND','UNKNOWN_ITEM'].includes(errorCode(error)))return null;
      throw error;
    }
    verifyCurrent();
    if(reply.records?.length===0)return null;
    const record=reply.records?.[0]??{};
    const creator=record.created?.userRecordName??record.created?.user;
    const startedAt=record.created?.timestamp;
    const expiresAt=startedAt+7*24*60*60*1000;
    if(reply.records?.length!==1||record.recordName!=='trial'||record.recordType!=='FeiCloudTrial'||record.fields?.version?.value!==1||
      (record.zoneID&&!ownedZone(record.zoneID,zoneName,owner))||
      ![owner,'_defaultOwner'].includes(creator)||!Number.isSafeInteger(expiresAt)||!Number.isSafeInteger(startedAt)||startedAt<=0)
      throw new Error('无法验证 iCloud 试用记录');
    return expiresAt;
  }
  async ensureZone(current=()=>true){
    const verifyCurrent=()=>this.#requireCurrent(current);
    verifyCurrent();
    if(this.zoneReady)return;
    const verify=async request=>{
      const reply=check(await deadline(request,this.timeoutMs));
      if(reply.zones?.length!==1||check(reply.zones[0]).zoneID?.zoneName!==this.zoneID.zoneName)
        throw new Error('CloudKit 未返回预期的私有记录区');
    };
    try{await verify(this.db.fetchRecordZones([this.zoneID]));}
    catch(e){
      if(this.readOnly||!['ZONE_NOT_FOUND','NOT_FOUND'].includes(errorCode(e)))throw e;
      verifyCurrent();
      try{await verify(this.db.saveRecordZones([this.zoneID]));}
      catch(error){
        if(errorCode(error)!=='EXISTS')throw error;
        verifyCurrent();
        await verify(this.db.fetchRecordZones([this.zoneID]));
      }
    }
    verifyCurrent();
    this.zoneReady=true;
  }
  async download(record,shared=false){
    if(record.deleted)throw new Error('同步历史记录被移除，需要重建本地同步状态');
    if(record.recordType!=='QuietNoteEdit')throw new Error('无法识别的 CloudKit 记录类型');
    const {note,digest,update}=record.fields;
    if(typeof note?.value!=='string'||typeof digest?.value!=='string'||!update?.value?.downloadURL)throw new Error('CloudKit 记录不完整');
    const response=await fetch(assetAddress(update.value.downloadURL),{credentials:'omit',redirect:'error',signal:AbortSignal.timeout(this.timeoutMs),...(shared?{cache:'no-store'}:{})});
    if(!response.ok)throw new Error('下载同步记录失败：'+response.status);
    const bytes=new Uint8Array(await response.arrayBuffer());
    if(!bytes.length||bytes.length>16*1024*1024)throw new Error('同步记录大小无效');
    const hash=bytesB64(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)));
    if(hash!==digest.value)throw new Error('CloudKit 记录校验失败');
    return {id:record.recordName,note:note.value,data:bytesB64(bytes)};
  }
  async upload(edit,current=()=>true){
    if(this.readOnly)throw new Error('此 Web 预览只允许读取 iCloud');
    const bytes=b64Bytes(edit.data),digest=bytesB64(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)));
    const record={recordName:edit.id,recordType:'QuietNoteEdit',fields:{note:{value:edit.note},digest:{value:digest,type:'BYTES'},update:{value:new Blob([bytes])}}};
    const active=()=>this.#requireCurrent(current,'上传');
    const mismatch=()=>new Error('同步记录 ID 冲突或归属不符');
    const exact=reply=>{
      const records=reply.records??[];
      if(!Array.isArray(records)||records.length>1)throw mismatch();
      const stored=records[0];
      if(!stored)return false;
      const {zoneID,recordType,recordName,fields,deleted}=stored;
      if(deleted||recordName!==edit.id||recordType&&recordType!==record.recordType||
        zoneID&&!ownedZone(zoneID,this.zoneID.zoneName,this.owner))throw mismatch();
      const note=fields?.note?.value,hash=fields?.digest?.value;
      if(note!=null&&note!==edit.note||hash!=null&&hash!==digest)throw mismatch();
      return recordType===record.recordType&&note===edit.note&&hash===digest;
    };
    active();
    try{
      const saved=check(await deadline(this.db.saveRecords([record],{zoneID:this.zoneID}),this.timeoutMs));
      active();
      if(exact(saved))return edit.id;
    }catch(e){
      // The request can succeed before the tab receives its acknowledgement.
      if(!['CONFLICT','EXISTS','ALREADY_EXISTS','SERVER_RECORD_CHANGED'].includes(errorCode(e)))throw e;
    }
    active();
    const existing=check(await deadline(this.db.fetchRecords([edit.id],{zoneID:this.zoneID,desiredKeys:['note','digest']}),this.timeoutMs));
    active();
    if(!exact(existing))throw new Error('iCloud 尚未确认这条记录，改动仍待同步');
    return edit.id;
  }
  async changes(cursor,{bootstrap=false,current=()=>true,scopes=null,restart=true,known,assetPaths}={}){
    const verifyCurrent=()=>this.#requireCurrent(current);
    const options={zoneID:this.zoneID,resultsLimit:bootstrap?200:50};if(cursor)options.syncToken=cursor;
    const readZone=async()=>{
      verifyCurrent();
      const response=check(await deadline(this.db.fetchRecordZoneChanges([options]),this.timeoutMs));
      verifyCurrent();
      if(response.zones?.length!==1)throw new Error('CloudKit 未返回预期的记录区');
      return check(response.zones[0]);
    };
    let zone;
    try{zone=await readZone();}
    catch(e){
      verifyCurrent();
      if(restart&&cursor&&String(errorCode(e)??'').includes('TOKEN'))return this.changes(null,{current,scopes,known,assetPaths});
      if(this.readOnly||!bootstrap||cursor||this.zoneReady||errorCode(e)!=='ZONE_NOT_FOUND')throw e;
      await this.ensureZone(current);
      zone=await readZone();
    }
    if(zone.zoneID?.zoneName&&zone.zoneID.zoneName!==this.zoneID.zoneName)throw new Error('CloudKit 返回了其它私有记录区');
    let owner=privateOwner(zone);
    if(bootstrap&&!owner){
      // CloudKit may omit the owner from changes/zone while returning it
      // from zones/lookup. Do this only after the first private pull.
      verifyCurrent();
      const metadata=check(await deadline(this.db.fetchRecordZones([this.zoneID]),this.timeoutMs));
      verifyCurrent();
      const matched=(metadata.zones||[]).filter(item=>item.zoneID?.zoneName===this.zoneID.zoneName);
      if(matched.length===1)owner=privateOwner(check(matched[0]));
      if(!owner)throw Object.assign(new Error('Missing private zone owner'),{userMessage:'CloudKit 未返回文档库所有者，无法安全打开云端缓存。'});
    }
    if(owner&&this.owner&&this.owner!==owner)
      throw Object.assign(new Error('CloudKit 返回了其它账号的文档库'),{code:'ACCOUNT_MISMATCH'});
    if(owner)this.owner=owner;
    this.zoneReady=true;
    // Physical cleanup changes the inventory. Document deletion arrives in the catalogue.
    const inventory=(zone.records||[]).filter(record=>!record.deleted&&record.recordName!==retentionFence);
    const records=inventory.filter(record=>(!scopes||scopes.includes(record.fields?.note?.value))&&
      needsAttachment(record,assetPaths)&&
      (!known?.[record.recordName]||known[record.recordName]!==digestHex(record)));
    if(records.some(record=>record.zoneID&&!ownedZone(record.zoneID,this.zoneID.zoneName,this.owner)))
      throw new Error('CloudKit 返回了其它私有记录区的记录');
    const edits=[];
    for(let i=0;i<records.length;i+=4){
      verifyCurrent();
      edits.push(...await Promise.all(records.slice(i,i+4).map(record=>this.download(record))));
    }
    verifyCurrent();
    return {edits,cursor:zone.syncToken,more:!!zone.moreComing,owner};
  }
  async readDocumentScope(note,current,known,assetPaths){
    if(!uuid.test(note)||note===catalogueScope)throw new Error('文档范围不匹配');
    const edits=[],seen=new Set();
    let cursor;
    for(let page=0;page<200;page++){
      const result=await this.changes(cursor,{current,scopes:[note],restart:false,known,assetPaths});
      edits.push(...result.edits);
      if(!result.more)return edits;
      if(!result.cursor||seen.has(result.cursor))throw new Error('文档历史分页没有继续');
      cursor=result.cursor;
      seen.add(cursor);
    }
    throw new Error('同步记录较多，请再次打开文档以继续');
  }
}
