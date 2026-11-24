const PREFIX='feinotes-web:'+self.registration.scope+':';
const CACHE=PREFIX+'676c5645eaf01f96';
const SHELL=[["./", "sha256-P5nDr6Z7ZodP98I6xWq8IjpO/A8+cKbr12Jtm9ss+zU="], ["./index.html", "sha256-P5nDr6Z7ZodP98I6xWq8IjpO/A8+cKbr12Jtm9ss+zU="], ["./style.css", "sha256-beabWRbxdLvv309uSBGyBuheYVLvklpewVFo3+TzSC4="], ["./main.js", "sha256-n0lq16yBkBwBIExuz04H8rL+YqlHyMYPysaCD3fuVhY="], ["./object-link.js", "sha256-gf9tP+TSF3Pzep//iYwMQAQ2TfGmJV9uJS2YnVvRcko="], ["./storage.js", "sha256-qsRgUy6WqSkpvw3+Z2JQCUCWJU71Vs53OQDD++BzPck="], ["./cloudkit.js", "sha256-r4gQ6chY9Vnhv7/lXNq0h6lGU8rFXPCegVo9NRYQPHE="], ["./cloud-auth.js", "sha256-f4BKw0K6q0gpUXAdr+OyKpAh02GX2bvTOeNtHgNNDF0="], ["./backup-check.js", "sha256-dZVCaWh6dEbofi2znAJH/aPSBnZLsN5e4sRvUIL/ZMw="], ["./fei-notes.png", "sha256-xmMNM6s+tH4Jg8l03u6nvVH2r8zBIqfmfjkJj06eJmc="], ["./wasm/feinotes.js", "sha256-uAukDRuPjwIbnhP7d4XHgqltH7U1J5C4qWxoaH8ip24="], ["./wasm/feinotes_bg.wasm.gz", "sha256-HbnNiydoNBGd4xW+vTPtrd9CbMoKeZzchiyTfb6rQqc="], ["./attachment-preview.js", "sha256-d1jY0JZSx/tGu+JccgvbkBc/2Nez6FyD4iMaF1hlu28="], ["./cloud-session.js", "sha256-WFaXb/j1kAKm7umqRROiqD5kGaISHpmvn8Sw+GKZg/w="], ["./shared-presence.js", "sha256-6fcbFvbdPpZmFnxB5N/2oZYD039dy1vhr0ZKqp1ZfJY="], ["./shared-write.js", "sha256-XJdYmcNOl6M85z6WttSMwdqcxEekA/G5A7axYi/ie44="], ["./backup-preview.js", "sha256-7PvifOv/rvgwO1GmD9Hsoueni+nLWiAvpPw0s2oQz+8="], ["./backup-export.js", "sha256-PLuk0D+cfqi+mYCLD0zz/TkLUw+0+O7Xj/bQkhOWuh4="], ["./backup-view.html", "sha256-D444Cz33gVs355ZIP5E3waYB9eLupUA8XuWueZNjg8I="], ["./backup-view.js", "sha256-YrDYFDZ1bLiuwmhX8knluBQ7xOYRgPTgEsXtbXq7KAQ="], ["./sqlite/sqlite3.mjs", "sha256-f2YSD1u3nQZGr4j1wnUQ7SbXMegB2W5AJNJIeN7FqGo="], ["./sqlite/sqlite3.wasm", "sha256-EP1yezeSR1XtwSwvPq0NVBVdjy2fKr9qHvmysll+ClE="], ["./wasm/snippets/quiet_md-5437665c0d761623/packaging/web/www/bridge.js", "sha256-0K70AC7xigJN6pIGVAc8wCszz8PIYBJQEDz++lyaDUU="]];
const manifest=new Map(SHELL.map(([path,integrity])=>[new URL(path,self.registration.scope).href,integrity]));
const assetRequest=(url,integrity)=>new Request(url,{cache:'reload',integrity});
self.addEventListener('install',event=>event.waitUntil((async()=>{
  const cache=await caches.open(CACHE);
  try{await cache.addAll([...manifest].map(([url,integrity])=>assetRequest(url,integrity)));}
  catch(error){await caches.delete(CACHE);throw error;}
})()));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k.startsWith(PREFIX)&&k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{
  if(event.request.method!=='GET')return;
  const url=new URL(event.request.url);
  url.search='';
  const integrity=manifest.get(url.href);
  if(!integrity)return;
  event.respondWith((async()=>{
    const cache=await caches.open(CACHE);
    const stored=await cache.match(url.href);
    if(stored)return stored;
    const request=assetRequest(url.href,integrity);
    const response=await fetch(request);
    if(response.ok)await cache.put(request,response.clone());
    return response;
  })());
});
