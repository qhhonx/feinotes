// Daily statistics for Quiet.md, written to the D1 database `quiet-md-stats`.
// It never sits in front of a download: a cron trigger reads what Cloudflare and
// GitHub already count and stores daily aggregates (no IPs) in the `metrics` table.
//
//   cloudflare  download.quiet.md requests (Cloudflare's analytics keep 31 days)
//     cdn.downloads          distinct IPs per file and day: a resumed or segmented
//                            download counts once      {platform, version, channel, country}
//     cdn.download_requests  raw GET requests, for comparison  {platform, version, channel, status}
//     update.checks          distinct IPs fetching an update manifest  {platform, country}
//     update.check_requests  raw manifest requests     {platform}
//   site        quiet.md's home page (same analytics, same 31 days)
//     site.visitors          distinct IPs that loaded the page, crawlers left out  {country}
//     site.pageviews         page loads, crawlers left out  {}
//   github      the repository (GitHub keeps only running totals)
//     github.downloads_total release asset counters at collection time  {platform, version, asset}
//     github.downloads       their increase since the previous snapshot  {platform, version}
//     github.stars, github.forks, github.watchers, github.open_issues (issues and PRs)  {}
//
// channel: "update" is the in-app updater (curl fetching a versioned v<x.y.z>/ package,
// the URL its manifest names); "cdn" is everything else on download.quiet.md, i.e. the
// site, the README and release-note links opened in a browser. The free plan's analytics
// have no query string or referer, so the site and the README are not told apart here.

const HOST = 'download.quiet.md', SITE = 'quiet.md';
const CRAWLER = /bot|crawl|spider|slurp|curl|wget|python|go-http|java\/|headless|preview|monitor|scan/i;
const PLATFORMS = { 'macos-arm64.zip': 'macos-arm64', 'windows-x64-setup.exe': 'windows-x64', 'macos-x86_64.zip': 'macos-x86_64', 'x86_64.AppImage': 'linux-x86_64' };
const PAGE = 5000; // Bound each analytics response; split full windows below.

export default {
  async scheduled(_event, env, ctx) {
    ctx.waitUntil(collect(env));
  },
};

export async function collect(env, now = new Date()) {
  const yesterday = dayOf(new Date(now.getTime() - 86400e3));
  // Cloudflare: every day since the last good run, oldest first, at most MAX_DAYS per run
  // (the analytics only reach 30 days back; a gap older than that is lost).
  const oldest = dayOf(new Date(now.getTime() - 30 * 86400e3));
  for (const [source, f] of [['cloudflare', cloudflare], ['site', site]]) {
    const last = await env.DB.prepare('SELECT max(day) AS d FROM runs WHERE source = ? AND error IS NULL').bind(source).first('d');
    let day = last ? next(last) : oldest;
    if (day < oldest) day = oldest;
    for (let n = 0; day <= yesterday && n < Number(env.MAX_DAYS || 3); day = next(day), n++) {
      await run(env, source, day, () => f(env, day));
    }
  }
  // GitHub: one snapshot per day, filed under yesterday (the run is just after midnight UTC).
  await run(env, 'github', yesterday, () => github(env, yesterday));
}

async function run(env, source, day, f) {
  const started = new Date().toISOString();
  try {
    const rows = await f();
    await env.DB.prepare('INSERT INTO runs (started_at, source, day, rows) VALUES (?, ?, ?, ?)').bind(started, source, day, rows).run();
  } catch (e) {
    await env.DB.prepare('INSERT INTO runs (started_at, source, day, error) VALUES (?, ?, ?, ?)').bind(started, source, day, String(e && e.stack || e).slice(0, 2000)).run();
  }
}

// ---- Cloudflare ----

