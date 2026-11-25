//! How much `quiet` is used, for stats.quiet.md: per UTC day, how many
//! commands ran, which ones (by name only), and how many failed, under a
//! random id for this computer. Nothing about the library, the documents or
//! the arguments is kept or sent.
//!
//! A run only counts, in `<data dir>/quiet-md/cli-usage.json`; finished days
//! go out at most once an hour, in a background `quiet` that posts them with
//! `curl` (as the app's update check fetches) and forgets them once accepted.
//! Off with `quiet telemetry off`, `DO_NOT_TRACK` or `QUIET_NO_TELEMETRY`,
//! in CI, and when 自动检查更新 is off in the app: then nothing is written either.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::fail::{R, fail};

const URL: &str = "https://telemetry.quiet.md/v1/cli";
/// Days kept waiting to be sent (a computer offline for longer loses the oldest).
const KEEP: usize = 31;
/// The commands counted by name; anything else counts as `other`.
const NAMES: &[&str] = &["ls", "find", "outline", "read", "blocks", "links", "check", "new", "mkdir", "insert", "replace", "delete", "move", "set",
    "rename", "mv", "rm", "table", "table add", "table update", "table delete", "board", "board add", "board rm", "history", "restore", "trash",
    "untrash", "import", "run", "call", "skill", "skill install", "help", "telemetry", "mcp"];

const NOTICE: &str = "quiet counts how often its commands run (which ones and how many, under a random id; never your notes or arguments) and sends those counts once a day to stats.quiet.md. `quiet telemetry off` turns it off; `quiet help telemetry` says more.";

fn file() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("QUIET_TELEMETRY_FILE") { return Some(PathBuf::from(p)); }
    Some(dirs::data_local_dir()?.join("quiet-md").join("cli-usage.json"))
}

fn url() -> String { std::env::var("QUIET_TELEMETRY_URL").unwrap_or_else(|_| URL.to_string()) }

fn set(v: &str) -> bool { !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false") }

/// Why counting is off, if it is; the file's own switch aside.
fn blocked() -> Option<&'static str> {
    let env = |k: &str| std::env::var(k).is_ok_and(|v| set(&v));
    if env("DO_NOT_TRACK") { return Some("DO_NOT_TRACK is set"); }
    if env("QUIET_NO_TELEMETRY") { return Some("QUIET_NO_TELEMETRY is set"); }
    if env("CI") { return Some("running in CI"); }
    // The app's 自动检查更新 off: the user wants it to stay offline.
    // 应用级设置 since 主页; before, the library's `.quiet/`.
    let prefs = [fdl_core::store::Store::settings_dir(), fdl_core::store::Store::legacy_dir().join(".quiet")].map(|d| d.join("preferences.json"));
    if let Some(p) = prefs.iter().find_map(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        && p.get("auto_update").and_then(Value::as_bool) == Some(false) {
        return Some("自动检查更新 (Check for updates automatically) is off in Quiet.md");
    }
    None
}

fn read(path: &Path) -> Value { std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({})) }

fn write(path: &Path, v: &Value) {
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, serde_json::to_vec_pretty(v).unwrap_or_default()).is_ok() { let _ = std::fs::rename(&tmp, path); }
}

fn now() -> u64 { fdl_core::time::now() }

/// `YYYY-MM-DD` in UTC.
fn day(secs: u64) -> String {
    let z = (secs / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + i64::from(m <= 2))
}

/// The command a command line is, by name: `table add`, `read`, `other`.
pub fn name(raw: &[String]) -> String {
    let pos: Vec<String> = crate::parse(raw.to_vec()).map(|a| a.pos).unwrap_or_default();
    let pos: Vec<&str> = pos.iter().map(String::as_str).collect();
    let two = pos.iter().take(2).copied().collect::<Vec<_>>().join(" ");
    if NAMES.contains(&two.as_str()) { return two; }
    match pos.first() { None => "help".into(), Some(c) => match *c { "cat" => "read".into(), "search" => "find".into(), "add" => "insert".into(), "guide" => "skill".into(), c if NAMES.contains(&c) => c.into(), _ => "other".into() } }
}

/// Count one run of `command`; `ok`: it succeeded. Says once, on stderr,
/// that it counts; starts a send when a finished day is waiting.
pub fn count(command: &str, ok: bool) {
    if blocked().is_some() { return; }
    let Some(path) = file() else { return };
    let mut v = read(&path);
    if v.get("enabled").and_then(Value::as_bool) == Some(false) { return; }
    let o = v.as_object_mut().unwrap();
    if !o.get("id").is_some_and(Value::is_string) { o.insert("id".into(), json!(uuid::Uuid::new_v4().to_string())); }
    if o.get("noticed").and_then(Value::as_bool) != Some(true) { eprintln!("{NOTICE}"); o.insert("noticed".into(), json!(true)); }
    let today = day(now());
    let days = o.entry("days").or_insert_with(|| json!({}));
    let d = days.as_object_mut().unwrap().entry(today.clone()).or_insert_with(|| json!({ "runs": 0, "errors": 0, "commands": {} }));
    d["runs"] = json!(d["runs"].as_u64().unwrap_or(0) + 1);
    if !ok { d["errors"] = json!(d["errors"].as_u64().unwrap_or(0) + 1); }
    let c = d["commands"].as_object_mut().unwrap();
    let n = c.get(command).and_then(Value::as_u64).unwrap_or(0) + 1;
    c.insert(command.to_string(), json!(n));
    // The oldest go first if the computer has been offline a long time.
    let days = o["days"].as_object_mut().unwrap();
    let mut keys: Vec<String> = days.keys().cloned().collect();
    keys.sort();
    for k in keys.iter().take(keys.len().saturating_sub(KEEP)) { days.remove(k); }
    let due = keys.iter().any(|k| k < &today) && o.get("next_try").and_then(Value::as_u64).unwrap_or(0) <= now();
    if due { o.insert("next_try".into(), json!(now() + 3600)); }
    write(&path, &v);
    if due && let Ok(me) = std::env::current_exe() {
        let mut c = Command::new(me);
        c.arg("__send-usage").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000 | 0x0000_0008); } // no window, detached
        let _ = c.spawn();
    }
}

