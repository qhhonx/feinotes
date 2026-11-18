/* Offline timing checks; no browser or synchronization service required. */
const vm = require('node:vm');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const code = fs.readFileSync(__dirname + '/theme-gallery.js', 'utf8');
async function check(language) {
  let now = 0, id = 0;
  const frames = new Map(), timers = new Map(), observers = new Map(), events = {};
  const scenes = ['sync', 'collab'].map(name => {
    const classes = new Set(), props = {};
    const toggle = { addEventListener: (_, callback) => { events[name + 'Toggle'] = callback; }, setAttribute() {} };
    return { hidden: name !== 'sync', dataset: { scene: name }, classes, props,
      classList: { add: key => classes.add(key), remove: key => classes.delete(key) },
      style: { setProperty: (key, value) => { props[key] = value; } },
      querySelector: () => toggle, querySelectorAll: () => [{ decode: () => Promise.resolve() }] };
  });
  const buttons = scenes.map(scene => ({ dataset: { theme: scene.dataset.scene }, classList: { toggle() {} }, setAttribute() {},
    addEventListener: (_, callback) => { events[scene.dataset.scene] = callback; } }));
  const document = { hidden: false, documentElement: { lang: language },
    querySelectorAll: selector => selector.includes('button') ? buttons : scenes,
    addEventListener: (name, callback) => { events[name] = callback; } };
  const reduced = { matches: false, addEventListener: (_, callback) => { events.reduced = callback; } };
  const context = { document, matchMedia: () => reduced, window: { IntersectionObserver: true }, performance: { now: () => now },
    requestAnimationFrame: callback => (frames.set(++id, callback), id), cancelAnimationFrame: key => frames.delete(key),
    setTimeout: (callback, delay) => (timers.set(++id, { callback, at: now + delay }), id), clearTimeout: key => timers.delete(key),
    IntersectionObserver: class { constructor(callback) { this.callback = callback; } observe(scene) { observers.set(scene, this.callback); } } };
  const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
  const tick = async time => { now = time; const pending = [...frames.values()]; frames.clear(); pending.forEach(callback => callback(time)); await flush(); };
  const visible = (scene, value) => observers.get(scene)([{ isIntersecting: value, intersectionRatio: value ? 1 : 0 }]);
  vm.runInNewContext(code, context);
  const [sync, collab] = scenes;
  visible(sync, true); await flush(); await tick(1500);
  assert(sync.props['--phone-progress'] > sync.props['--mac-progress']);
  events.collab(); visible(sync, false); visible(collab, true); await flush();
  assert(!sync.classes.has('is-writing')); assert(collab.classes.has('is-writing')); assert.equal(frames.size, 1);
  await tick(3500);
  assert(collab.props['--first-progress'] > 0 && collab.props['--first-progress'] < 1);
  assert(collab.props['--second-progress'] > 0 && collab.props['--second-progress'] < 1);
  await tick(5100); assert.equal(collab.dataset.phase, 'complete');
  assert.equal([...timers.values()][0].at, 9100);
  now = 9100; [...timers.values()][0].callback(); await flush(); assert(collab.classes.has('is-writing'));
  events.collabToggle(); assert.equal(frames.size, 0); assert.equal(timers.size, 0);
  events.collabToggle(); await flush(); assert.equal(frames.size, 1);
  document.hidden = true; events.visibilitychange(); assert.equal(frames.size, 0);
  document.hidden = false; events.visibilitychange(); await flush(); assert.equal(frames.size, 1);
  visible(collab, false); assert.equal(frames.size, 0); visible(collab, true); await flush(); assert.equal(frames.size, 1);
  reduced.matches = true; events.reduced(); assert.equal(frames.size, 0); assert.equal(timers.size, 0);
  reduced.matches = false; events.reduced(); events.sync(); visible(collab, false); visible(sync, true); await flush();
  assert(sync.classes.has('is-writing')); assert(!collab.classes.has('is-writing')); assert.equal(frames.size, 1);
}
(async () => { await check('zh-Hans'); await check('en'); console.log('Both languages passed: overlapping editors, loop hold, pause, scene switch, tab/viewport suspension, reduced motion.'); })()
  .catch(error => { console.error(error); process.exitCode = 1; });
