const apiOrigin='https://api.apple-cloudkit.com';
const relayOrigin='https://cdn.apple-cloudkit.com';
const loginOrigins=new Set(['https://idmsa.apple.com',relayOrigin]);
const challengeAge=75_000;

export class AppleSignIn {
  constructor(config, button, onSession, onStatus, text=value=>value) {
    this.config=config;
    this.button=button;
    this.onSession=onSession;
    this.onStatus=onStatus;
    this.text=text;
    this.clientId=crypto.randomUUID();
    this.receive=this.receive.bind(this);
    this.refresh=this.refresh.bind(this);
    this.button.onclick=()=>this.open();
    window.addEventListener('message',this.receive);
    window.addEventListener('focus',this.refresh);
  }

  async prepare() {
    if(this.disposed||this.connecting||this.popup||this.preparing)return;
    if(this.challenge&&Date.now()-this.challengeAt<challengeAge)return;
    this.challenge=null;
    this.preparing=true;
    this.button.disabled=true;
    this.button.textContent=this.text('正在准备 Apple 授权…');
    try{
      // This request only obtains Apple's challenge. The signed-in session
      // will be consumed by the private database, not an identity probe.
      const {containerIdentifier,environment,apiToken}=this.config;
      const url=new URL(`/database/1/${encodeURIComponent(containerIdentifier)}/${environment}/public/users/current`,apiOrigin);
      url.searchParams.set('ckAPIToken',apiToken);
      url.searchParams.set('clientId',this.clientId);
      const response=await fetch(url,{credentials:'omit',redirect:'error',cache:'no-store',signal:AbortSignal.timeout(15000)});
      const body=await response.json();
      if(response.status!==421||body.serverErrorCode!=='AUTHENTICATION_REQUIRED'||typeof body.redirectURL!=='string')throw new Error('challenge');
      const challenge=new URL(body.redirectURL);
      if(!loginOrigins.has(challenge.origin)||challenge.username||challenge.password)throw new Error('challenge origin');
      if(this.disposed)return;
      this.challenge=challenge;
      this.challengeAt=Date.now();
      this.button.textContent=this.text('在 Apple 窗口登录');
    }catch{
      if(!this.disposed){
        this.button.textContent=this.text('重试 Apple 授权');
        this.onStatus('暂时无法打开 Apple 登录。');
      }
    }finally{
      this.preparing=false;
      if(!this.disposed)this.button.disabled=false;
    }
  }

  refresh() {
    if(!this.button.hidden&&this.button.closest('dialog')?.open)void this.prepare();
  }

  open() {
    if(this.disposed||this.connecting||this.popup)return;
    if(!this.challenge||Date.now()-this.challengeAt>=challengeAge){void this.prepare();return;}
    const target=this.challenge.href;
    this.challenge=null;
    this.popup=window.open(target,`feinotes_signin_${crypto.randomUUID()}`,'popup=yes,width=530,height=750');
    if(!this.popup||this.popup===window){
      this.popup=null;
      this.onStatus('浏览器未打开 Apple 登录窗口。');
      void this.prepare();
      return;
    }
    this.button.disabled=true;
    this.onStatus('请在 Apple 窗口完成登录。');
    const opened=this.popup;
    const started=Date.now();
    this.poll=setInterval(()=>{
      if(!opened.closed&&Date.now()-started<300000)return;
      clearInterval(this.poll);
      this.expiration=setTimeout(()=>{
        if(this.popup!==opened||this.connecting||this.disposed)return;
        this.popup=null;
        this.onStatus(opened.closed?'Apple 窗口已关闭，尚未收到授权。':'Apple 登录等待超时，请重试。');
        void this.prepare();
      },1500);
    },850);
  }

  async receive(event) {
    if(this.disposed||this.connecting||!this.popup||event.origin!==relayOrigin||event.source!==this.popup)return;
    const data=event.data;
    const session=data?.ckSession||data?.ckWebAuthToken;
    const valid=data?.status===0&&data?.appInfo?.DevPostMsgOrigin===location.origin&&
      typeof session==='string'&&!!session&&!(data.ckSession&&data.ckWebAuthToken&&data.ckSession!==data.ckWebAuthToken);
    if(!valid){
      this.resetWindow();
      this.onStatus('Apple 未返回有效授权。');
      void this.prepare();
      return;
    }
    this.connecting=true;
    this.resetWindow();
    this.onStatus('正在读取 iCloud 私有文档库…');
    try{await this.onSession(session,this.clientId);}
    catch(error){
      if(!this.disposed){
        this.onStatus(error?.userMessage||'无法读取 iCloud 私有文档库，请重试。');
        this.button.textContent=this.text('重新登录 Apple');
        this.button.disabled=false;
      }
    }finally{
      this.connecting=false;
      if(!this.disposed)void this.prepare();
    }
  }

  resetWindow() {
    clearInterval(this.poll);
    clearTimeout(this.expiration);
    if(this.popup&&!this.popup.closed)this.popup.close();
    this.popup=null;
  }

  dispose() {
    this.disposed=true;
    this.resetWindow();
    this.challenge=null;
    this.button.onclick=null;
    this.button.disabled=true;
    window.removeEventListener('message',this.receive);
    window.removeEventListener('focus',this.refresh);
  }
}
