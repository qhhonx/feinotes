import init from './wasm/feinotes.js';

export async function loadWorkspaceEngine(){
  const moduleFile=await fetch(new URL('./wasm/feinotes_bg.wasm.gz',import.meta.url));
  if(!moduleFile.ok||!moduleFile.body)throw new Error('无法下载飞记编辑器');
  const moduleStream=moduleFile.body.pipeThrough(new DecompressionStream('gzip'));
  await init({module_or_path:new Response(moduleStream,{headers:{'Content-Type':'application/wasm'}})});
}
