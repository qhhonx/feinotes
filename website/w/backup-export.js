// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors.
import {writeMacBackup} from './backup-check.js';
import {sqliteEngine} from './backup-preview.js';
import {b64Bytes} from './storage.js';

const utf8=new TextEncoder();

function database(sqlite,snapshot,marker){
  const db=new sqlite.oo1.DB(':memory:');
  try{
    db.transaction(()=>{
      db.exec(snapshot.schema.join('\n'));
      db.exec(`PRAGMA user_version=${snapshot.version}`);
      const value=db.prepare('INSERT INTO library_values VALUES (?, ?)');
      value.bind(['restored',marker]).stepReset();
      value.bind(['settings',utf8.encode(snapshot.settings)]).stepReset();
      value.bind(['preferences',utf8.encode(snapshot.preferences)]).stepReset();
      const live=db.prepare('INSERT INTO library_entries VALUES (?, ?)');
      const trash=db.prepare('INSERT INTO library_trash VALUES (?, ?)');
      const document=db.prepare(`INSERT INTO library_documents
        (id,trashed,header,front,title,icon,plain,reminders,links) VALUES (?,?,?,'',?,?,'',?,?)`);
      const block=db.prepare('INSERT INTO library_blocks VALUES (?, ?, ?, ?, ?)');
      const search=db.prepare('INSERT INTO library_search_blocks VALUES (?, ?, ?)');
      for(const {id,meta,trashed,body} of snapshot.entries){
        (trashed?trash:live).bind([id,meta]).stepReset();
        if(!body)continue;
        document.bind([id,Number(trashed),body.header,body.title,body.icon,body.reminders,body.links]).stepReset();
        for(const row of body.blocks){
          block.bind([id,BigInt(row.id),row.parent===null?null:BigInt(row.parent),row.position,row.payload]).stepReset();
        }
        for(const [i,text] of body.search.entries())search.bind([id,i,text]).stepReset();
      }
      for(const stmt of [value,live,trash,document,block,search])stmt.finalize();
    });
    return sqlite.capi.sqlite3_js_db_export(db.pointer);
  }finally{db.close();}
}

export async function createLocalBackup(snapshot,codec){
  const marker=utf8.encode('local-only\n');
  const sqlite=await sqliteEngine();
  const members=new Map([['library.sqlite3',database(sqlite,snapshot,marker)],['.cloud-restore-ready',marker]]);
  for(const [path,base64] of Object.entries(snapshot.assets))members.set(path,b64Bytes(base64));
  for(const [path,text] of Object.entries(snapshot.history))members.set(path,utf8.encode(text));
  return writeMacBackup(members,codec);
}
