//! GitHub Releases update check and semantic version comparison.

use serde_json::Value;

use crate::launch::ProxyConfig;

pub(crate) const GITHUB_REPO_URL: &str = "https://github.com/Summe-rsnow/Wonton";
pub(crate) const GITHUB_RELEASES_URL: &str = "https://github.com/Summe-rsnow/Wonton/releases";
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Summe-rsnow/Wonton/releases/latest";

/// Parsed metadata of a published GitHub release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseInfo {
    pub(crate) tag_name: String,
    pub(crate) version: String,
    pub(crate) html_url: String,
    pub(crate) published_date: Option<String>,
    pub(crate) notes: Option<String>,
}

/// Result of comparing the current app version against GitHub Releases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateCheckResult {
    UpToDate { latest: String },
    Available(ReleaseInfo),
    NoRelease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PrePart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrePart {
    Num(u64),
    Text(String),
}

impl Ord for PrePart {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Num(a), Self::Num(b)) => a.cmp(b),
            (Self::Num(_), Self::Text(_)) => std::cmp::Ordering::Less,
            (Self::Text(_), Self::Num(_)) => std::cmp::Ordering::Greater,
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for PrePart {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => std::cmp::Ordering::Equal,
                // A normal version has higher precedence than a pre-release version.
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_semver(raw: &str) -> Option<SemVer> {
    let trimmed = raw.trim();
    let no_prefix = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let core_and_pre = no_prefix.split('+').next().unwrap_or(no_prefix);
    let (core, pre_raw) = match core_and_pre.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core_and_pre, None),
    };
    let mut nums = core.split('.');
    let major = nums.next()?.parse::<u64>().ok()?;
    let minor = nums.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = nums.next().unwrap_or("0").parse::<u64>().ok()?;
    if nums.next().is_some() {
        return None;
    }
    let pre = pre_raw
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.split('.')
                .map(|part| match part.parse::<u64>() {
                    Ok(n) => PrePart::Num(n),
                    Err(_) => PrePart::Text(part.to_owned()),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(SemVer { major, minor, patch, pre })
}

/// Normalizes a tag like `"v1.1.0"` into `"1.1.0"`.
pub(crate) fn normalize_version(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
        .to_owned()
}

/// Returns `true` when `candidate` is strictly newer than `current`.
pub(crate) fn is_newer_version(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some(cur), Some(cand)) => cand > cur,
        _ => false,
    }
}

/// Parses GitHub's `/releases/latest` JSON payload into [`ReleaseInfo`].
pub(crate) fn parse_github_release(body: &str) -> Result<ReleaseInfo, String> {
    let root: Value =
        serde_json::from_str(body).map_err(|e| format!("解析发布信息失败：{e}"))?;
    let tag_name = root
        .get("tag_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "发布信息里缺少 tag_name".to_owned())?
        .to_owned();
    let version = normalize_version(&tag_name);
    let html_url = root
        .get("html_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
        .unwrap_or(GITHUB_RELEASES_URL)
        .to_owned();
    let published_date = root
        .get("published_at")
        .and_then(Value::as_str)
        .and_then(|s| s.split('T').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let notes = root
        .get("body")
        .and_then(Value::as_str)
        .map(summarize_release_notes)
        .filter(|s| !s.is_empty());

    Ok(ReleaseInfo {
        tag_name,
        version,
        html_url,
        published_date,
        notes,
    })
}

fn summarize_release_notes(raw: &str) -> String {
    let lines: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .map(|line| {
            line.trim_start_matches('#')
                .trim_start_matches('-')
                .trim_start_matches('*')
                .trim()
                .to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect();
    lines.join(" · ")
}

/// Evaluates an HTTP response from GitHub's latest release endpoint against `current_version`.
pub(crate) fn evaluate_release_response(
    current_version: &str,
    status: u16,
    body: &str,
) -> Result<UpdateCheckResult, String> {
    if status == 404 {
        return Ok(UpdateCheckResult::NoRelease);
    }
    if !(200..300).contains(&status) {
        return Err(format!("GitHub 返回 HTTP {status}"));
    }
    let release = parse_github_release(body)?;
    if is_newer_version(current_version, &release.version) {
        Ok(UpdateCheckResult::Available(release))
    } else {
        Ok(UpdateCheckResult::UpToDate {
            latest: release.version,
        })
    }
}

/// Checks GitHub Releases for a newer version of Wonton through the configured proxy.
pub(crate) async fn check_latest_release(
    current_version: &str,
    proxy: ProxyConfig,
) -> Result<UpdateCheckResult, String> {
    let user_agent = format!("Wonton/{current_version}");
    let headers = [
        ("Accept", "application/vnd.github+json"),
        ("User-Agent", user_agent.as_str()),
    ];
    let (status, body) =
        crate::google::http_get(GITHUB_LATEST_RELEASE_API, &headers, proxy).await?;
    evaluate_release_response(current_version, status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_versions_accurately() {
        assert!(!is_newer_version("1.1.0", "1.1.0"));
        assert!(!is_newer_version("1.1.0", "v1.1.0"));
        assert!(!is_newer_version("1.1.0", "v1.0.9"));
        assert!(is_newer_version("1.1.0", "v1.1.1"));
        assert!(is_newer_version("1.1.0", "1.2.0"));
        assert!(is_newer_version("1.1.0", "2.0.0"));
        assert!(is_newer_version("1.1.0", "v1.2.0-rc.1"));
        assert!(is_newer_version("1.2.0-rc.1", "1.2.0"));
        assert!(!is_newer_version("1.2.0", "1.2.0-rc.1"));
        assert!(!is_newer_version("1.1.0", "invalid"));
    }

    #[test]
    fn parses_github_release_payload_and_evaluates_update() {
        let json = r###"{
            "tag_name": "v1.2.0",
            "html_url": "https://github.com/Summe-rsnow/Wonton/releases/tag/v1.2.0",
            "published_at": "2026-12-01T10:00:00Z",
            "body": "## 更新内容\n- 新增功能 A\n- 优化性能 B"
        }"###;
        let info = parse_github_release(json).expect("release info");
        assert_eq!(info.tag_name, "v1.2.0");
        assert_eq!(info.version, "1.2.0");
        assert_eq!(
            info.html_url,
            "https://github.com/Summe-rsnow/Wonton/releases/tag/v1.2.0"
        );
        assert_eq!(info.published_date.as_deref(), Some("2026-12-01"));
        assert_eq!(
            info.notes.as_deref(),
            Some("更新内容 · 新增功能 A · 优化性能 B")
        );

        assert!(matches!(
            evaluate_release_response("1.1.0", 200, json).unwrap(),
            UpdateCheckResult::Available(ref r) if r.version == "1.2.0"
        ));
        assert_eq!(
            evaluate_release_response("1.2.0", 200, json).unwrap(),
            UpdateCheckResult::UpToDate { latest: "1.2.0".to_owned() }
        );
        assert_eq!(
            evaluate_release_response("1.1.0", 404, "{}").unwrap(),
            UpdateCheckResult::NoRelease
        );
        assert!(evaluate_release_response("1.1.0", 500, "{}").is_err());
    }
}
