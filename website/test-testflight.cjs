/* Offline checks of the published availability consumer, with no Apple calls. */
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const code = fs.readFileSync(__dirname + '/testflight.js', 'utf8');
const available = { state: 'available', version: '1.0.4', build: '1010', url: 'https://testflight.apple.com/join/example' };
function element() {
  return { attrs: {}, hidden: true, textContent: '',
    removeAttribute(key) { delete this.attrs[key]; }, setAttribute(key, value) { this.attrs[key] = value; },
    set href(value) { this.attrs.href = value; } };
}
async function render(status, language = 'zh') {
  const blocks = ['MAC_OS', 'IOS'].map(platform => {
    const link = element(), review = element();
    return { dataset: { betaPlatform: platform }, link, review,
      querySelector: key => key === '[data-beta-link]' ? link : review };
  });
  const section = { dataset: { language, status: '/status.json' }, querySelectorAll: () => blocks };
  vm.runInNewContext(code, { document: { querySelector: () => section }, URL,
    fetch: async () => ({ ok: true, json: async () => status }) });
  await new Promise(resolve => setTimeout(resolve, 0));
  return blocks;
}
(async () => {
  let [mac, ios] = await render(available);
  assert.equal(mac.link.attrs.href, available.url);
  assert.equal(ios.link.attrs['aria-disabled'], 'true');
  assert.equal(ios.link.attrs.href, undefined);
  [mac, ios] = await render({ ...available, platforms: { MAC_OS: available, IOS: { state: 'review' } } });
  assert.equal(mac.link.attrs.href, available.url);
  assert.match(ios.link.textContent, /审核中/);
  [mac, ios] = await render({ platforms: { MAC_OS: { state: 'unavailable' }, IOS: available } }, 'en');
  assert.equal(mac.link.attrs.href, undefined);
  assert.equal(ios.link.attrs.href, available.url);
  assert.match(ios.link.textContent, /iPhone \/ iPad/);
  [mac, ios] = await render({ platforms: { MAC_OS: available, IOS: { ...available, url: 'https://example.com/join/example' } } });
  assert.equal(mac.link.attrs.href, available.url);
  assert.equal(ios.link.attrs.href, undefined);
  [mac] = await render({ ...available, pending: { state: 'review', version: '1.0.7' } });
  assert.match(mac.review.textContent, /1.0.7 审核中/);
  assert.equal(mac.review.hidden, false);
  console.log('5 website platform availability scenarios passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
