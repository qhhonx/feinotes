export class BackupValidationError extends Error {}

const utf8=new TextDecoder('utf-8',{fatal:true});
const magic=new TextEncoder().encode('Fei Notes Backup\n');
const same=(left,right)=>left.length===right.length&&left.every((byte,index)=>byte===right[index]);
const chunkSize=1024*1024;

export function writeMacBackup(members,{BackupHash,valid_backup_path}){
  if(members.size>100000)throw new BackupValidationError('备份文件清单超出格式限制');
  const parts=[magic,Uint8Array.of(2)];
  const utf8=new TextEncoder();
  const item=new BackupHash(),manifest=new BackupHash();
  try{
    for(const [path,bytes] of members){
      if(!valid_backup_path(path))throw new BackupValidationError('备份含有无效的文件路径');
      const name=utf8.encode(path),length=new Uint8Array(4),size=new Uint8Array(8);
      new DataView(length.buffer).setUint32(0,name.length,true);
      new DataView(size.buffer).setBigUint64(0,BigInt(bytes.length),true);
      for(let offset=0;offset<bytes.length;offset+=chunkSize)item.update(bytes.subarray(offset,offset+chunkSize));
      const digest=item.finish();
      for(const metadata of [length,name,size,digest])manifest.update(metadata);
      parts.push(length,name,size,bytes,digest);
    }
    parts.push(new Uint8Array(4),manifest.finish());
    return new Blob(parts,{type:'application/octet-stream'});
  }finally{item.free();manifest.free();}
}

export async function inspectMacBackup(file,{BackupHash,valid_backup_path},current=()=>true){
  let offset=0,version,kind,localOnly=false,assets=0,history=0;
  const names=new Map(),shared=new Set();
  const item=new BackupHash(),manifest=new BackupHash();
  const take=async length=>{
    if(!current())throw new DOMException('已取消检查','AbortError');
    if(!Number.isSafeInteger(length)||length>file.size-offset)throw new BackupValidationError('备份文件不完整');
    const bytes=new Uint8Array(await file.slice(offset,offset+length).arrayBuffer());
    offset+=length;
    return bytes;
  };
  try{
    const signature=await take(magic.length+1);
    version=signature.at(-1);
    if(!same(signature.subarray(0,-1),magic)||![1,2].includes(version))throw new BackupValidationError('无法识别此飞记备份格式');
    while(true){
      const nameSize=await take(4);
      const length=new DataView(nameSize.buffer).getUint32(0,true);
      if(length===0)break;
      if(length>4096||names.size>=100000)throw new BackupValidationError('备份文件清单超出格式限制');
      const encodedName=await take(length);
      const name=utf8.decode(encodedName);
      if(!valid_backup_path(name)||names.has(name))throw new BackupValidationError('备份含有无效或重复的文件路径');
      const parts=name.split('/');
      const sizeBytes=await take(8);
      const size=Number(new DataView(sizeBytes.buffer).getBigUint64(0,true));
      if(!Number.isSafeInteger(size)||size>file.size-offset-32)throw new BackupValidationError('备份文件大小不正确');
      names.set(name,{offset,size});
      for(let left=size;left>0;){
        const bytes=await take(Math.min(left,chunkSize));
        if(left===size&&name==='library.sqlite3'&&utf8.decode(bytes.subarray(0,16))!=='SQLite format 3\0'){
          throw new BackupValidationError('无法识别备份中的文档库');
        }
        if(name==='.cloud-restore-ready'){
          if(size>1024)throw new BackupValidationError('备份中的文档库标识无效');
          const [owner,scope]=utf8.decode(bytes).split(/\r?\n/);
          if(scope==='shared'||!(/^[a-f0-9]{64}$/i.test(owner)||owner==='local-only'))throw new BackupValidationError('备份中的文档库标识无效');
          localOnly=owner==='local-only';
          kind=localOnly?'此设备的文档库':'iCloud 文档库';
        }
        item.update(bytes);
        left-=bytes.length;
      }
      if(name==='library.sqlite3'&&size<100)throw new BackupValidationError('备份中的文档库不完整');
      const expected=await take(32);
      if(!same(item.finish(),expected))throw new BackupValidationError('备份中的文件校验失败');
      for(const bytes of [nameSize,encodedName,sizeBytes,expected])manifest.update(bytes);
      const fromShare=parts[0]==='shared-notes';
      if(fromShare)shared.add(parts[1]);
      const local=parts.slice(fromShare?2:0);
      if(local[0]==='assets')assets++;
      if(local[0]==='.quiet'&&['history','history-trash'].includes(local[1])&&/\.(md|xml)$/.test(local.at(-1)))history++;
    }
    const expected=await take(32);
    if(!same(manifest.finish(),expected)||offset!==file.size)throw new BackupValidationError('备份文件清单校验失败');
    if(!kind||!names.has('library.sqlite3'))throw new BackupValidationError('备份缺少文档库文件或标识');
    for(const name of names.keys()){
      for(let index=name.indexOf('/');index>=0;index=name.indexOf('/',index+1)){
        if(names.has(name.slice(0,index)))throw new BackupValidationError('备份含有冲突的文件路径');
      }
    }
    for(const id of shared){
      const prefix='shared-notes/'+id+'/';
      if(!['library.sqlite3','.cloud-restore-ready','shared-note.json'].every(name=>names.has(prefix+name)))throw new BackupValidationError('备份中的共享文档库不完整');
    }
    return {version,kind,localOnly,files:names.size,assets,history,shared:shared.size,entries:names};
  }finally{item.free();manifest.free();}
}
