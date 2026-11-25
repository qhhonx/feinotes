//! Wall-clock time and local civil dates, without a time-zone database:
//! the offset comes from `date +%z` once per run.

/// Wall-clock unix seconds.
pub fn now() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Local (fixed +08:00 fallback when TZ unknown) civil time from a unix timestamp.
pub fn civil(secs: u64) -> (i64, u32, u32, u32, u32) {
    let off = local_offset();
    let t = secs as i64 + off;
    let days = t.div_euclid(86_400);
    let rem = t.rem_euclid(86_400);
    // Howard Hinnant's days-from-civil inverse.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Hinnant's
/// days-from-civil).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix seconds of a local date and time (inverse of `civil`).
pub fn local_to_unix(y: i64, m: u32, d: u32, h: u32, mi: u32) -> i64 {
    days_from_civil(y, m, d) * 86_400 + (h * 3600 + mi * 60) as i64 - local_offset()
}

/// Unix seconds of a date as front matter writes one: `2024-03-05` or
/// `2024/03/05`, then optionally ` 14:30`, `T14:30:00`, fractions of a
/// second, and a zone (`Z`, `+08:00`, `+0800`, `-5`); quoted or not. With no
/// zone it is local time, and a date alone is local midnight. None for
/// anything else, or a date that does not exist.
pub fn parse_date(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_prefix(['"', '\'']).and_then(|r| r.strip_suffix(['"', '\''])).unwrap_or(s).trim();
    let num = |t: &str, min: usize, max: usize| (t.len() >= min && t.len() <= max && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.parse::<u32>().ok()).flatten();
    let (date, rest) = match s.find(['T', 't', ' ']) { Some(i) => (&s[..i], s[i + 1..].trim_start()), None => (s, "") };
    let sep = if date.contains('/') { '/' } else { '-' };
    let mut d = date.split(sep);
    let (y, m, day) = (num(d.next()?, 4, 4)? as i64, num(d.next()?, 1, 2)?, num(d.next()?, 1, 2)?);
    if d.next().is_some() || !(1..=12).contains(&m) || day == 0 || day > days_in_month(y, m) { return None; }
    let (mut h, mut mi, mut sec, mut zone) = (0, 0, 0, None);
    if !rest.is_empty() {
        // The time, then what is left is the zone.
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == ':' || c == '.')).unwrap_or(rest.len());
        let (time, z) = (&rest[..end], rest[end..].trim());
        let mut t = time.split(':');
        h = num(t.next()?, 1, 2)?;
        mi = num(t.next()?, 2, 2)?;
        if let Some(x) = t.next() { sec = num(x.split('.').next()?, 2, 2)?; }
        if t.next().is_some() || h > 23 || mi > 59 || sec > 60 { return None; }
        if z == "Z" || z == "z" { zone = Some(0); }
        else if !z.is_empty() {
            let sign = match z.as_bytes()[0] { b'+' => 1, b'-' => -1, _ => return None };
            let z = &z[1..];
            let (zh, zm) = match z.split_once(':') { Some((a, b)) => (num(a, 1, 2)?, num(b, 2, 2)?), None if z.len() == 4 => (num(&z[..2], 2, 2)?, num(&z[2..], 2, 2)?), None => (num(z, 1, 2)?, 0) };
            if zh > 14 || zm > 59 { return None; }
            zone = Some(sign * (zh as i64 * 3600 + zm as i64 * 60));
        }
    }
    let t = match zone {
        Some(off) => days_from_civil(y, m, day) * 86_400 + (h * 3600 + mi * 60 + sec) as i64 - off,
        None => local_to_unix(y, m, day, h, mi) + sec as i64,
    };
    u64::try_from(t).ok()
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// 0 = Monday … 6 = Sunday.
pub fn weekday(y: i64, m: u32, d: u32) -> u32 {
    (days_from_civil(y, m, d) + 3).rem_euclid(7) as u32
}

/// Wall-clock unix milliseconds.
pub fn now_ms() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub fn local_offset() -> i64 {
    use std::sync::OnceLock;
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        // The browser knows the zone; there is no `date` to run.
        #[cfg(target_family = "wasm")]
        return -(js_sys::Date::new_0().get_timezone_offset() as i64) * 60;
        // `date +%z` prints e.g. +0800.
        let mut date = std::process::Command::new("date");
        // On Windows only a `date.exe` on PATH (Git's) answers: without a console window.
        #[cfg(target_os = "windows")]
        { use std::os::windows::process::CommandExt; date.creation_flags(0x0800_0000); }
        date.arg("+%z")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let s = s.trim();
                let sign = if s.starts_with('-') { -1 } else { 1 };
                let digits = s.trim_start_matches(['+', '-']);
                let h: i64 = digits.get(0..2)?.parse().ok()?;
                let m: i64 = digits.get(2..4)?.parse().ok()?;
                Some(sign * (h * 3600 + m * 60))
            })
            .unwrap_or(8 * 3600)
    })
}

#[cfg(test)]
mod tests {
    use super::{local_to_unix, parse_date};

    #[test]
    fn front_matter_dates() {
        let local = |y, m, d, h, mi| local_to_unix(y, m, d, h, mi) as u64;
        let utc = 1_709_649_000; // 2024-03-05T14:30:00Z
        for (s, want) in [
            ("2024-03-05", Some(local(2024, 3, 5, 0, 0))),
            ("2024/03/05", Some(local(2024, 3, 5, 0, 0))),
            ("2024-3-5", Some(local(2024, 3, 5, 0, 0))),
            ("2024-03-05 14:30", Some(local(2024, 3, 5, 14, 30))),
            ("2024-03-05 14:30:00", Some(local(2024, 3, 5, 14, 30))),
            ("2024-03-05T14:30:07", Some(local(2024, 3, 5, 14, 30) + 7)),
            ("2024-03-05T14:30:00Z", Some(utc)),
            ("2024-03-05t14:30:00.123z", Some(utc)),
            ("2024-03-05T14:30:00+08:00", Some(utc - 8 * 3600)),
            ("2024-03-05T14:30:00+0800", Some(utc - 8 * 3600)),
            ("2024-03-05T14:30:00-05:30", Some(utc + 5 * 3600 + 1800)),
            // YAML's own timestamps: a space before the zone, an hour alone.
            ("2024-03-05 14:30:00.10 -5", Some(utc + 5 * 3600)),
            ("\"2024-03-05T14:30:00Z\"", Some(utc)),
            ("'2024-03-05'", Some(local(2024, 3, 5, 0, 0))),
            ("  2024-03-05T14:30:00Z  ", Some(utc)),
        ] {
            assert_eq!(parse_date(s), want, "{s}");
        }
        for bad in ["", "soon", "2024", "2024-03", "2024-13-01", "2024-02-30", "2023-02-29", "24-03-05", "2024-03-05 25:00", "2024-03-05 14:3", "2024-03-05T14:30:00+15:00", "2024-03-05 tomorrow", "2024.03.05", "2024-03-05-06", "1969-12-31T00:00:00Z", "[2024-03-05]"] {
            assert_eq!(parse_date(bad), None, "{bad}");
        }
        assert_eq!(parse_date("2024-02-29"), Some(local(2024, 2, 29, 0, 0)), "a leap day");
    }
}
