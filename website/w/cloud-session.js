const apiOrigin='https://api.apple-cloudkit.com';
const clientIdPattern=/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i;
const ownerPattern=/^_[a-f0-9]{16,128}$/i;
const validToken=value=>typeof value==='string'&&value.length>0&&value.length<=8192;

// The browser store is scoped by origin and application path. The entry also
// binds the session to the CloudKit container, environment, library and owner.
export function sessionScope({containerIdentifier,environment,zoneName}){
  return JSON.stringify([containerIdentifier,environment,zoneName]);
}

export function createCloudSessionVault(storage,locks,key){
  function read(){
    try{
      const entry=JSON.parse(storage.getItem(key));
      if(typeof entry?.scope==='string'&&clientIdPattern.test(entry.clientId)&&
        ownerPattern.test(entry.owner)&&validToken(entry.session))return entry;
    }catch{}
    return null;
  }
  function save(entry){
    try{storage.setItem(key,JSON.stringify(entry));return true;}
    catch{return false;}
  }
  function clear(clientId){
    // Leave a newer account connection alone.
    try{if(clientId&&read()?.clientId===clientId)storage.removeItem(key);}catch{}
  }
  function connect(entry,previous,active){
    let current={...entry};
    let remembered=!!entry.owner;
    const matches=saved=>saved?.clientId===current.clientId&&saved.scope===current.scope&&saved.owner===current.owner;
    function check(){
      if(!active()||(remembered&&!matches(read())))
        throw new Error('此浏览器的 iCloud 会话已变化');
    }
    async function request(url,options,fetcher){
      if(url.origin!==apiOrigin||!url.searchParams.has('ckWebAuthToken'))
        throw new Error('iCloud 会话只用于 Apple 的授权请求');
      return locks.request(key,{signal:AbortSignal.timeout(20000)},async()=>{
        check();
        if(remembered)current=read();
        url.searchParams.set('ckWebAuthToken',current.session);
        const timeout=AbortSignal.timeout(18000);
        const signal=options.signal?AbortSignal.any([options.signal,timeout]):timeout;
        const response=await fetcher(url,{...options,signal});
        check();
        // Commit the rotation before another tab can make an authenticated
        // request. A response arriving after logout cannot recreate the entry.
        const rotated=response.headers.get('x-apple-cloudkit-web-auth-token')??
          response.headers.get('x-apple-cloudkit-session');
        if(response.ok&&validToken(rotated)&&rotated!==current.session){
          const updated={...current,session:rotated};
          if(remembered&&!save(updated)){
            clear(current.clientId);
            remembered=false;
          }
          current=updated;
        }
        return response;
      });
    }
    async function remember(owner){
      return locks.request(key,()=>{
        check();
        if(!ownerPattern.test(owner)||(current.owner&&current.owner!==owner)){
          clear(current.clientId);
          throw new Error('iCloud 文档库账号与本机保存的会话不一致');
        }
        if(remembered)return true;
        if(read()?.clientId!==previous)
          throw new Error('另一个标签页已经连接 iCloud');
        current={...current,owner};
        remembered=save(current);
        return remembered;
      });
    }
    return {request,remember};
  }
  return {read,clear,connect};
}