/// `quiet __send-usage` (started by `count`): post the finished days and forget the ones accepted.
pub fn send() -> i32 {
    if blocked().is_some() { return 0; }
    let Some(path) = file() else { return 0 };
    let v = read(&path);
    if v.get("enabled").and_then(Value::as_bool) == Some(false) { return 0; }
    let (Some(id), Some(days)) = (v.get("id").and_then(Value::as_str), v.get("days").and_then(Value::as_object)) else { return 0 };
    let today = day(now());
    let done: Vec<Value> = days.iter().filter(|(k, _)| k.as_str() < today.as_str()).map(|(k, d)| json!({ "day": k, "runs": d["runs"], "errors": d["errors"], "commands": d["commands"] })).collect();
    if done.is_empty() { return 0; }
    let body = json!({ "v": 1, "id": id, "version": env!("CARGO_PKG_VERSION"), "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "days": done });
    let testing = std::env::var_os("QUIET_TELEMETRY_URL").is_some();
    let proto = if testing { "=https,http" } else { "=https" };
    let mut c = Command::new("curl");
    c.args(["-fsS", "--proto", proto, "--max-time", "15", "-X", "POST", "-H", "content-type: application/json", "--data-binary", "@-", &url()])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
    let Ok(mut child) = c.spawn() else { return 1 };
    if let Some(mut stdin) = child.stdin.take() { use std::io::Write; let _ = stdin.write_all(body.to_string().as_bytes()); }
    if !child.wait().is_ok_and(|s| s.success()) { return 1; }
    // Forget what was sent; days counted meanwhile (today) stay.
    let mut now_v = read(&path);
    if let Some(days) = now_v.get_mut("days").and_then(Value::as_object_mut) {
        for d in &done { if let Some(k) = d["day"].as_str() { days.remove(k); } }
    }
    write(&path, &now_v);
    0
}

/// `quiet telemetry [on|off|status]`.
pub fn command(arg: Option<&str>) -> R<String> {
    let path = file().ok_or_else(|| fail("io", "no data folder to keep the counts in"))?;
    let mut v = read(&path);
    match arg {
        Some("off") => {
            // Off forgets the id and what was waiting to be sent.
            write(&path, &json!({ "enabled": false, "noticed": true }));
            Ok("usage counting is off; nothing more is counted or sent\n".into())
        }
        Some("on") => {
            v["enabled"] = json!(true);
            v["noticed"] = json!(true);
            write(&path, &v);
            Ok(match blocked() { Some(why) => format!("usage counting is on, but nothing is counted while {why}\n"), None => "usage counting is on\n".into() })
        }
        None | Some("status") => {
            let on = v.get("enabled").and_then(Value::as_bool) != Some(false);
            let mut out = match (on, blocked()) {
                (false, _) => "usage counting: off (`quiet telemetry on` turns it on)\n".to_string(),
                (true, Some(why)) => format!("usage counting: off while {why}\n"),
                (true, None) => "usage counting: on (`quiet telemetry off` turns it off)\n".to_string(),
            };
            out += &format!("counts kept in {}\n", path.display());
            if let Some(id) = v.get("id").and_then(Value::as_str) { out += &format!("id {id}\n"); }
            for (k, d) in v.get("days").and_then(Value::as_object).into_iter().flatten() {
                out += &format!("  {k}: {} runs, {} failed  {}\n", d["runs"], d["errors"], d["commands"]);
            }
            out += &format!("sent to {} once a day, as JSON: id, version, os, arch, and per day the counts above\n", url());
            Ok(out)
        }
        Some(x) => Err(fail("usage", format!("telemetry {x}: use on, off or status"))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_days() {
        assert_eq!(super::day(0), "1970-01-01");
        assert_eq!(super::day(951_782_400), "2000-02-29");
        assert_eq!(super::day(1_795_320_000), "2026-11-22");
    }

    #[test]
    fn names_only() {
        let n = |s: &str| super::name(&s.split(' ').map(String::from).collect::<Vec<_>>());
        assert_eq!(n("table add 书单 --set a=1"), "table add");
        assert_eq!(n("--json read 周报#3"), "read");
        assert_eq!(n("cat x"), "read");
        assert_eq!(n("secret-thing"), "other");
    }
}
