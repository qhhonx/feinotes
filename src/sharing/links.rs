//! Map the CloudKit share identifier and region directly, without a lookup service.
use anyhow::{Result, ensure};

const WEB: &str = "https://feinotes.app/share/?";
const APP: &str = "feinotes://share/?";
const MAX: usize = 4096;
const REGIONS: [(&str, &str); 2] = [("c=", "https://www.icloud.com.cn/share/"), ("g=", "https://www.icloud.com/share/")];

fn compact_token(value: &str) -> bool { !value.is_empty() && value.len() <= 256 && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')) }

fn apple(value: &str) -> bool {
    value.len() <= MAX && value.bytes().all(|b| b.is_ascii() && !b.is_ascii_control() && !b.is_ascii_whitespace() && b != b'\\')
        && ["https://www.icloud.com/share/", "https://icloud.com/share/", "https://www.icloud.com.cn/share/", "https://icloud.com.cn/share/"]
            .iter().any(|prefix| value.strip_prefix(prefix).is_some_and(|path| !path.is_empty() && !path.starts_with(['?', '#', '/']) && !path.split('/').any(|part| matches!(part, "." | ".."))))
}

pub(crate) fn cloud(value: &str) -> Result<String> {
    let value = value.trim();
    if apple(value) { return Ok(value.into()); }
    ensure!(value.len() <= MAX * 2, "Invalid invitation link");
    let token = value.strip_prefix(WEB).or_else(|| value.strip_prefix(APP)).ok_or_else(|| anyhow::anyhow!("Invalid invitation link"))?;
    for (region, host) in REGIONS {
        if let Some(token) = token.strip_prefix(region) {
            ensure!(compact_token(token), "Invalid invitation link");
            return Ok(format!("{host}{token}"));
        }
    }
    anyhow::bail!("Invalid invitation link")
}

pub(crate) fn branded(value: &str) -> Result<String> {
    let raw = cloud(value)?;
    for (region, host) in REGIONS {
        if let Some(token) = raw.strip_prefix(host).filter(|token| compact_token(token)) { return Ok(format!("{WEB}{region}{token}")); }
    }
    anyhow::bail!("Invalid invitation link")
}

pub(crate) fn incoming(value: &str) -> bool { value.starts_with("https://feinotes.app/share/") || value.starts_with("feinotes://share/") }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_preserve_regional_urls_and_reject_other_destinations() -> Result<()> {
        for apple in ["https://www.icloud.com/share/0synthetic", "https://www.icloud.com.cn/share/0synthetic"] {
            let public = branded(apple)?;
            assert!(public.starts_with(WEB));
            assert_eq!(cloud(&public)?, apple);
            assert_eq!(cloud(&public.replacen(WEB, APP, 1))?, apple);
            assert_eq!(branded(&public)?, public);
        }
        for bad in ["https://evil.example/share/x", "https://www.icloud.com@evil.example/share/x", "https://www.icloud.com/share/", "https://www.icloud.com/share/\nfoo", "https://feinotes.app.evil.example/share/?c=token", "https://feinotes.app/share/v2_YQ", "feinotes://share/v1_", "feinotes://share/v1_aHR0cHM6Ly9ldmlsLmV4YW1wbGU", "https://www.icloud.com/share/../private", "https://www.icloud.com/share/token\\x", "https://feinotes.app/share/?c=", "https://feinotes.app/share/?c=token&g=token", "feinotes://share/?g=token/path", "https://feinotes.app/share/?g=../x"] {
            assert!(cloud(bad).is_err(), "accepted {bad}");
        }
        Ok(())
    }

    #[test]
    fn compact_links_keep_the_region_without_an_encoded_transport() -> Result<()> {
        for (region, host) in REGIONS {
            let raw = format!("{host}0synthetic_ABC-123");
            let short = format!("{WEB}{region}0synthetic_ABC-123");
            assert_eq!(branded(&raw)?, short);
            assert_eq!(cloud(&short)?, raw);
            assert_eq!(cloud(&short.replacen(WEB, APP, 1))?, raw);
            assert!(cloud(&format!("{WEB}v1_aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS8wc3ludGhldGlj")).is_err());
            assert!(short.len() <= raw.len() + 1);
        }
        Ok(())
    }
}
