# quiet-md-stats

A Cloudflare Worker that runs once a day (00:30 UTC) and stores Quiet.md's statistics in the D1 database `quiet-md-stats`. Downloads and update checks never pass through it. Its one route, `POST telemetry.quiet.md/v1/cli`, takes the `quiet` command line tool's daily usage counts (`crates/cli/src/usage.rs`) into `cli_reports`. It reads what Cloudflare and GitHub already count and keeps daily aggregates, which those services don't keep for long (Cloudflare's per-path analytics: 31 days; GitHub: a running total only). No IP addresses are stored.

## What it records

Every number is a row of `metrics`: `(day, metric, dims, value, source)`, `day` in UTC and `dims` a JSON object with sorted keys. A new statistic is a new metric name, not a new table.

| metric | value | dims |
|---|---|---|
| `cdn.downloads` | distinct IPs that downloaded a package from download.quiet.md (a resumed or segmented download counts once) | platform, version (`latest` or `x.y.z`), channel, country |
| `cdn.download_requests` | raw GET requests for packages, for comparison | platform, version, channel, status |
| `update.checks` | distinct IPs that fetched an update manifest | platform, country |
| `update.check_requests` | raw manifest requests | platform |
| `daily.downloads` | distinct IPs per package path that got a 200/206, summed per platform family | platform (`Mac`, `Windows`, `Linux`, `Other`), channel |
| `daily.checks` | distinct IPs that requested an update manifest, whatever the status | platform (`all` or a family) |
| `daily.coverage` | 1: marks a day that has the `daily.*` metrics | none |
| `github.downloads_total` | GitHub's download counter when collected | platform, version, asset |
| `github.downloads` | the counter's increase since the previous day's snapshot | platform, version |
| `github.stars`, `github.forks`, `github.watchers`, `github.open_issues` | the repository's counts when collected (open issues include PRs) | none |
| `site.visitors` | distinct IPs that loaded quiet.md's home page, crawlers left out | country |
| `site.pageviews` | home page loads, crawlers left out | none |
| `cli.users` | installs of `quiet` that reported using it that day (random install id) | os |
| `cli.new` | installs whose first reported day it is | none |
| `cli.runs` | `quiet` commands run (MCP tool calls included; a script is one) | os |
| `cli.errors` | commands that failed | none |
| `cli.commands` | runs per command name | command |
| `cli.versions` | installs per `quiet` version | version |

`channel` is `update` for the in-app updater (curl fetching the versioned `v<x.y.z>/` package its manifest names) and `cdn` for everything else on download.quiet.md: the site, the README and release-note links opened in a browser. The free plan's analytics have no query string or referer, so those links are not told apart.

`cli_reports` keeps one row per UTC day and install: version, os, arch, runs, errors and runs per command (JSON). A client sends finished days when it next runs, so each collection recomputes the `cli.*` metrics of the last 7 days. The route stores no IP and checks every report (shape, a uuid v4 id, days from 40 back to tomorrow, known command names); anything else is a 400.

`downloads_daily` sums `cdn.downloads` and `github.downloads` per day, platform and channel (`cdn`, `update`, `github`). `runs` logs every collection with its row count or error.

```sh
npx wrangler@4 d1 execute quiet-md-stats --remote --command "SELECT * FROM downloads_daily ORDER BY day DESC LIMIT 20"
npx wrangler@4 d1 execute quiet-md-stats --remote --command "SELECT * FROM runs ORDER BY id DESC LIMIT 10"
```

## Setup

From this directory:

```sh
npx wrangler@4 d1 migrations apply quiet-md-stats --remote   # schema changes: add a file to migrations/ (0002: cli_reports)
npx wrangler@4 secret put CF_API_TOKEN                       # API token: Zone › Analytics › Read on quiet.md
npx wrangler@4 secret put GITHUB_TOKEN                       # optional: any token, for GitHub's higher rate limit
npx wrangler@4 deploy                                        # also attaches telemetry.quiet.md
```

Each run fills in every Cloudflare day since the last successful one, up to `MAX_DAYS` (3) per run. After a longer gap, or for the first fill, run the same collector from a machine:

```sh
CF_API_TOKEN=<token with Analytics Read and D1 Edit> node backfill.mjs [days]
```

Cloudflare only answers for the last 30 days; anything older that was missed is gone.
