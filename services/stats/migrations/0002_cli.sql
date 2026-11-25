-- What `quiet` (the command line tool) reports once a day: per UTC day and random
-- install id, how many commands ran, how many failed, and how often each command
-- ran (by name). No IP, no library content. The cron turns it into cli.* metrics.
-- A day reported again (a resend) replaces the earlier report.
CREATE TABLE cli_reports (
  day TEXT NOT NULL,          -- YYYY-MM-DD, UTC, as the client counted it
  install TEXT NOT NULL,      -- random id the client made (uuid v4)
  version TEXT NOT NULL,      -- quiet's version, e.g. 0.1.15
  os TEXT NOT NULL,           -- macos, windows, linux
  arch TEXT NOT NULL,         -- aarch64, x86_64
  runs INTEGER NOT NULL,
  errors INTEGER NOT NULL,
  commands TEXT NOT NULL,     -- JSON object: command name -> runs
  received_at TEXT NOT NULL,
  PRIMARY KEY (day, install)
);
CREATE INDEX cli_reports_install ON cli_reports (install, day);
