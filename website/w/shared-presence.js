// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import {b64Bytes} from './storage.js';

const ttl = 24_000;
const heartbeat = 8_000;
const segments = new Intl.Segmenter(undefined, {granularity:'grapheme'});
const peerName = value => typeof value==='string'&&/^(?:0|[1-9][0-9]{0,19})$/.test(value)&&BigInt(value)<=0xffff_ffff_ffff_ffffn;
const codes = error => [error?.serverErrorCode,error?.ckErrorCode];

export function createPresencePublisher(now=()=>performance.now()) {
  let desired, attempted, running;
  async function flush() {
    try {
      while(true) {
        const next=desired;
        if(attempted&&attempted.write!==next?.write){
          const previous=attempted;
          attempted=undefined;
          try{await previous.write(null);}catch{}
          continue;
        }
        if(!next||attempted&&(now()<attempted.retryAfter||
          attempted.state===next.state&&now()-attempted.when<heartbeat))return;
        attempted={...next,when:now(),retryAfter:0};
        try{await next.write(next.state);}
        catch{attempted.retryAfter=now()+heartbeat;}
      }
    }finally{running=undefined;}
  }
  return (write,state)=>{
    desired=write&&state?{write,state}:undefined;
    return running??=Promise.resolve().then(flush);
  };
}

function activity(record, account, note) {
  const author = record.created?.userRecordName;
  const changedAt = record.modified?.timestamp;
  const age = Date.now()-changedAt;
  if(record.deleted||record.recordType!=='QuietPresence'||!author||author===account||author==='_defaultOwner'
     ||author!==record.modified?.userRecordName||!Number.isSafeInteger(changedAt)||age>=ttl||age< -ttl)return;
  const field = record.fields?.state;
  if(field?.type!=='BYTES'||typeof field.value!=='string'||field.value.length>5464)return;
  try {
    const bytes=b64Bytes(field.value);
    if(bytes.length>4096)return;
    const state=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
    if(state?.note!==note)return;
    // Keep the native bytes. A block ID can exceed the exact range of JS numbers.
    const selection=state.anchor&&state.head?field.value:null;
    return {author,selection,until:performance.now()+ttl-Math.max(0,age)};
  }catch{}
}

function displayName(member) {
  const name = [member.userIdentity.nameComponents?.givenName,member.userIdentity.nameComponents?.familyName]
    .filter(part=>typeof part==='string').join(' ').trim();
  if(!name||/[@\p{Cc}\u202a-\u202e\u2066-\u2069]/u.test(name))return '';
  return [...segments.segment(name.slice(0,512))].slice(0,20).map(item=>item.segment).join('');
}

// Only the display name and native selection payload reach the editor.
export function createPresenceReader({readPage, readShare, available, note, account}) {
  let cursor, devices=new Map(), members=new Map();
  const aliases=new Map();
  function visible() {
    if(!available())return [];
    const people=new Map(), time=performance.now();
    for(const {author,selection,until} of devices.values()){
      if(until<=time||!members.has(author))continue;
      if(!people.has(author)){
        if(!aliases.has(author))aliases.set(author,aliases.size+1);
        people.set(author,{name:members.get(author),alias:aliases.get(author)});
      }
      if(selection)(people.get(author).selections??=[]).push(selection);
    }
    return [...people.values()].sort((a,b)=>a.alias-b.alias);
  }
  return {visible, async refresh() {
    if(!available())return [];
    let pending=cursor, updated=new Map(devices);
    try {
      for(let attempt=0;attempt<2;attempt++){
        const seen=new Set([pending]);
        try {
          for(let page=0;page<200;page++){
            const result=await readPage(pending);
            if(!available())return [];
            for(const record of result.records??[]){
              if(!peerName(record.recordName))continue;
              const active=activity(record,account,note);
              if(active)updated.set(record.recordName,active);
              else updated.delete(record.recordName);
            }
            if(!result.syncToken||result.moreComing&&seen.has(result.syncToken))throw new Error('共享状态分页没有继续');
            pending=result.syncToken;
            if(!result.moreComing)break;
            seen.add(pending);
            if(page===199)throw new Error('共享状态记录较多');
          }
          break;
        }catch(error){
          if(attempt||!cursor||!codes(error).some(code=>typeof code==='string'&&code.includes('TOKEN')))throw error;
          pending=undefined;
          updated=new Map();
        }
      }
      for(const [peer,device] of updated)if(device.until<=performance.now())updated.delete(peer);
      const share=updated.size?await readShare():null;
      if(!available())return [];
      members=new Map((share?.participants??[]).filter(member=>{
        return member?.userIdentity?.userRecordName&&(member.type==='OWNER'||
          member.acceptanceStatus==='ACCEPTED'&&member.permission==='READ_WRITE');
      }).map(member=>[member.userIdentity.userRecordName,displayName(member)]));
      for(const [peer,device] of updated)if(!members.has(device.author))updated.delete(peer);
      cursor=pending;
      devices=updated;
      return visible();
    }catch(error){
      devices.clear();
      members.clear();
      cursor=undefined;
      throw error;
    }
  }};
}
