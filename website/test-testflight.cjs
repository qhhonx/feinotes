/* Offline checks of the published availability consumer, with no Apple calls. */
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const code = fs.readFileSync(__dirname + '/testflight.js', 'utf8');
const available = { state: 'available', version: '1.0.4', build: '1010', url: 'https://testflight.apple.com/join/example' };
const mobileInvite = { ...available, url: 'https://testflight.apple.com/join/mobile' };
const inReview = { ...available, state: 'review' };

async function render(status, language = 'zh', ok = true) {
  const offers = Array.from({ length: 2 }, () => {
    const link = {}, version = { rows: [], replaceChildren(...rows) { this.rows = rows; } };
    return { hidden: true, link, version, querySelector: selector => selector === '[data-beta-link]' ? link : version };
  });
  const actions = { hidden: true, querySelectorAll: () => offers };
  const notice = { textContent: '' };
  const section = {
    dataset: { language, status: '/status.json' },
    querySelector: selector => selector === '.beta-actions' ? actions : notice,
  };
  vm.runInNewContext(code, {
    document: {
      querySelector: () => section,
      createElement: () => ({ children: [], append(value) { this.children.push(value); } }),
    },
    URL,
    fetch: async () => ({ ok, json: async () => status }),
  });
  await new Promise(resolve => setTimeout(resolve, 0));
  const visible = offers.filter(offer => !offer.hidden);
  assert.equal(actions.hidden, visible.length === 0);
  for (const offer of offers.filter(offer => offer.hidden)) assert.equal(offer.link.href, undefined);
  return {
    links: visible.map(offer => offer.link),
    versions: visible.map(offer => offer.version.rows.map(row => row.children
      .map(child => typeof child === 'string' ? child : child.textContent).join('')).join('\n')),
    notice: notice.textContent,
  };
}

(async () => {
  let result = await render(available);
  assert.deepEqual(result.links.map(link => link.href), [available.url]);
  assert.match(result.notice, /iPhone \/ iPad · 测试版暂未开放/);

  for (const [language, label, publicBeta] of [
    ['zh', '通过 TestFlight 体验', '公开测试版'], ['en', 'Try with TestFlight', 'Public beta'],
  ]) {
    result = await render({ platforms: { MAC_OS: available, IOS: available } }, language);
    assert.deepEqual(result.links, [{ href: available.url, textContent: label }]);
    assert.deepEqual(result.versions, [`Mac · iPhone · iPad ${publicBeta} 1.0.4`]);
    assert.equal(result.notice, '');
  }

  result = await render({ platforms: { MAC_OS: available, IOS: { ...available, version: '1.0.5' } } });
  assert.deepEqual(result.links.map(link => link.href), [available.url]);
  assert.deepEqual(result.versions, ['Mac 公开测试版 1.0.4\niPhone / iPad 公开测试版 1.0.5']);

  const noVersion = { state: available.state, url: available.url };
  result = await render({ platforms: { MAC_OS: noVersion, IOS: noVersion } });
  assert.deepEqual(result.versions, ['Mac · iPhone · iPad']);
  result = await render({ platforms: { MAC_OS: available, IOS: noVersion } });
  assert.deepEqual(result.versions, ['Mac 公开测试版 1.0.4\niPhone / iPad']);

  result = await render({ platforms: { MAC_OS: available, IOS: { ...mobileInvite, version: '1.0.5' } } }, 'en');
  assert.deepEqual(result.links.map(link => link.href), [available.url, mobileInvite.url]);
  assert.deepEqual(result.links.map(link => link.textContent), [
    'Try with TestFlight · Mac', 'Try with TestFlight · iPhone / iPad',
  ]);
  assert.deepEqual(result.versions, ['Public beta 1.0.4', 'Public beta 1.0.5']);
  assert.equal(result.notice, '');

  result = await render({ platforms: { MAC_OS: available, IOS: inReview } });
  assert.deepEqual(result.links.map(link => link.href), [available.url]);
  assert.match(result.links[0].textContent, / · Mac$/);
  assert.deepEqual(result.versions, ['公开测试版 1.0.4']);
  assert.equal(result.notice, 'iPhone / iPad · 1.0.4 审核中');

  result = await render({ platforms: { MAC_OS: { state: 'unavailable' }, IOS: mobileInvite } }, 'en');
  assert.deepEqual(result.links.map(link => link.href), [mobileInvite.url]);
  assert.match(result.links[0].textContent, / · iPhone \/ iPad$/);
  assert.equal(result.notice, 'Mac · Beta unavailable');

  result = await render({ platforms: { MAC_OS: inReview, IOS: inReview } }, 'en');
  assert.deepEqual(result.links, []);
  assert.equal(result.notice, 'Mac · 1.0.4 in review\niPhone / iPad · 1.0.4 in review');

  for (const url of ['https://example.com/join/example', 'https://testflight.apple.com/other', 'not a URL']) {
    result = await render({ platforms: { MAC_OS: available, IOS: { ...available, url } } });
    assert.deepEqual(result.links.map(link => link.href), [available.url]);
    assert.match(result.notice, /iPhone \/ iPad · 测试版暂未开放/);
  }

  const pending = { state: 'review', version: '1.0.7' };
  result = await render({ platforms: { MAC_OS: available, IOS: { ...available, pending } } });
  assert.deepEqual(result.links.map(link => link.href), [available.url]);
  assert.equal(result.links[0].textContent, '通过 TestFlight 体验');
  assert.deepEqual(result.versions, ['Mac · iPhone · iPad 公开测试版 1.0.4']);
  assert.equal(result.notice, 'iPhone / iPad · 1.0.7 审核中');

  result = await render({ platforms: { MAC_OS: { ...available, pending: { ...pending, version: available.version } }, IOS: available } }, 'en');
  assert.deepEqual(result.versions, ['Mac · iPhone · iPad Public beta 1.0.4']);
  assert.equal(result.notice, 'Mac · New build in review');

  result = await render(available, 'en', false);
  assert.deepEqual(result.links, []);
  assert.equal(result.notice, 'Could not check beta availability.');

  console.log('Website TestFlight availability checks passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
