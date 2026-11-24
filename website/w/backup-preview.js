// Only verified members of the personal library enter the preview.
import initSqlite from './sqlite/sqlite3.mjs';

const MAX_PREVIEW_BYTES=128*1024*1024;
const utf8=new TextDecoder('utf-8',{fatal:true});
const catalogue='00000000-0000-0000-0000-000000000001';
let engine;

export function sqliteEngine(){
  engine??=initSqlite().catch(error=>{engine=undefined;throw error;});
  return engine;
}

function base64(bytes){
  const chunks=[];
  for(let i=0;i<bytes.length;i+=16384)chunks.push(String.fromCharCode(...bytes.subarray(i,i+16384)));
  return btoa(chunks.join(''));
}

function readSource(sqlite,bytes,marker){
  const db=new sqlite.oo1.DB(':memory:');
  let ptr;
  try {
    ptr=sqlite.wasm.allocFromTypedArray(bytes);
    db.checkRc(sqlite.capi.sqlite3_deserialize(db.pointer,'main',ptr,bytes.length,bytes.length,
      sqlite.capi.SQLITE_DESERIALIZE_READONLY));
    db.exec('PRAGMA trusted_schema=OFF; PRAGMA query_only=ON');
    const version=db.selectValue('PRAGMA user_version');
    if(version<2||version>7)throw new Error('此文档库需要不同版本的飞记。');
    if(db.selectValue('PRAGMA integrity_check')!=='ok')throw new Error('备份的数据库完整性检查失败。');
    if(db.selectValue(`SELECT count(*) FROM sqlite_schema
      WHERE type NOT IN ('table','index') OR sql LIKE 'CREATE VIRTUAL TABLE%'`)!==0)
      throw new Error('无法读取此备份的数据库结构。');
    if(utf8.decode(db.selectValue("SELECT value FROM library_values WHERE key='restored'"))!==marker)
      throw new Error('备份的文档库标识不一致。');

    const documents=new Map(db.selectObjects('SELECT id,header,trashed FROM library_documents')
      .map(({id,header,trashed})=>[id,{header,trashed,blocks:[]}]));
    for(const {document,...block} of db.selectObjects(`SELECT document,CAST(block AS TEXT) AS id,
      CAST(parent AS TEXT) AS parent,payload FROM library_blocks ORDER BY document,parent,position`)){
      const owner=documents.get(document);
      if(!owner)throw new Error('备份含有无法定位的内容块。');
      owner.blocks.push(block);
    }
    const entries=[];
    for(const {id,meta} of db.selectObjects('SELECT id,meta FROM library_entries')){
      if(documents.get(id)?.trashed)throw new Error('备份的文档目录状态不一致。');
      entries.push({id,meta:JSON.parse(meta),document:documents.get(id)});
    }
    for(const {id,meta} of db.selectObjects('SELECT id,meta FROM library_trash')){
      if(documents.has(id)&&!documents.get(id).trashed)throw new Error('备份的回收站状态不一致。');
      const trash=JSON.parse(meta);
      entries.push({id,meta:trash.meta,trash,document:documents.get(id)});
    }
    if(marker.split(/\r?\n/,1)[0]!=='local-only') {
      const original=db.selectValue("SELECT value FROM state WHERE key='catalogue'");
      const snapshots=Object.fromEntries(db.selectObjects('SELECT id,snapshot FROM notes')
        .map(({id,snapshot})=>[id,base64(snapshot)]));
      if(original)snapshots[catalogue]=base64(original);
      const updates=db.selectObjects('SELECT note,data FROM note_local_updates ORDER BY rowid')
        .map(({note,data})=>({note,data:base64(data),id:crypto.randomUUID()}));
      return {kind:'cloud',snapshots,updates,entries};
    }

    const value=key=>db.selectValue('SELECT value FROM library_values WHERE key=?',[key]);
    const settings=value('settings'),preferences=value('preferences');
    return {kind:'local',entries,settings:settings?JSON.parse(utf8.decode(settings)):undefined,
      preferences:preferences?utf8.decode(preferences):''};
  } finally {
    db.close();
    if(ptr)sqlite.wasm.dealloc(ptr);
  }
}

export async function readBackupPreview(file,archive,current){
  const check=()=>{
    if(!current())throw new DOMException('已关闭备份预览','AbortError');
  };
  const read=async ({offset,size})=>{
    check();
    const bytes=new Uint8Array(await file.slice(offset,offset+size).arrayBuffer());
    check();
    return bytes;
  };
  const members=[...archive.entries].filter(([path])=>path.startsWith('assets/')||
    path.startsWith('.quiet/history/')||path.startsWith('.quiet/history-trash/'));
  const index=archive.entries.get('library.sqlite3');
  if(index.size+members.reduce((total,[,member])=>total+member.size,0)>MAX_PREVIEW_BYTES)
    throw new Error('该备份的个人文档库超过当前浏览器预览支持的 128 MB。');
  const [sqlite,bytes,marker]=await Promise.all([sqliteEngine(),read(index),read(archive.entries.get('.cloud-restore-ready'))]);
  const source=readSource(sqlite,bytes,utf8.decode(marker));
  const assets=Object.create(null),versions=[];
  for(const [path,member] of members){
    if(path.startsWith('assets/')){
      assets[path]=base64(await read(member));
    }else if(!path.endsWith('.name')){
      const [, , id, name]=path.split('/');
      const label=archive.entries.get(path+'.name');
      versions.push({id,name,text:utf8.decode(await read(member)),label:label?utf8.decode(await read(label)).trim():null});
    }
  }
  return {source,assets,versions};
}
