// Run with node tools/test-share-links.mjs. Synthetic links never leave this process.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {invitation} from '../website/share/links.mjs';

for (const region of ['c', 'g']) {
  const web = `https://feinotes.app/share/?${region}=0synthetic_ABC-123`;
  assert.deepEqual(invitation(web), {web, app:`feinotes://share/?${region}=0synthetic_ABC-123`});
}
for (const url of [
  'https://evil.example/share/?c=token', 'https://feinotes.app.evil.example/share/?c=token',
  'https://feinotes.app@evil.example/share/?c=token', 'http://feinotes.app/share/?c=token',
  'https://user@feinotes.app/share/?c=token', 'https://feinotes.app:444/share/?c=token',
  'https://feinotes.app/share/?c=', 'https://feinotes.app/share/?x=token',
  'https://feinotes.app/share/?c=token&url=https://evil.example', 'https://feinotes.app/share/?g=token#fragment',
  'https://feinotes.app/share/?g=%2fsecret', 'https://feinotes.app/share/?g=../x',
  'https://feinotes.app/share/v1_aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS8wc3ludGhldGlj',
  `https://feinotes.app/share/?g=${'a'.repeat(257)}`,
]) assert.throws(() => invitation(url), url);

const source = readFileSync(new URL('../website/share/page.mjs', import.meta.url), 'utf8').replace("import {invitation} from './links.mjs?v=share3';", '');
function page({language='zh-CN', valid=true, visible=true, blocked=false}={}) {
  const elements = new Map(); const timers = []; const attempts = []; const copied = [];
  const get = id => { if (!elements.has(id)) elements.set(id, {hidden:true, textContent:'', addEventListener(type, fn) { this[type] = fn; }}); return elements.get(id); };
  const document = {visibilityState:visible ? 'visible' : 'hidden', documentElement:{}, getElementById:get, querySelectorAll:() => []};
  const location = {href:`https://feinotes.app/share/${valid ? '?c=0synthetic' : 'invalid'}`, assign(url) { attempts.push(url); if (blocked) throw Error('User activation required'); }};
  vm.runInNewContext(source, {invitation, navigator:{language, clipboard:{writeText:async text => copied.push(text)}}, document, location, setTimeout:(fn, delay) => timers.push({fn,delay})});
  return {get, timers, attempts, copied, document};
}
for (const language of ['zh-CN','en-US']) {
  const p = page({language, blocked:true});
  assert.equal(p.get('invite-open').hidden, false);
  assert.equal(p.get('open-app').href, 'feinotes://share/?c=0synthetic');
  assert.match(p.get('invite-status').textContent, /正在打开|Opening/);
  p.timers.forEach(timer => timer.fn());
  assert.equal(p.attempts.length, 1);
  assert.match(p.get('invite-status').textContent, /下方按钮|button below/);
  await p.get('copy-link').click({preventDefault(){}});
  assert.deepEqual(p.copied, ['https://feinotes.app/share/?c=0synthetic']);
  assert.match(p.get('invite-status').textContent, /已复制|copied/);
  p.document.visibilityState = 'hidden'; p.document.visibilityState = 'visible';
  assert.equal(p.attempts.length, 1, 'Returning from the app must not launch it again');
}
const hidden = page({visible:false}); hidden.timers.forEach(timer => timer.fn()); assert.equal(hidden.attempts.length, 0);
const invalid = page({valid:false}); assert.equal(invalid.timers.length, 0); assert.equal(invalid.get('invite-open').hidden, true);
assert.match(invalid.get('invite-status').textContent, /邀请链接无效/);
console.log('SHARE_LINKS_OK compact-regions invalid-inputs auto-once blocked-fallback bilingual copy no-return-loop');
