// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import {b64Bytes,bytesB64} from './storage.js';

export const retentionFence='FeiNotes-retention-v1';
const editType='QuietNoteEdit';
const assetScope='00000000-0000-0000-0000-000000000002';
const uuid=/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i;
const missing=new Set(['NOT_FOUND','UNKNOWN_ITEM']);
const conflicting=new Set(['CONFLICT','EXISTS','ALREADY_EXISTS','SERVER_RECORD_CHANGED','NOT_FOUND','UNKNOWN_ITEM']);
const batchErrors=new Set(['BATCH_REQUEST_FAILED','ATOMIC_ERROR']);
const matchesCode=(error,codes)=>[error?.serverErrorCode,error?.ckErrorCode].some(code=>codes.has(code));

function assert(condition,message){if(!condition)throw new Error(message);}

function errors(response){
  const failures=response.errors??[];
  if(failures.length)return failures;
  if(response.serverErrorCode||response.hasErrors)return [response];
  return [];
}

function checked(response){
  const failures=errors(response);
  if(failures.length)throw failures[0];
  return response;
}

function assetIndex(bytes){
  const end=bytes.subarray(0,4096).indexOf(10);
  assert(end>0,'Shared asset index is missing');
  const header=bytes.subarray(0,end);
  const {path,digest,size,part}=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(header));
  assert(typeof path==='string'&&path.startsWith('assets/')&&typeof digest==='string'&&
    Number.isSafeInteger(size)&&size>=0&&size<=64*1024*1024&&
    Number.isSafeInteger(part)&&part>=0&&part<Math.max(1,Math.ceil(size/1048576)), 'Invalid shared asset index');
  return {value:bytesB64(header),type:'BYTES'};
}

// The expected generation comes from the same completed pull used to merge the
// room, not a separate fetch taken immediately before the write.
export async function commitSharedEdits({db,zoneID,matchesZone,expectedGeneration,available},edits){
  const refresh={uploaded:[],needsRefresh:true};
  const checkAvailable=()=>assert(available(),'Shared write access changed');
  // The request is already bound to the verified zone. Apple can omit zoneID
  // from individual records in that response, including the generation record.
  const inZone=record=>assert(record&&(!record.zoneID||matchesZone(record.zoneID)),'CloudKit returned a different shared zone');
  const sameEdit=(stored,planned)=>{
    inZone(stored);
    assert(stored.recordType===editType&&stored.recordName===planned.recordName&&
      stored.fields?.note?.value===planned.fields.note.value&&stored.fields?.digest?.value===planned.fields.digest.value,
    'Shared record does not match the queued edit');
  };
  const lookup=async name=>{
    checkAvailable();
    const result=await db.fetchRecords(name,{zoneID,desiredKeys:['note','digest']});
    checkAvailable();
    const failures=errors(result);
    if(failures.length===1&&matchesCode(failures[0],missing)){assert(!result.records?.length,'Inconsistent record lookup');return null;}
    checked(result);
    assert(result.records?.length===1&&result.records[0].recordName===name,'Invalid shared record lookup');
    inZone(result.records[0]);
    return result.records[0];
  };

  checkAvailable();
  assert(expectedGeneration===null||uuid.test(expectedGeneration),'Missing shared generation');
  assert(edits.length>0&&edits.length<=20&&new Set(edits.map(edit=>edit.id)).size===edits.length,'Invalid shared edit batch');
  const records=await Promise.all(edits.map(async edit=>{
    assert(uuid.test(edit.id)&&uuid.test(edit.note),'Invalid shared record or document ID');
    const bytes=b64Bytes(edit.data);
    assert(bytes.length>0&&bytes.length<=16*1024*1024,'Invalid shared edit size');
    const digest=bytesB64(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)));
    return {recordName:edit.id,recordType:editType,fields:{
      note:{value:edit.note},digest:{value:digest,type:'BYTES'},update:{value:new Blob([bytes])},
      ...(edit.note===assetScope?{assetIndex:assetIndex(bytes)}:{}),
    }};
  }));
  checkAvailable();
  const zones=checked(await db.fetchRecordZones([zoneID]));
  checkAvailable();
  assert(zones.zones?.length===1&&zones.zones[0].atomic===true&&matchesZone(zones.zones[0].zoneID),'Shared zone does not confirm atomic writes');
  const oldFence=await lookup(retentionFence);
  const generation=oldFence?.fields?.note?.value??null;
  if(oldFence)assert(oldFence.recordType===editType&&uuid.test(generation)&&
    typeof oldFence.recordChangeTag==='string'&&oldFence.recordChangeTag,'Invalid CloudKit shared generation record');

  const fresh=[];
  for(const record of records){
    const existing=await lookup(record.recordName);
    if(existing)sameEdit(existing,record);
    else fresh.push(record);
  }
  if(!fresh.length)return {uploaded:edits.map(edit=>edit.id),needsRefresh:generation!==expectedGeneration};
  if(generation!==expectedGeneration)return refresh;

  const nextGeneration=crypto.randomUUID();
  const fence={recordName:retentionFence,recordType:editType,fields:{note:{value:nextGeneration}}};
  checkAvailable();
  const batch=db.newRecordsBatch({zoneID,atomic:true}).create(fresh);
  if(oldFence)batch.update({...fence,recordChangeTag:oldFence.recordChangeTag});
  else batch.create(fence);
  const result=await batch.commit();
  checkAvailable();
  const failures=errors(result);
  if(failures.some(error=>matchesCode(error,conflicting))&&
    failures.every(error=>matchesCode(error,conflicting)||matchesCode(error,batchErrors)))return refresh;
  checked(result);
  const confirmed=result.records?.find(record=>record.recordName===retentionFence);
  assert(result.records?.length===fresh.length+1&&confirmed?.fields?.note?.value===nextGeneration,
    'CloudKit did not confirm the atomic shared write');
  inZone(confirmed);
  for(const planned of fresh){
    sameEdit(result.records.find(record=>record.recordName===planned.recordName),planned);
    sameEdit(await lookup(planned.recordName),planned);
  }
  return {uploaded:edits.map(edit=>edit.id),needsRefresh:true};
}
