// Run with node tools/test-share-links.mjs. Synthetic links never leave this process.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {invitation} from '../website/share/links.mjs';

const webPrefix = 'https://feinotes.app/share/?v=2&u=';
const appPrefix = 'feinotes://share/?v=2&u=';
const tokenFor = apple => Buffer.from(apple, 'utf8').toString('base64url');
const sample = tokenFor('https://www.icloud.com/share/0synthetic');
assert.equal(sample, 'aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS8wc3ludGhldGlj');

for (const apple of ['https://www.icloud.com/share/0synthetic',
  'https://icloud.com.cn/share/0synthetic?source=icloud#%E6%96%87%E6%A1%A3']) {
  const token = tokenFor(apple);
  assert.deepEqual(invitation(webPrefix + token), {web: webPrefix + token, app: appPrefix + token});
  assert.equal(Buffer.from(invitation(webPrefix + token).app.slice(appPrefix.length), 'base64url').toString(), apple);
}
for (const region of ['c', 'g']) {
  const query = `?${region}=0synthetic_ABC-123`;
  const web = 'https://feinotes.app/share/' + query;
  assert.deepEqual(invitation(web), {web, app: 'feinotes://share/' + query});
}
for (const apple of ['https://elsewhere.example/share/x', 'https://www.icloud.com@elsewhere.example/share/x',
  'https://www.icloud.com:444/share/x', 'https://www.icloud.com/share/?x=1',
  'https://www.icloud.com/share/#title', 'https://www.icloud.com/share/%2e./x',
  'https://www.icloud.com/share/foo%2fbar', 'https://www.icloud.com/share/foo%5Cbar',
  'https://www.icloud.com/share/ foo']) assert.throws(() => invitation(webPrefix + tokenFor(apple)), apple);
for (const url of [webPrefix, webPrefix + sample + '=', webPrefix + sample + '#fragment',
  webPrefix + sample + '&u=' + sample, webPrefix + 'a'.repeat(5463),
  'https://feinotes.app.evil.example/share/?v=2&u=' + sample,
  'https://feinotes.app/share/?v=1&u=' + sample,
  ...['?c=', '?g=id&c=other', '?c=id#fragment', '?g=id/path', '?g=%61', '?c=' + 'a'.repeat(257)]
    .map(query => 'https://feinotes.app/share/' + query)]) {
  assert.throws(() => invitation(url), url);
}

const source = readFileSync(new URL('../website/share/page.mjs', import.meta.url), 'utf8').replace("import {invitation} from './links.mjs?v=share6';", '');
function page({language='zh-CN', url=webPrefix + sample, visible=true, blocked=false}={}) {
  const elements = new Map(); const timers = []; const attempts = []; const copied = [];
  const get = id => { if (!elements.has(id)) elements.set(id, {hidden:true, textContent:'', addEventListener(type, fn) { this[type] = fn; }}); return elements.get(id); };
  const document = {visibilityState:visible ? 'visible' : 'hidden', documentElement:{}, getElementById:get, querySelectorAll:() => []};
  const location = {href:url, assign(url) { attempts.push(url); if (blocked) throw Error('User activation required'); }};
  vm.runInNewContext(source, {invitation, navigator:{language, clipboard:{writeText:async text => copied.push(text)}}, document, location, setTimeout:(fn, delay) => timers.push({fn,delay})});
  return {get, timers, attempts, copied, document};
}
for (const language of ['zh-CN','en-US']) {
  const p = page({language, blocked:true});
  assert.equal(p.get('invite-open').hidden, false);
  assert.equal(p.get('open-app').href, appPrefix + sample);
  assert.match(p.get('invite-status').textContent, /正在打开|Opening/);
  p.timers.forEach(timer => timer.fn());
  assert.equal(p.attempts.length, 1);
  assert.match(p.get('invite-status').textContent, /下方按钮|button below/);
  await p.get('copy-link').click({preventDefault(){}});
  assert.deepEqual(p.copied, [webPrefix + sample]);
  assert.match(p.get('invite-status').textContent, /已复制|copied/);
  p.document.visibilityState = 'hidden'; p.document.visibilityState = 'visible';
  assert.equal(p.attempts.length, 1, 'Returning from the app must not launch it again');
}
const legacyUrl = 'https://feinotes.app/share/?c=0synthetic';
const legacyPage = page({url:legacyUrl});
assert.equal(legacyPage.get('open-app').href, 'feinotes://share/?c=0synthetic');
await legacyPage.get('copy-link').click({preventDefault(){}});
assert.deepEqual(legacyPage.copied, [legacyUrl]);
const hidden = page({visible:false}); hidden.timers.forEach(timer => timer.fn()); assert.equal(hidden.attempts.length, 0);
const invalid = page({url:'https://feinotes.app/share/invalid'}); assert.equal(invalid.timers.length, 0); assert.equal(invalid.get('invite-open').hidden, true);
assert.match(invalid.get('invite-status').textContent, /邀请链接无效/);
console.log('SHARE_LINKS_OK v1-and-v2-routing v2-exact-payload invalid-inputs auto-once blocked-fallback bilingual copy no-return-loop');
