//! Preserve the CloudKit invitation address inside the versioned Fei Notes link.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

const WEB: &str = "https://feinotes.app/share/?";
const APP: &str = "feinotes://share/?";
const V2: &str = "v=2&u=";
const LEGACY_REGIONS: [(&str, &str); 2] = [("c=", "https://www.icloud.com.cn/share/"), ("g=", "https://www.icloud.com/share/")];
const MAX: usize = 4096;
const ENCODED_MAX: usize = (MAX * 4).div_ceil(3);

fn apple(value: &str) -> bool {
    let hosts = ["https://www.icloud.com/share/", "https://icloud.com/share/",
        "https://www.icloud.com.cn/share/", "https://icloud.com.cn/share/"];
    value.len() <= MAX && value.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
        && hosts.iter().any(|prefix| value.strip_prefix(prefix).is_some_and(|suffix| {
            let path = suffix.split(['?', '#']).next().unwrap_or_default();
            !path.is_empty() && !path.starts_with('/') && path.split('/').all(|part| {
                let lower = part.to_ascii_lowercase();
                !lower.contains("%2f") && !lower.contains("%5c") && !matches!(lower.replace("%2e", ".").as_str(), "." | "..")
            })
        }))
}

pub(crate) fn cloud(value: &str) -> Result<String> {
    let value = value.trim();
    if apple(value) { return Ok(value.into()); }
    let query = value.strip_prefix(WEB).or_else(|| value.strip_prefix(APP)).unwrap_or_default();
    for (region, host) in LEGACY_REGIONS {
        if let Some(id) = query.strip_prefix(region) {
            ensure!(!id.is_empty() && id.len() <= 256
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')), "Invalid invitation link");
            return Ok(format!("{host}{id}"));
        }
    }
    let token = query.strip_prefix(V2).unwrap_or_default();
    ensure!(!token.is_empty() && token.len() <= ENCODED_MAX, "Invalid invitation link");
    let bytes = URL_SAFE_NO_PAD.decode(token)?;
    let decoded = String::from_utf8(bytes)?;
    ensure!(apple(&decoded), "Invalid iCloud invitation address");
    Ok(decoded)
}

pub(super) fn pasted_invitation(value: &str) -> Result<String> {
    if let Ok(link) = cloud(value) { return Ok(link); }
    // UIKit can remove the line break when a full invitation is pasted.
    let value = value.trim();
    ensure!(value.starts_with("邀请你在飞记中协作《") || value.starts_with("Join me in Fei Notes: "), "Invalid invitation link");
    if let Some((_, link)) = value.rsplit_once('\n') { return cloud(link); }
    value.match_indices("https://").find_map(|(start, _)| cloud(&value[start..]).ok())
        .ok_or_else(|| anyhow::anyhow!("Invalid invitation link"))
}

pub(crate) fn branded(value: &str) -> Result<String> {
    Ok(format!("{WEB}{V2}{}", URL_SAFE_NO_PAD.encode(cloud(value)?)))
}

pub(crate) fn invitation_url(value: &str) -> Option<String> { branded(value).ok() }

pub(crate) fn incoming(value: &str) -> bool { value.starts_with("https://feinotes.app/share/") || value.starts_with("feinotes://share/") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_invitation_preserves_the_exact_apple_url() -> Result<()> {
        for apple in ["https://www.icloud.com/share/0synthetic",
            "https://icloud.com.cn/share/0synthetic?source=icloud#%E6%96%87%E6%A1%A3",
            "https://www.icloud.com/share/0synthetic?callback=https://example.test/#Document"] {
            let web = branded(apple)?;
            assert_eq!(cloud(&web)?, apple);
            assert_eq!(cloud(&web.replacen(WEB, APP, 1))?, apple);
            assert_eq!(branded(&web)?, web);
            assert_eq!(pasted_invitation(&format!("邀请你在飞记中协作《验收样文》\n{web}"))?, apple);
            assert_eq!(pasted_invitation(&format!("Join me in Fei Notes: Test document {apple}"))?, apple);
        }
        assert_eq!(branded("https://www.icloud.com/share/0synthetic")?,
            "https://feinotes.app/share/?v=2&u=aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS8wc3ludGhldGlj");
        Ok(())
    }

    #[test]
    fn legacy_invitation_opens_but_copies_as_v2() -> Result<()> {
        for (region, host) in LEGACY_REGIONS {
            let raw = format!("{host}0synthetic_ABC-123");
            for prefix in [WEB, APP] {
                let legacy = format!("{prefix}{region}0synthetic_ABC-123");
                assert_eq!(cloud(&legacy)?, raw);
                assert_eq!(branded(&legacy)?, branded(&raw)?);
                assert_eq!(pasted_invitation(&format!("Join me in Fei Notes: Sample\n{legacy}"))?, raw);
            }
            let legacy = format!("{WEB}{region}0synthetic");
            assert_eq!(pasted_invitation(&format!("Join me in Fei Notes: Sample {legacy}"))?, format!("{host}0synthetic"));
        }
        Ok(())
    }

    #[test]
    fn invalid_apple_destinations_and_noncanonical_payloads_are_rejected() {
        for bad in ["https://elsewhere.example/share/x", "https://www.icloud.com@elsewhere.example/share/x",
            "https://www.icloud.com:444/share/x", "https://www.icloud.com/share/?x=1",
            "https://www.icloud.com/share/#value", "https://www.icloud.com/share/../x",
            "https://www.icloud.com/share/%2e./x", "https://www.icloud.com/share/foo%2fbar",
            "https://www.icloud.com/share/foo%5Cbar", "https://www.icloud.com/share/ foo"] {
            assert!(invitation_url(bad).is_none());
            assert!(cloud(&format!("{WEB}{V2}{}",URL_SAFE_NO_PAD.encode(bad))).is_err());
        }
        for bad in ["=", "Zg=", "Zh", "////", "YWJj&u=bQ", "YWJj#tag"] {
            assert!(cloud(&format!("{WEB}{V2}{bad}")).is_err());
        }
        assert!(cloud(&format!("{WEB}{V2}{}", "a".repeat(ENCODED_MAX + 1))).is_err());
        for bad in ["c=", "g=", "c=c&g=g", "g=id#fragment", "c=id/path", "g=%61"] {
            assert!(cloud(&format!("{WEB}{bad}")).is_err());
        }
        assert!(cloud(&format!("{APP}g={}", "a".repeat(257))).is_err());
    }
}
