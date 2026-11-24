import * as wasm from './wasm/feinotes.js';
import {loadWorkspaceEngine} from './workspace-engine.js';
import {b64Bytes} from './storage.js';
import {createAttachmentPreview,download} from './attachment-preview.js';

const loading=document.getElementById('loading');
if(window.parent===window)loading.textContent='请从飞记的文档库设置打开备份预览。';

window.openBackupPreview=async state=>{
  delete window.openBackupPreview;
  await loadWorkspaceEngine();
  const preview=createAttachmentPreview(document.getElementById('attachment-preview'));
  addEventListener('feinotes',({detail})=>{
    if(detail.type==='ready')loading.remove();
    else if(detail.type==='preview-attachment')preview.show(JSON.parse(detail.data));
    else if(detail.type==='download-attachment'){
      const {name,content}=JSON.parse(detail.data);
      download(name,b64Bytes(content),'application/octet-stream');
    }
  });
  wasm.start(state,false,'');
};
