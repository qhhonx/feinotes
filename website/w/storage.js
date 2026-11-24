export function openDatabase() {
  return new Promise((resolve,reject)=>{
    const req=indexedDB.open('feinotes-browser-mvp:'+new URL('.',import.meta.url).pathname,1);
    req.onupgradeneeded=()=>req.result.createObjectStore('libraries');
    req.onerror=()=>reject(req.error); req.onsuccess=()=>resolve(req.result);
  });
}
export async function readLibrary(db,key) {
  return new Promise((resolve,reject)=>{
    const tx=db.transaction('libraries','readonly'),req=tx.objectStore('libraries').get(key);
    req.onsuccess=()=>resolve(req.result||''); req.onerror=()=>reject(req.error);
  });
}
export async function writeLibrary(db,key,value) {
  return saveLibraryRecords(db,[[key,value]],false);
}
function saveLibraryRecords(db,records,create) {
  return new Promise((resolve,reject)=>{
    const tx=db.transaction('libraries','readwrite',{durability:'strict'});
    const store=tx.objectStore('libraries');
    for(const [key,value] of records){
      if(create)store.add(value,key);
      else store.put(value,key);
    }
    tx.oncomplete=()=>resolve(); tx.onerror=()=>reject(tx.error); tx.onabort=()=>reject(tx.error||new Error('本地保存中止'));
  });
}
export function saveRestoredLibrary(db,key,state,details) {
  return saveLibraryRecords(db,[[key,state],['metadata:'+key,details]],true);
}
export function listRestoredLibraries(db,prefix) {
  const key='metadata:'+prefix;
  return new Promise((resolve,reject)=>{
    const request=db.transaction('libraries','readonly').objectStore('libraries')
      .getAll(IDBKeyRange.bound(key,key+'\uffff'));
    request.onsuccess=()=>resolve(request.result);
    request.onerror=()=>reject(request.error);
  });
}
export function b64Bytes(value) {
  const bin=atob(value); return Uint8Array.from(bin,c=>c.charCodeAt(0));
}
export function bytesB64(value) {
  let s=''; for(let i=0;i<value.length;i+=32768)s+=String.fromCharCode(...value.subarray(i,i+32768)); return btoa(s);
}