/// What a request to download.quiet.md is: a manifest, a package, or noise (null).
export function classify(path, userAgent) {
  let m = path.match(/^\/latest\/update-(.+)\.json$/);
  if (m) return { kind: 'check', platform: m[1] };
  m = path.match(/^\/latest\/quiet-md-(macos-arm64\.zip|macos-x86_64\.zip|windows-x64-setup\.exe|x86_64\.AppImage)$/);
  if (m) return { kind: 'download', platform: PLATFORMS[m[1]], version: 'latest', channel: 'cdn' };
  m = path.match(/^\/v(\d+\.\d+\.\d+)\/quiet-md-(\d+\.\d+\.\d+)-(macos-arm64\.zip|macos-x86_64\.zip|windows-x64-setup\.exe|x86_64\.AppImage)$/);
  if (m && m[1] === m[2]) return { kind: 'download', platform: PLATFORMS[m[3]], version: m[1], channel: /^curl\//.test(userAgent || '') ? 'update' : 'cdn' };
  return null;
}

const QUERY = (filter) => `query($zone: String!, $from: Time!, $to: Time!, $limit: Int!) { viewer { zones(filter: {zoneTag: $zone}) {
  httpRequestsAdaptiveGroups(limit: $limit, filter: {datetime_geq: $from, datetime_lt: $to, clientRequestHTTPMethodName: "GET", ${filter}}) {
    count dimensions { clientIP clientRequestPath userAgent clientCountryName edgeResponseStatus } } } } }`;
const DOWNLOADS = `clientRequestHTTPHost: "${HOST}"`;
const HOME = `clientRequestHTTPHost: "${SITE}", clientRequestPath: "/", edgeResponseStatus: 200`;

async function groups(env, from, to, filter = DOWNLOADS) {
  const res = await fetch('https://api.cloudflare.com/client/v4/graphql', {
    method: 'POST',
    headers: { authorization: `Bearer ${String(env.CF_API_TOKEN || '').trim()}`, 'content-type': 'application/json' },
    body: JSON.stringify({ query: QUERY(filter), variables: { zone: env.ZONE_ID, from: from.toISOString(), to: to.toISOString(), limit: PAGE } }),
  });
  const body = await res.json();
  if (!res.ok || body.errors) {
    throw new Error(`graphql ${res.status}: ${JSON.stringify(body.errors || body).slice(0, 500)}`);
  }
  const list = body.data.viewer.zones[0].httpRequestsAdaptiveGroups;
  // A full page may have been cut short: split the window and ask again.
  if (list.length < PAGE) return list;
  if (to - from <= 60000) throw new Error('Cloudflare analytics result truncated');
  const mid = new Date((from.getTime() + to.getTime()) / 2);
  return [...(await groups(env, from, mid, filter)), ...(await groups(env, mid, to, filter))];
}

async function cloudflare(env, day) {
  const from = new Date(`${day}T00:00:00Z`);
  const list = await groups(env, from, new Date(from.getTime() + 86400e3));
  const add = (map, key, n) => map.set(key, (map.get(key) || 0) + n);
  const ips = new Map(), downloadReq = new Map(), checkIps = new Map(), checkReq = new Map();
  const dailyDownloads = new Map(), dailyChecks = new Map(['all', 'Mac', 'Windows', 'Linux', 'Other'].map((p) => [p, new Set()]));
  const family = (p) => p.startsWith('macos-') ? 'Mac' : p.startsWith('windows-') ? 'Windows' : p.startsWith('linux-') ? 'Linux' : 'Other';
  for (const { count, dimensions: d } of list) {
    const c = classify(d.clientRequestPath, d.userAgent);
    if (!c) continue;
    const country = d.clientCountryName || 'XX';
    if (c.kind === 'download') {
      if (![200, 206].includes(d.edgeResponseStatus)) continue;
      const dk = key({ platform: family(c.platform), channel: c.channel, path: d.clientRequestPath });
      if (!dailyDownloads.has(dk)) dailyDownloads.set(dk, new Set());
      dailyDownloads.get(dk).add(d.clientIP);
      add(downloadReq, key({ platform: c.platform, version: c.version, channel: c.channel, status: String(d.edgeResponseStatus) }), count);
      const k = key({ platform: c.platform, version: c.version, channel: c.channel, country });
      if (!ips.has(k)) ips.set(k, new Set());
      ips.get(k).add(d.clientIP);
    } else {
      dailyChecks.get('all').add(d.clientIP);
      dailyChecks.get(family(c.platform)).add(d.clientIP);
      if (![200, 206, 304].includes(d.edgeResponseStatus)) continue;
      add(checkReq, key({ platform: c.platform }), count);
      const k = key({ platform: c.platform, country });
      if (!checkIps.has(k)) checkIps.set(k, new Set());
      checkIps.get(k).add(d.clientIP);
    }
  }
  const rows = [['daily.coverage', '{}', 1]];
  const daily = new Map();
  for (const [k, set] of dailyDownloads) { const { platform, channel } = JSON.parse(k); add(daily, key({ platform, channel }), set.size); }
  for (const platform of ['Mac', 'Windows', 'Linux', 'Other']) for (const channel of ['cdn', 'update']) {
    const k = key({ platform, channel }); rows.push(['daily.downloads', k, daily.get(k) || 0]);
  }
  for (const [platform, set] of dailyChecks) rows.push(['daily.checks', key({ platform }), set.size]);
  for (const [k, s] of ips) rows.push(['cdn.downloads', k, s.size]);
  for (const [k, n] of downloadReq) rows.push(['cdn.download_requests', k, n]);
  for (const [k, s] of checkIps) rows.push(['update.checks', k, s.size]);
  for (const [k, n] of checkReq) rows.push(['update.check_requests', k, n]);
  return replace(env, day, 'cloudflare', rows);
}

async function site(env, day) {
  const from = new Date(`${day}T00:00:00Z`);
  const list = await groups(env, from, new Date(from.getTime() + 86400e3), HOME);
  const ips = new Map();
  let views = 0;
  for (const { count, dimensions: d } of list) {
    if (CRAWLER.test(d.userAgent || '') || !/mozilla/i.test(d.userAgent || '')) continue;
    views += count;
    const k = key({ country: d.clientCountryName || 'XX' });
    if (!ips.has(k)) ips.set(k, new Set());
    ips.get(k).add(d.clientIP);
  }
  const rows = [['site.pageviews', '{}', views]];
  for (const [k, s] of ips) rows.push(['site.visitors', k, s.size]);
  return replace(env, day, 'site', rows);
}

// ---- GitHub ----

async function github(env, day) {
  const headers = { 'user-agent': 'quiet-md-stats', accept: 'application/vnd.github+json' };
  if (env.GITHUB_TOKEN) headers.authorization = `Bearer ${env.GITHUB_TOKEN}`;
  const repo = await fetch(`https://api.github.com/repos/${env.GITHUB_REPO}`, { headers });
  if (!repo.ok) throw new Error(`github ${repo.status}: ${(await repo.text()).slice(0, 300)}`);
  const r0 = await repo.json();
  const rows = [['github.stars', '{}', r0.stargazers_count], ['github.forks', '{}', r0.forks_count],
    ['github.watchers', '{}', r0.subscribers_count], ['github.open_issues', '{}', r0.open_issues_count]];
  for (let page = 1; page < 10; page++) {
    const res = await fetch(`https://api.github.com/repos/${env.GITHUB_REPO}/releases?per_page=100&page=${page}`, { headers });
    if (!res.ok) throw new Error(`github ${res.status}: ${(await res.text()).slice(0, 300)}`);
    const releases = await res.json();
    for (const r of releases) {
      for (const a of r.assets) {
        const p = Object.keys(PLATFORMS).find((s) => a.name.endsWith(s));
        if (p) rows.push(['github.downloads_total', key({ platform: PLATFORMS[p], version: r.tag_name.replace(/^v/, ''), asset: a.name }), a.download_count]);
      }
    }
    if (releases.length < 100) break;
  }
  // The day's increase, against the latest earlier snapshot of the same asset.
  const prev = await env.DB.prepare(`SELECT dims, value FROM metrics m WHERE metric = 'github.downloads_total'
    AND day = (SELECT max(day) FROM metrics WHERE metric = 'github.downloads_total' AND day < ?)`).bind(day).all();
  const previousDay = await env.DB.prepare("SELECT max(day) AS d FROM metrics WHERE metric = 'github.downloads_total' AND day < ?").bind(day).first('d');
  const consecutive = previousDay && next(previousDay) === day;
  const before = new Map(prev.results.map((r) => [r.dims, r.value]));
  const delta = new Map();
  for (const [metric, k, total] of rows) {
    if (metric !== 'github.downloads_total') continue;
    const d = JSON.parse(k);
    // A first snapshot has nothing to compare with; an asset new since then starts at 0.
    if (!consecutive || before.size === 0) continue;
    const k2 = key({ platform: d.platform, version: d.version });
    delta.set(k2, (delta.get(k2) || 0) + total - (before.get(k) || 0));
  }
  for (const [k, n] of delta) rows.push(['github.downloads', k, n]);
  return replace(env, day, 'github', rows);
}

// ---- D1 ----

/// The source's rows for the day, replaced as a whole: a rerun leaves no stale breakdowns.
async function replace(env, day, source, rows) {
  const at = new Date().toISOString();
  const stmts = [env.DB.prepare('DELETE FROM metrics WHERE day = ? AND source = ?').bind(day, source)];
  const insert = env.DB.prepare('INSERT INTO metrics (day, metric, dims, value, source, collected_at) VALUES (?, ?, ?, ?, ?, ?)');
  for (const [metric, dims, value] of rows) stmts.push(insert.bind(day, metric, dims, value, source, at));
  for (let i = 0; i < stmts.length; i += 500) await env.DB.batch(stmts.slice(i, i + 500));
  return rows.length;
}

/// Dimensions as JSON with sorted keys, so one breakdown always has one key.
export function key(o) {
  return JSON.stringify(Object.fromEntries(Object.keys(o).sort().map((k) => [k, o[k]])));
}

function dayOf(d) { return d.toISOString().slice(0, 10); }
function next(day) { return dayOf(new Date(new Date(`${day}T00:00:00Z`).getTime() + 86400e3)); }
