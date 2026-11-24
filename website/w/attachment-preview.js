import {b64Bytes} from './storage.js';

export function download(name,content,mime){
  const url=URL.createObjectURL(new Blob([content],{type:mime}));
  const anchor=document.createElement('a');
  anchor.href=url;anchor.download=name;anchor.click();
  setTimeout(()=>URL.revokeObjectURL(url),1000);
}

export function createAttachmentPreview(previewDialog){
  const previewContent=previewDialog.querySelector('#attachment-preview-content');
  let attachmentPreview;
  function clearAttachmentPreview(){
    previewContent.replaceChildren();
    if(attachmentPreview?.url)URL.revokeObjectURL(attachmentPreview.url);
    attachmentPreview=null;
  }
  function closeAttachmentPreview(){
    if(previewDialog.open)previewDialog.close();
    clearAttachmentPreview();
  }
  previewDialog.addEventListener('close',()=>{if(!previewDialog.open)clearAttachmentPreview();});
  previewDialog.querySelector('#attachment-preview-download').onclick=()=>{
    if(!attachmentPreview)return;
    const {name,bytes}=attachmentPreview;
    download(name.split(/[\\/]/).pop()||'Attachment',bytes,'application/octet-stream');
  };
  function showAttachmentPreview({name,content,mime,shared}){
    closeAttachmentPreview();
    const bytes=b64Bytes(content);
    attachmentPreview={name,bytes,shared};
    previewDialog.querySelector('#attachment-preview-title').textContent=name;
    const fallback=()=>{previewContent.textContent='此附件暂时无法预览，可以下载后打开。';};
    if(mime==='text/plain'){
      const text=document.createElement('pre');
      const limit=1024*1024;
      try{
        text.textContent=new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(0,limit),{stream:bytes.length>limit});
        if(bytes.length>limit)text.textContent+='\n\n仅显示前 1 MB';
        previewContent.append(text);
      }catch{fallback();}
    }else{
      let element;
      if(mime==='application/pdf'&&navigator.pdfViewerEnabled&&new TextDecoder().decode(bytes.subarray(0,5))==='%PDF-'){
        element=document.createElement('embed');
        element.type=mime;
        element.title=name;
      }else if(mime.startsWith('image/')){
        element=document.createElement('img');
        element.alt=name;
      }else if(mime.startsWith('audio/')||mime.startsWith('video/')){
        element=document.createElement(mime.startsWith('audio/')?'audio':'video');
        element.controls=true;
        element.preload='metadata';
        if(!element.canPlayType(mime))element=undefined;
      }
      if(element){
        attachmentPreview.url=URL.createObjectURL(new Blob([bytes],{type:mime}));
        element.addEventListener('error',()=>{if(previewContent.contains(element))fallback();},{once:true});
        element.src=attachmentPreview.url;
        previewContent.append(element);
      }else fallback();
    }
    previewDialog.showModal();
  }
  return {show:showAttachmentPreview,close:closeAttachmentPreview,isShared:()=>Boolean(attachmentPreview?.shared)};
}
