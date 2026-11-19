//! Google OAuth and the Antigravity quota endpoint.
//!
//! The client ids and the endpoint order come from Antigravity-Manager (MIT
//! parts: the values), which the user asked to align with. Pure parsing is
//! separated from the requests so `cargo test` covers it without a network.

use std::time::Duration;

use nsh_core::prelude::*;

use crate::launch::ProxyConfig;
use serde_json::Value;

/// The Antigravity Google OAuth client credentials.
const OAUTH_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const OAUTH_CLIENT_SECRET: &str = "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";

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const QUOTA_HOSTS: [&str; 3] = [
    "https://daily-cloudcode-pa.googleapis.com",
    "https://daily-cloudcode-pa.sandbox.googleapis.com",
    "https://cloudcode-pa.googleapis.com",
];
/// The IDE's OAuth user agent.
const USER_AGENT: &str = "vscode/1.X.X (Antigravity/4.3.0)";
/// Official Antigravity Hub egress User-Agent required by Google Cloud Code
/// endpoints (`loadCodeAssist`, `retrieveUserQuotaSummary`, Argon gating).
const HUB_USER_AGENT: &str =
    "antigravity/hub/2.28.0 (aidev_client; os_type=windows; arch=amd64; cl=1008300195)";

/// The scopes Antigravity's own login asks for.
const SCOPES: &str = "openid \
https://www.googleapis.com/auth/cloud-platform \
https://www.googleapis.com/auth/userinfo.email \
https://www.googleapis.com/auth/userinfo.profile \
https://www.googleapis.com/auth/cclog \
https://www.googleapis.com/auth/experimentsandconfigs";

/// A Google token response.
#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
}

/// One quota pool's remaining quota, supporting both 5H and weekly windows.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelQuota {
    pub name: String,
    pub remaining: f32,
    #[serde(default)]
    pub reset: Option<String>,
    #[serde(default)]
    pub weekly_remaining: Option<f32>,
    #[serde(default)]
    pub weekly_reset: Option<String>,
    #[serde(default)]
    pub is_weekly_only: bool,
}

/// Authoritative account subscription tier reported by `loadCodeAssist`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AccountTier {
    Free,
    Pro,
    Ultra,
}

impl AccountTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ultra => "ULTRA",
            Self::Pro => "PRO",
            Self::Free => "FREE",
        }
    }
}

/// Result of refreshing an account's subscription tier and summarized quotas.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountQuotaSnapshot {
    pub tier: Option<AccountTier>,
    pub models: Vec<ModelQuota>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryBucket {
    pub bucket_id: String,
    pub window: String,
    pub display_name: String,
    pub description: String,
    pub remaining_fraction: f32,
    pub reset_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryGroup {
    pub display_name: String,
    pub description: String,
    pub buckets: Vec<SummaryBucket>,
}

/// One entry in Antigravity-Manager's JSON export (`[{email, refresh_token}]`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExportAccount {
    pub email: String,
    pub refresh_token: String,
}

/// One candidate account parsed from an imported JSON file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry {
    pub email: Option<String>,
    pub refresh_token: String,
}

/// The authorization URL a browser opens.
pub fn auth_url(redirect_uri: &str, state: &str) -> String {
    let query = [
        ("client_id", OAUTH_CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", SCOPES),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("include_granted_scopes", "true"),
        ("state", state),
    ]
    .iter()
    .map(|(key, value)| format!("{key}={}", percent_encode(value)))
    .collect::<Vec<_>>()
    .join("&");
    format!("{AUTH_URL}?{query}")
}

/// Reads `code` and `state` from the callback request line
/// (`GET /oauth-callback?code=..&state=.. HTTP/1.1`).
pub fn callback_query(request_line: &str) -> Option<(Option<String>, Option<String>)> {
    let target = request_line.split_whitespace().nth(1)?;
    let query = target.split_once("code=").map(|_| target)?;
    let (_, raw) = query.split_once('?')?;
    let mut code = None;
    let mut state = None;
    for pair in raw.split('&') {
        let (key, value) = pair.split_once('=')?;
        let value = percent_decode(value);
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => return Some((None, state)),
            _ => {}
        }
    }
    Some((code, state))
}

/// Extracts the refresh tokens a user pasted: a JSON array of objects with a
/// `refresh_token`, or any text holding `1//…` tokens. Deduplicated in order.
pub fn parse_tokens(input: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(input.trim()) {
        for item in items {
            if let Some(token) = item.get("refresh_token").and_then(Value::as_str) {
                tokens.push(token.to_owned());
            }
        }
    }
    if tokens.is_empty() {
        let bytes = input.as_bytes();
        let mut at = 0;
        while at + 3 <= bytes.len() {
            if &bytes[at..at + 3] == b"1//" {
                let start = at;
                let mut end = at + 3;
                while end < bytes.len() && is_token_char(bytes[end]) {
                    end += 1;
                }
                if end - start > 20 {
                    tokens.push(input[start..end].to_owned());
                }
                at = end;
            } else {
                at += 1;
            }
        }
    }
    let mut seen = Vec::new();
    for token in tokens {
        let token = token.trim().to_owned();
        if !token.is_empty() && !seen.contains(&token) {
            seen.push(token);
        }
    }
    seen
}

fn is_token_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'/'
}

/// Formats accounts into Antigravity-Manager's pretty JSON array.
pub fn format_export_json(accounts: &[ExportAccount]) -> Result<String, String> {
    serde_json::to_string_pretty(accounts).map_err(|error| format!("生成导出 JSON 失败：{error}"))
}

/// Parses an Antigravity-Manager (or Wonton) JSON export into deduplicated
/// `1//…` refresh-token entries. Accepts `[...]`, `{accounts:[...]}`,
/// `{accounts:{...}}`, `{id:{...}}`, or a single `{email, refresh_token}` object.
pub fn parse_import_entries(input: &str) -> Result<Vec<ImportEntry>, String> {
    let trimmed = input.trim().trim_start_matches('\u{feff}');
    let parsed = serde_json::from_str::<Value>(trimmed)
        .map_err(|_| "JSON 格式无效，请检查文件内容".to_owned())?;

    let mut raw_items: Vec<&Value> = Vec::new();
    match &parsed {
        Value::Array(items) => raw_items.extend(items.iter()),
        Value::Object(map) => {
            if extract_refresh_token(&parsed).is_some() {
                raw_items.push(&parsed);
            } else if let Some(accounts) = map.get("accounts") {
                match accounts {
                    Value::Array(items) => raw_items.extend(items.iter()),
                    Value::Object(inner) => raw_items.extend(inner.values()),
                    _ => {}
                }
            } else {
                raw_items.extend(map.values());
            }
        }
        _ => {}
    }

    let mut entries: Vec<ImportEntry> = Vec::new();
    for item in raw_items {
        let Some(token) = extract_refresh_token(item) else {
            continue;
        };
        let email = extract_email_hint(item);
        if let Some(existing) = entries.iter_mut().find(|entry| entry.refresh_token == token) {
            if existing.email.is_none() {
                existing.email = email;
            }
            continue;
        }
        entries.push(ImportEntry { email, refresh_token: token });
    }

    if entries.is_empty() {
        return Err("未找到有效账号或 refresh_token（需以 1// 开头）".to_owned());
    }
    Ok(entries)
}

fn extract_refresh_token(item: &Value) -> Option<String> {
    let raw = item
        .get("refresh_token")
        .and_then(Value::as_str)
        .or_else(|| item.get("token").and_then(|token| token.get("refresh_token")).and_then(Value::as_str))?;
    let trimmed = raw.trim();
    trimmed.starts_with("1//").then(|| trimmed.to_owned())
}

fn extract_email_hint(item: &Value) -> Option<String> {
    let raw = item
        .get("email")
        .and_then(Value::as_str)
        .or_else(|| item.get("name").and_then(Value::as_str))?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

struct ParsedCatalogItem {
    id: String,
    name: String,
    quota: Option<(f32, Option<String>)>,
}

struct ParsedCatalog {
    items: Vec<ParsedCatalogItem>,
    argon_explicitly_disabled: bool,
}

/// Parses `fetchAvailableModels` once for both raw test assertions and quota assembly.
fn parse_models_catalog(body: &str, enforce_official_sorts: bool) -> Result<ParsedCatalog, String> {
    let root: Value = serde_json::from_str(body).map_err(|e| format!("响应不是 JSON：{e}"))?;
    let models = root
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| "响应里没有 models".to_owned())?;
    let deprecated = root.get("deprecatedModelIds").and_then(Value::as_object);
    let official_supported_ids = if enforce_official_sorts {
        collect_official_supported_ids(&root)
    } else {
        std::collections::HashSet::new()
    };

    let mut argon_explicitly_disabled = false;
    let mut items = Vec::new();
    for (id, model) in models {
        let id_lower = id.trim().to_lowercase();
        let name = model
            .get("displayName")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(id);
        let name_lower = name.trim().to_lowercase();
        let is_disabled = model.get("disabled").and_then(Value::as_bool) == Some(true)
            || id_lower.contains("-disabled")
            || id_lower.ends_with("_disabled")
            || name_lower.contains("-disabled")
            || name_lower.ends_with("_disabled");
        if (id_lower.contains("argon") || name_lower.contains("argon")) && is_disabled {
            argon_explicitly_disabled = true;
        }
        if is_disabled || deprecated.is_some_and(|map| map.contains_key(id)) {
            continue;
        }
        if is_ignored_model(id) || is_ignored_model(name) {
            continue;
        }
        let quota = model.get("quotaInfo").and_then(|info| {
            let reset = info
                .get("resetTime")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let remaining = info
                .get("remainingFraction")
                .and_then(Value::as_f64)
                .or_else(|| reset.as_ref().map(|_| 0.0))? as f32;
            Some((remaining.clamp(0.0, 1.0), reset))
        });
        // Keep enabled models that explicitly report `quotaInfo` even when
        // `agentModelSorts` omits placeholder/variant keys (e.g. Argon).
        if !official_supported_ids.is_empty()
            && !official_supported_ids.contains(&id_lower)
            && quota.is_none()
        {
            continue;
        }
        items.push(ParsedCatalogItem {
            id: id.clone(),
            name: name.to_owned(),
            quota,
        });
    }
    Ok(ParsedCatalog { items, argon_explicitly_disabled })
}

#[cfg(test)]
pub fn parse_quota(body: &str) -> Result<Vec<ModelQuota>, String> {
    let catalog = parse_models_catalog(body, false)?;
    let mut quotas: Vec<ModelQuota> = catalog
        .items
        .into_iter()
        .filter_map(|item| {
            let (remaining, reset) = item.quota?;
            Some(ModelQuota {
                name: item.name,
                remaining,
                reset,
                weekly_remaining: None,
                weekly_reset: None,
                is_weekly_only: false,
            })
        })
        .collect();
    quotas.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(quotas)
}

fn is_ignored_model(name: &str) -> bool {
    let m = name.trim().to_lowercase();
    m.contains("-disabled")
        || m.ends_with("_disabled")
        || m.starts_with("chat_")
        || m.starts_with("chat ")
        || m.starts_with("tab_")
        || m.starts_with("tab ")
        || m.contains("internal")
        || m.contains("rev19")
        || m.contains("-exp")
        || m.ends_with("-agent")
        || m.ends_with(" agent")
}

/// Upstream Antigravity currently maintains three distinct quota pools:
/// `Gemini`, `Gemini Argon`, and `Claude / GPT` (with 4.6 / 5.5 versions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuotaBucket {
    Gemini,
    Argon,
    ClaudeGpt,
}

impl QuotaBucket {
    fn default_label(self) -> &'static str {
        match self {
            Self::Gemini => "Gemini",
            Self::Argon => "Gemini Argon",
            Self::ClaudeGpt => "Claude / GPT",
        }
    }
}

fn classify_bucket(name: &str) -> Option<QuotaBucket> {
    let lower = name.trim().to_lowercase();
    if lower.contains("argon") {
        Some(QuotaBucket::Argon)
    } else if lower.contains("gemini") || lower.contains("imagen") {
        Some(QuotaBucket::Gemini)
    } else if lower.contains("claude")
        || lower.contains("opus")
        || lower.contains("sonnet")
        || lower.contains("haiku")
        || lower.contains("gpt")
    {
        Some(QuotaBucket::ClaudeGpt)
    } else {
        None
    }
}

fn is_legacy_quota_model(name: &str, bucket: QuotaBucket) -> bool {
    let lower = name.trim().to_lowercase();
    if lower.contains("1.5")
        || lower.contains("1-5")
        || lower.contains("2.0")
        || lower.contains("2-0")
        || lower.contains("2.5")
        || lower.contains("2-5")
    {
        return true;
    }
    match bucket {
        QuotaBucket::Gemini => {
            lower.contains("preview")
                || lower == "gemini-3-pro"
                || lower == "gemini 3 pro"
                || lower.contains("3.6")
                || lower.contains("3-6")
                || lower.contains("3.7")
                || lower.contains("3-7")
                || lower.contains("3.5-flash-low")
                || lower.contains("3.5 flash low")
                || lower.contains("3.5-flash-extra-low")
                || lower.contains("3.5 flash extra low")
                || lower.contains("3-5-flash-low")
                || lower == "gemini-3-flash"
                || lower == "gemini 3 flash"
        }
        QuotaBucket::ClaudeGpt => {
            let old_claude = ["3.5", "3-5", "3.7", "3-7", "4.0", "4-0", "4.1", "4-1", "4.5", "4-5"];
            let bad_thinking = (lower.contains("sonnet") || lower.contains("5.5") || lower.contains("5-5"))
                && lower.contains("thinking");
            bad_thinking || old_claude.iter().any(|ver| lower.contains(ver))
        }
        QuotaBucket::Argon => false,
    }
}

fn is_pre_gemini3(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    ["1.5", "1-5", "2.0", "2-0", "2.5", "2-5"]
        .iter()
        .any(|ver| lower.contains(ver))
}

fn extract_claude_version(text: &str) -> Option<(u32, u32)> {
    let lower = text.trim().to_lowercase();
    if !(lower.contains("claude")
        || lower.contains("opus")
        || lower.contains("sonnet")
        || lower.contains("haiku")
        || lower.contains("fable")
        || lower.contains("mythos"))
    {
        return None;
    }
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let maj_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < bytes.len() && (bytes[i] == b'.' || bytes[i] == b'-') && bytes[i + 1].is_ascii_digit() {
                let min_start = i + 1;
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if let (Ok(maj), Ok(min)) = (
                    lower[maj_start..min_start - 1].parse::<u32>(),
                    lower[min_start..i].parse::<u32>(),
                ) && maj >= 4
                    && min < 20
                {
                    return Some((maj, min));
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

fn claude_bucket_label<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut versions: Vec<(u32, u32)> = Vec::new();
    let mut has_gpt = false;
    for name in names {
        if name.to_lowercase().contains("gpt") {
            has_gpt = true;
        }
        if let Some(ver) = extract_claude_version(name)
            && !versions.contains(&ver)
        {
            versions.push(ver);
        }
    }
    versions.sort_by(|a, b| b.cmp(a));
    if versions.is_empty() {
        return QuotaBucket::ClaudeGpt.default_label().to_owned();
    }
    let joined = versions
        .iter()
        .map(|(maj, min)| format!("{maj}.{min}"))
        .collect::<Vec<_>>()
        .join("&");
    if has_gpt {
        format!("Claude {joined} / GPT")
    } else {
        format!("Claude {joined}")
    }
}

fn aggregate_bucket(label: &str, items: &[&ModelQuota]) -> Option<ModelQuota> {
    let first = *items.first()?;
    let mut chosen = first;
    let mut weekly_rem: Option<f32> = None;
    let mut weekly_reset: Option<String> = None;
    let mut weekly_only = first.is_weekly_only;

    for &item in items {
        let lower_rem = item.remaining < chosen.remaining - f32::EPSILON;
        let same_rem_with_reset = (item.remaining - chosen.remaining).abs() <= f32::EPSILON
            && chosen.reset.is_none()
            && item.reset.is_some();
        if lower_rem || same_rem_with_reset {
            chosen = item;
        }
        if !item.is_weekly_only {
            weekly_only = false;
        }
        if let Some(w_rem) = item.weekly_remaining {
            let replace_w = match weekly_rem {
                None => true,
                Some(prev) => {
                    w_rem < prev - f32::EPSILON
                        || ((w_rem - prev).abs() <= f32::EPSILON
                            && weekly_reset.is_none()
                            && item.weekly_reset.is_some())
                }
            };
            if replace_w {
                weekly_rem = Some(w_rem);
                weekly_reset = item.weekly_reset.clone();
            }
        }
    }
    Some(ModelQuota {
        name: label.to_owned(),
        remaining: chosen.remaining,
        reset: chosen.reset.clone(),
        weekly_remaining: weekly_rem,
        weekly_reset,
        is_weekly_only: weekly_only,
    })
}

/// Collapses raw per-variant quotas into the three Antigravity quota pools
/// (`Gemini`, `Gemini Argon`, and `Claude [4.6/5.5] / GPT`).
pub fn summarize_quotas(models: &[ModelQuota]) -> Vec<ModelQuota> {
    let mut primary: [Vec<&ModelQuota>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut fallback: [Vec<&ModelQuota>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut others: Vec<ModelQuota> = Vec::new();

    for model in models {
        if is_ignored_model(&model.name) {
            continue;
        }
        let Some(bucket) = classify_bucket(&model.name) else {
            others.push(model.clone());
            continue;
        };
        let slot = bucket as usize;
        if is_legacy_quota_model(&model.name, bucket) {
            if !is_pre_gemini3(&model.name) {
                fallback[slot].push(model);
            }
        } else {
            primary[slot].push(model);
        }
    }

    let mut summary = Vec::new();
    for bucket in [QuotaBucket::Gemini, QuotaBucket::ClaudeGpt, QuotaBucket::Argon] {
        let slot = bucket as usize;
        let candidates = if !primary[slot].is_empty() {
            &primary[slot]
        } else {
            &fallback[slot]
        };
        let label = if bucket == QuotaBucket::ClaudeGpt {
            claude_bucket_label(candidates.iter().map(|q| q.name.as_str()))
        } else {
            bucket.default_label().to_owned()
        };
        if let Some(mut row) = aggregate_bucket(&label, candidates) {
            if bucket == QuotaBucket::Argon {
                // Argon has weekly quota only (no 5H rolling window).
                let w_rem = row.weekly_remaining.unwrap_or(row.remaining);
                row.remaining = w_rem;
                row.weekly_remaining = Some(w_rem);
                if row.weekly_reset.is_none() {
                    row.weekly_reset = row.reset.clone();
                }
                row.reset = row.weekly_reset.clone();
                row.is_weekly_only = true;
            }
            summary.push(row);
        }
    }
    others.sort_by(|a, b| a.name.cmp(&b.name));
    summary.extend(others);
    summary
}

fn parse_tokens_body(body: &str) -> Result<Tokens, String> {
    let root: Value = serde_json::from_str(body).map_err(|e| format!("响应不是 JSON：{e}"))?;
    if let Some(error) = root.get("error").and_then(Value::as_str) {
        return Err(format!("Google 返回 {error}"));
    }
    let access_token = root
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "响应里没有 access_token".to_owned())?
        .to_owned();
    let expires_in = root.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
    Ok(Tokens {
        access_token,
        refresh_token: root
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_owned),
        expires_at: (nsh_core::time::unix_ms() as i64) + expires_in * 1000,
    })
}

/// The account's e-mail from the userinfo payload.
pub fn parse_email(body: &str) -> Result<String, String> {
    let root: Value = serde_json::from_str(body).map_err(|e| format!("响应不是 JSON：{e}"))?;
    root.get("email")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "响应里没有 email".to_owned())
}

/// Exchanges an authorization code for tokens.
pub async fn exchange_code(
    code: &str,
    redirect_uri: &str,
    proxy: ProxyConfig,
) -> Result<Tokens, String> {
    let body = [
        ("client_id", OAUTH_CLIENT_ID),
        ("client_secret", OAUTH_CLIENT_SECRET),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ]
    .iter()
    .map(|(key, value)| format!("{key}={}", percent_encode(value)))
    .collect::<Vec<_>>()
    .join("&");
    let (status, text) = send(
        "POST",
        TOKEN_URL.to_owned(),
        FORM_HEADERS,
        Some(body),
        proxy,
    )
    .await?;
    if !(200..300).contains(&status) {
        return Err(format!("换取令牌失败：HTTP {status} {text}"));
    }
    let tokens = parse_tokens_body(&text)?;
    if tokens.refresh_token.is_none() {
        return Err("Google 没有返回 refresh_token，请重新授权".to_owned());
    }
    Ok(tokens)
}

/// Renews an access token from the account's refresh token.
pub async fn refresh(
    refresh_token: &str,
    proxy: ProxyConfig,
) -> Result<Tokens, String> {
    let body = [
        ("client_id", OAUTH_CLIENT_ID),
        ("client_secret", OAUTH_CLIENT_SECRET),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ]
    .iter()
    .map(|(key, value)| format!("{key}={}", percent_encode(value)))
    .collect::<Vec<_>>()
    .join("&");
    let (status, text) = send("POST", TOKEN_URL.to_owned(), FORM_HEADERS, Some(body), proxy).await?;
    if !(200..300).contains(&status) {
        return Err(format!("续期失败：HTTP {status} {text}"));
    }
    parse_tokens_body(&text)
}

/// Renews a refresh token, retrying once after a short backoff on HTTP 429.
pub async fn refresh_with_rate_retry(
    refresh_token: &str,
    proxy: ProxyConfig,
) -> Result<Tokens, String> {
    match refresh(refresh_token, proxy.clone()).await {
        Err(error) if error.contains("HTTP 429") || error.contains("RESOURCE_EXHAUSTED") => {
            timer(Duration::from_millis(1500)).await;
            refresh(refresh_token, proxy).await
        }
        other => other,
    }
}

/// The account's e-mail address.
pub async fn email(access_token: &str, proxy: ProxyConfig) -> Result<String, String> {
    let authorization = format!("Bearer {access_token}");
    let headers = [("Authorization", authorization.as_str()), ("User-Agent", USER_AGENT)];
    let (status, text) = send("GET", USERINFO_URL.to_owned(), &headers, None, proxy).await?;
    if !(200..300).contains(&status) {
        return Err(format!("读取账号信息失败：HTTP {status}"));
    }
    parse_email(&text)
}

/// Normalizes raw `loadCodeAssist` tier strings into [`AccountTier`].
pub fn normalize_tier(raw: &str) -> AccountTier {
    let lower = raw.trim().to_lowercase();
    if lower.contains("ultra") || lower.contains("helium") {
        AccountTier::Ultra
    } else if lower.contains("pro") || lower.contains("premium") || lower.contains("advanced") {
        AccountTier::Pro
    } else {
        AccountTier::Free
    }
}

/// Parses `/v1internal:loadCodeAssist` for project ID and subscription tier.
pub fn parse_code_assist(body: &str) -> (Option<String>, Option<AccountTier>) {
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return (None, None);
    };
    let project_id = root
        .get("cloudaicompanionProject")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    fn tier_str(value: &Value) -> Option<&str> {
        value
            .get("id")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .or_else(|| {
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
            })
    }
    let raw_tier = root
        .get("paidTier")
        .and_then(tier_str)
        .or_else(|| root.get("currentTier").and_then(tier_str))
        .or_else(|| {
            root.get("allowedTiers")
                .and_then(Value::as_array)
                .and_then(|tiers| {
                    tiers
                        .iter()
                        .find(|tier| {
                            tier.get("isDefault").and_then(Value::as_bool) == Some(true)
                                || tier.get("id").and_then(Value::as_str) == Some("free-tier")
                        })
                        .and_then(tier_str)
                })
        })
        .unwrap_or("free-tier");
    (project_id, Some(normalize_tier(raw_tier)))
}

/// Parses `/v1internal:retrieveUserQuotaSummary` grouped 5h/weekly quota buckets.
pub fn parse_quota_summary(body: &str) -> Vec<SummaryGroup> {
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(groups) = root.get("groups").and_then(Value::as_array) else {
        return Vec::new();
    };
    groups
        .iter()
        .filter_map(|group| {
            let display_name = group
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let description = group
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let buckets = group
                .get("buckets")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(|bucket| {
                    let reset_time = bucket
                        .get("resetTime")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned);
                    // Proto3 JSON omits 0.0 floats when exhausted; treat absent fraction with a
                    // reset timestamp as 0.0 so exhausted buckets never vanish.
                    let fraction = bucket
                        .get("remainingFraction")
                        .and_then(Value::as_f64)
                        .or_else(|| reset_time.as_ref().map(|_| 0.0))? as f32;
                    Some(SummaryBucket {
                        bucket_id: bucket
                            .get("bucketId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        window: bucket
                            .get("window")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        display_name: bucket
                            .get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        description: bucket
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        remaining_fraction: fraction.clamp(0.0, 1.0),
                        reset_time,
                    })
                })
                .collect::<Vec<_>>();
            (!buckets.is_empty()).then_some(SummaryGroup {
                display_name,
                description,
                buckets,
            })
        })
        .collect()
}

fn is_argon_group(group: &SummaryGroup) -> bool {
    group.display_name.to_lowercase().contains("argon")
        || group.description.to_lowercase().contains("argon")
        || group.buckets.iter().any(|bucket| {
            bucket.bucket_id.to_lowercase().contains("argon")
                || bucket.display_name.to_lowercase().contains("argon")
                || bucket.description.to_lowercase().contains("argon")
        })
}

fn is_third_party_group(group: &SummaryGroup) -> bool {
    let name = group.display_name.to_lowercase();
    name.contains("claude")
        || name.contains("gpt")
        || name.contains("3p")
        || group
            .buckets
            .iter()
            .any(|bucket| bucket.bucket_id.to_lowercase().contains("3p"))
}

struct GroupWindows {
    five_hour: Option<(f32, Option<String>)>,
    weekly: Option<(f32, Option<String>)>,
}

fn pick_min_window(items: impl Iterator<Item = (f32, Option<String>)>) -> Option<(f32, Option<String>)> {
    let mut best: Option<(f32, Option<String>)> = None;
    for (rem, reset) in items {
        best = match best {
            None => Some((rem, reset)),
            Some((b_rem, b_reset)) => {
                if rem < b_rem - f32::EPSILON
                    || ((rem - b_rem).abs() <= f32::EPSILON && b_reset.is_none() && reset.is_some())
                {
                    Some((rem, reset))
                } else {
                    Some((b_rem, b_reset))
                }
            }
        };
    }
    best
}

fn pick_group_windows(
    groups: &[SummaryGroup],
    target: QuotaBucket,
    allow_ultra_argon_fallback: bool,
) -> Option<GroupWindows> {
    let matched = match target {
        QuotaBucket::Argon => groups.iter().find(|group| is_argon_group(group)).or_else(|| {
            allow_ultra_argon_fallback.then(|| {
                groups
                    .iter()
                    .find(|group| !is_argon_group(group) && !is_third_party_group(group))
            })?
        }),
        QuotaBucket::Gemini => groups
            .iter()
            .find(|group| !is_argon_group(group) && !is_third_party_group(group)),
        QuotaBucket::ClaudeGpt => groups.iter().find(|group| is_third_party_group(group)),
    }?;

    let is_5h = |b: &SummaryBucket| {
        let win = b.window.to_lowercase();
        let bid = b.bucket_id.to_lowercase();
        win.contains("5h") || bid.contains("5h") || win.contains("hour") || bid.contains("hour")
    };
    let is_weekly = |b: &SummaryBucket| {
        let win = b.window.to_lowercase();
        let bid = b.bucket_id.to_lowercase();
        win.contains("week") || bid.contains("week") || win.contains("7d") || bid.contains("7d")
    };

    let five_hour = pick_min_window(
        matched
            .buckets
            .iter()
            .filter(|b| is_5h(b))
            .map(|b| (b.remaining_fraction, b.reset_time.clone())),
    );
    let mut weekly = pick_min_window(
        matched
            .buckets
            .iter()
            .filter(|b| is_weekly(b))
            .map(|b| (b.remaining_fraction, b.reset_time.clone())),
    );
    let mut five_hour = five_hour;
    if five_hour.is_none() && weekly.is_none()
        && let Some(first) = matched.buckets.first()
    {
        let fallback_val = (first.remaining_fraction, first.reset_time.clone());
        if target == QuotaBucket::Argon {
            weekly = Some(fallback_val);
        } else {
            five_hour = Some(fallback_val);
        }
    }
    Some(GroupWindows { five_hour, weekly })
}

fn combine_quota(
    model: Option<(f32, Option<String>)>,
    group: Option<(f32, Option<String>)>,
) -> Option<(f32, Option<String>)> {
    match (model, group) {
        (Some((m_rem, m_reset)), Some((g_rem, g_reset))) => {
            if m_rem <= 0.0 {
                Some((0.0, m_reset.or(g_reset)))
            } else if g_rem < m_rem {
                Some((g_rem, g_reset.or(m_reset)))
            } else {
                Some((m_rem, m_reset.or(g_reset)))
            }
        }
        (Some(m), None) => Some(m),
        (None, Some(g)) => Some(g),
        (None, None) => None,
    }
}

fn collect_official_supported_ids(root: &Value) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    if let Some(sorts) = root.get("agentModelSorts").and_then(Value::as_array) {
        for sort in sorts {
            if let Some(groups) = sort.get("groups").and_then(Value::as_array) {
                for group in groups {
                    if let Some(model_ids) = group.get("modelIds").and_then(Value::as_array) {
                        for id in model_ids.iter().filter_map(Value::as_str) {
                            ids.insert(id.trim().to_lowercase());
                        }
                    }
                }
            }
        }
    }
    if let Some(images) = root.get("imageGenerationModelIds").and_then(Value::as_array) {
        for id in images.iter().filter_map(Value::as_str) {
            ids.insert(id.trim().to_lowercase());
        }
    }
    if let Some(tiered) = root.get("tieredModelIds").and_then(Value::as_object) {
        for values in tiered.values().filter_map(Value::as_array) {
            for id in values.iter().filter_map(Value::as_str) {
                ids.insert(id.trim().to_lowercase());
            }
        }
    }
    ids
}

/// Resolves subscription tier and the three quota pools (`Gemini`,
/// `Gemini Argon`, `Claude [4.6/5.5] / GPT`) from `fetchAvailableModels`,
/// `retrieveUserQuotaSummary`, and `loadCodeAssist`.
pub fn build_account_quotas(
    models_body: &str,
    groups: Option<&[SummaryGroup]>,
    detected_tier: Option<AccountTier>,
) -> Result<AccountQuotaSnapshot, String> {
    let catalog = parse_models_catalog(models_body, true)?;

    let effective_tier = detected_tier;

    let mut primary: [Vec<ModelQuota>; 3] = Default::default();
    let mut fallback: [Vec<ModelQuota>; 3] = Default::default();
    let mut claude_primary_names: Vec<String> = Vec::new();
    let mut claude_fallback_names: Vec<String> = Vec::new();
    let mut others: Vec<ModelQuota> = Vec::new();
    let mut has_supported_claude = false;

    for item in catalog.items {
        let bucket = classify_bucket(&item.name).or_else(|| classify_bucket(&item.id));
        let Some(bucket) = bucket else {
            if let Some((remaining, reset)) = item.quota {
                others.push(ModelQuota {
                    name: item.name,
                    remaining,
                    reset,
                    weekly_remaining: None,
                    weekly_reset: None,
                    is_weekly_only: false,
                });
            }
            continue;
        };

        let is_legacy =
            is_legacy_quota_model(&item.name, bucket) || is_legacy_quota_model(&item.id, bucket);
        if bucket == QuotaBucket::ClaudeGpt {
            if !is_legacy {
                has_supported_claude = true;
                claude_primary_names.push(item.name.clone());
                claude_primary_names.push(item.id.clone());
            } else {
                claude_fallback_names.push(item.name.clone());
                claude_fallback_names.push(item.id.clone());
            }
        }

        if let Some((remaining, reset)) = item.quota {
            let entry = ModelQuota {
                name: item.name.clone(),
                remaining,
                reset,
                weekly_remaining: None,
                weekly_reset: None,
                is_weekly_only: false,
            };
            let slot = bucket as usize;
            if is_legacy {
                if !is_pre_gemini3(&item.name) {
                    fallback[slot].push(entry);
                }
            } else {
                primary[slot].push(entry);
            }
        }
    }

    let allow_ultra = matches!(effective_tier, Some(AccountTier::Ultra));
    let is_free = effective_tier == Some(AccountTier::Free);
    let has_argon = !catalog.argon_explicitly_disabled
        && (allow_ultra
            || match groups {
                Some(g) => g.iter().any(is_argon_group),
                None => !primary[QuotaBucket::Argon as usize].is_empty(),
            });

    let claude_names = if !claude_primary_names.is_empty() {
        &claude_primary_names
    } else {
        &claude_fallback_names
    };
    let claude_label = claude_bucket_label(claude_names.iter().map(String::as_str));

    let mut out = Vec::new();
    for bucket in [QuotaBucket::Gemini, QuotaBucket::ClaudeGpt, QuotaBucket::Argon] {
        if bucket == QuotaBucket::Argon && !has_argon {
            continue;
        }
        let slot = bucket as usize;
        let label = if bucket == QuotaBucket::ClaudeGpt {
            claude_label.as_str()
        } else {
            bucket.default_label()
        };
        let model_refs: Vec<&ModelQuota> = if !primary[slot].is_empty() {
            primary[slot].iter().collect()
        } else {
            fallback[slot].iter().collect()
        };
        let model_val = aggregate_bucket(label, &model_refs).map(|q| (q.remaining, q.reset));

        if bucket == QuotaBucket::ClaudeGpt && !has_supported_claude && model_val.is_none() {
            continue;
        }

        let group_win = groups.and_then(|g| pick_group_windows(g, bucket, allow_ultra));
        let (group_5h, group_weekly) = match group_win {
            Some(w) => (w.five_hour, w.weekly),
            None => (None, None),
        };

        let resolved = if is_free && bucket != QuotaBucket::Argon {
            // Free accounts use weekly quotas only (no 5H rolling window).
            let weekly = combine_quota(model_val, group_weekly.or(group_5h));
            weekly.map(|(w_rem, w_reset)| ModelQuota {
                name: label.to_owned(),
                remaining: w_rem,
                reset: w_reset.clone(),
                weekly_remaining: Some(w_rem),
                weekly_reset: w_reset,
                is_weekly_only: true,
            })
        } else if bucket == QuotaBucket::Argon {
            // Argon has a weekly quota window only (no 5H window).
            let weekly = combine_quota(model_val, group_weekly.or(group_5h)).or_else(|| {
                allow_ultra.then(|| {
                    groups
                        .and_then(|g| pick_group_windows(g, QuotaBucket::Gemini, false))
                        .and_then(|w| w.weekly.or(w.five_hour))
                        .unwrap_or((1.0, None))
                })
            });
            weekly.map(|(w_rem, w_reset)| ModelQuota {
                name: label.to_owned(),
                remaining: w_rem,
                reset: w_reset.clone(),
                weekly_remaining: Some(w_rem),
                weekly_reset: w_reset,
                is_weekly_only: true,
            })
        } else {
            let five_h = combine_quota(model_val, group_5h);
            match (five_h, group_weekly) {
                (Some((h_rem, h_reset)), w) => Some(ModelQuota {
                    name: label.to_owned(),
                    remaining: h_rem,
                    reset: h_reset,
                    weekly_remaining: w.as_ref().map(|(r, _)| *r),
                    weekly_reset: w.and_then(|(_, rs)| rs),
                    is_weekly_only: false,
                }),
                (None, Some((w_rem, w_reset))) => Some(ModelQuota {
                    name: label.to_owned(),
                    remaining: w_rem,
                    reset: w_reset.clone(),
                    weekly_remaining: Some(w_rem),
                    weekly_reset: w_reset,
                    is_weekly_only: true,
                }),
                (None, None) => None,
            }
        };

        if let Some(row) = resolved {
            out.push(row);
        }
    }

    others.sort_by(|a, b| a.name.cmp(&b.name));
    out.extend(others);

    Ok(AccountQuotaSnapshot { tier: effective_tier, models: out })
}

/// Calls a Cloud Code internal endpoint synchronously in `QUOTA_HOSTS` order.
fn post_cloudcode_sync(
    path: &str,
    access_token: &str,
    body: &str,
    proxy: &ProxyConfig,
) -> Result<String, String> {
    let headers = [
        ("Authorization".to_owned(), format!("Bearer {access_token}")),
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("User-Agent".to_owned(), HUB_USER_AGENT.to_owned()),
    ];
    let mut last_error = String::new();
    for host in QUOTA_HOSTS {
        let url = format!("{host}{path}");
        let (status, text) = match request("POST", &url, &headers, Some(body), proxy) {
            Ok(response) => response,
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        if !(200..300).contains(&status) {
            last_error = format!("HTTP {status}");
            continue;
        }
        return Ok(text);
    }
    Err(if last_error.is_empty() {
        "额度接口全部失败".to_owned()
    } else {
        last_error
    })
}

/// Fetches the account's tier (`loadCodeAssist`), grouped summary
/// (`retrieveUserQuotaSummary`), and model quotas (`fetchAvailableModels`).
pub async fn quota(
    access_token: &str,
    proxy: ProxyConfig,
) -> Result<AccountQuotaSnapshot, String> {
    let token = access_token.to_owned();
    blocking(move || {
        let (project_id, detected_tier) = match post_cloudcode_sync(
            "/v1internal:loadCodeAssist",
            &token,
            r#"{"metadata":{"ideType":"ANTIGRAVITY"}}"#,
            &proxy,
        ) {
            Ok(text) => parse_code_assist(&text),
            Err(_) => (None, None),
        };

        let summary_payload = match project_id {
            Some(ref pid) => serde_json::json!({ "project": pid }).to_string(),
            None => "{}".to_owned(),
        };

        // Run grouped summary and available-models queries concurrently once project_id is known.
        let (summary_res, models_res) = std::thread::scope(|scope| {
            let summary_handle = scope.spawn(|| {
                post_cloudcode_sync(
                    "/v1internal:retrieveUserQuotaSummary",
                    &token,
                    &summary_payload,
                    &proxy,
                )
            });
            let models_res = post_cloudcode_sync(
                "/v1internal:fetchAvailableModels",
                &token,
                &summary_payload,
                &proxy,
            );
            let summary_res = summary_handle
                .join()
                .map_err(|_| "retrieveUserQuotaSummary worker panicked".to_owned())
                .and_then(|res| res);
            (summary_res, models_res)
        });

        let summary_groups = match summary_res {
            Ok(text) => Some(parse_quota_summary(&text)),
            Err(error) => {
                log::warn!("retrieveUserQuotaSummary failed: {error}");
                None
            }
        };
        let models_body = models_res?;

        build_account_quotas(&models_body, summary_groups.as_deref(), detected_tier)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The form headers every token request sends.
const FORM_HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/x-www-form-urlencoded"),
    ("User-Agent", USER_AGENT),
];

/// Performs an HTTP GET request through the configured proxy (or directly when disabled).
pub(crate) async fn http_get(
    url: &str,
    headers: &[(&str, &str)],
    proxy: ProxyConfig,
) -> Result<(u16, String), String> {
    send("GET", url.to_owned(), headers, None, proxy).await
}

/// One request through the proxy the user configured, blocking on the
/// request: NSH's own `fetch` follows the system proxy, which is exactly
/// what a user behind a proxy without the system setting does not have.
async fn send(
    method: &str,
    url: String,
    headers: &[(&str, &str)],
    body: Option<String>,
    proxy: ProxyConfig,
) -> Result<(u16, String), String> {
    let headers: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    let method = method.to_owned();
    blocking(move || request(&method, &url, &headers, body.as_deref(), &proxy))
        .await
        .map_err(|error| error.to_string())?
}

const PROXY_PROBE_URL: &str = "https://www.gstatic.com/generate_204";

/// Tests whether `proxy` can establish an HTTPS tunnel to Google and returns round-trip latency in ms.
pub async fn test_proxy(proxy: ProxyConfig) -> Result<u64, String> {
    proxy.validate_endpoint()?;
    blocking(move || probe_proxy_url(PROXY_PROBE_URL, &proxy))
        .await
        .map_err(|error| error.to_string())?
}

fn cached_agent(proxy_url: Option<&str>, timeout_secs: u64) -> Result<ureq::Agent, String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    type AgentCache = Mutex<HashMap<(Option<String>, u64), ureq::Agent>>;
    static CACHE: OnceLock<AgentCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (proxy_url.map(str::to_owned), timeout_secs);
    if let Ok(guard) = cache.lock()
        && let Some(agent) = guard.get(&key)
    {
        return Ok(agent.clone());
    }
    let proxy_opt = match proxy_url {
        Some(url) => Some(ureq::Proxy::new(url).map_err(|e| format!("代理地址无效：{e}"))?),
        None => None,
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(timeout_secs)))
        .http_status_as_error(false)
        .proxy(proxy_opt)
        .build()
        .into();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, agent.clone());
    }
    Ok(agent)
}

fn probe_proxy_url(url: &str, proxy: &ProxyConfig) -> Result<u64, String> {
    proxy.validate_endpoint()?;
    let started = std::time::Instant::now();
    let proxy_url = proxy.url();
    let agent = cached_agent(Some(&proxy_url), 8)?;
    match agent.get(url).header("User-Agent", USER_AGENT).call() {
        Ok(response) => {
            let status = response.status().as_u16();
            if status < 400 {
                Ok(started.elapsed().as_millis() as u64)
            } else {
                Err(format!("代理返回 HTTP {status}"))
            }
        }
        Err(error) => Err(format!("经代理 {proxy_url} 连接失败：{error}")),
    }
}

fn send_with_headers<B>(
    mut req: ureq::RequestBuilder<B>,
    headers: &[(String, String)],
    finish: impl FnOnce(ureq::RequestBuilder<B>) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    for (name, value) in headers {
        req = req.header(name.as_str(), value.as_str());
    }
    finish(req)
}

/// The blocking request itself, on a blocking thread.
fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    proxy: &ProxyConfig,
) -> Result<(u16, String), String> {
    proxy.validate()?;
    let proxy_url = proxy.enabled.then(|| proxy.url());
    let agent = cached_agent(proxy_url.as_deref(), 20)?;
    let response = match (method, body) {
        ("POST", Some(body)) => send_with_headers(agent.post(url), headers, |r| r.send(body)),
        ("POST", None) => send_with_headers(agent.post(url), headers, |r| r.send_empty()),
        ("PUT", Some(body)) => send_with_headers(agent.put(url), headers, |r| r.send(body)),
        ("PATCH", Some(body)) => send_with_headers(agent.patch(url), headers, |r| r.send(body)),
        ("GET", None) => send_with_headers(agent.get(url), headers, |r| r.call()),
        ("HEAD", None) => send_with_headers(agent.head(url), headers, |r| r.call()),
        ("DELETE", None) => send_with_headers(agent.delete(url), headers, |r| r.call()),
        (other, _) => return Err(format!("不支持的 HTTP 方法：{other}")),
    };
    match response {
        Ok(mut response) => {
            let status = response.status().as_u16();
            response
                .body_mut()
                .read_to_string()
                .map(|text| (status, text))
                .map_err(|e| format!("读取响应失败：{e}"))
        }
        Err(error) if proxy.enabled => {
            Err(format!("经代理 {proxy} 请求失败：{error}", proxy = proxy.url()))
        }
        Err(error) => Err(format!("直接请求失败：{error}")),
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' if at + 3 <= bytes.len() => {
                let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        at += 3;
                    }
                    Err(_) => {
                        out.push(bytes[at]);
                        at += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_authorization_url() {
        let url = auth_url("http://127.0.0.1:8080/oauth-callback", "abc");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8080%2Foauth-callback"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("state=abc"));
    }

    #[test]
    fn reads_the_callback() {
        let (code, state) =
            callback_query("GET /oauth-callback?state=xyz&code=4%2F0Abc HTTP/1.1").expect("a callback");
        assert_eq!(code.as_deref(), Some("4/0Abc"));
        assert_eq!(state.as_deref(), Some("xyz"));
        assert_eq!(callback_query("GET /favicon.ico HTTP/1.1"), None);
    }

    #[test]
    fn reads_tokens_from_a_json_array_or_plain_text() {
        let json = r#"[{"refresh_token":"1//0gAAA"},{"refresh_token":"1//0gBBB"},{"refresh_token":"1//0gAAA"}]"#;
        assert_eq!(parse_tokens(json), vec!["1//0gAAA", "1//0gBBB"]);
        let text = "first 1//0gCCCCCCCCCCCCCCCCCCCCCCC then 1//0gDDDDDDDDDDDDDDDDDDDDDDD";
        assert_eq!(
            parse_tokens(text),
            vec!["1//0gCCCCCCCCCCCCCCCCCCCCCCC", "1//0gDDDDDDDDDDDDDDDDDDDDDDD"]
        );
        assert!(parse_tokens("nothing here").is_empty());
    }

    #[test]
    fn reads_quotas_and_drops_models_without_one() {
        let body = r#"{
            "models": {
                "MODEL_PLACEHOLDER_M35": {
                    "displayName": "Claude Sonnet",
                    "quotaInfo": {"remainingFraction": 0.78, "resetTime": "2026-11-14T14:00:00Z"}
                },
                "MODEL_PLACEHOLDER_M318": {"displayName": "Gemini Pro"},
                "no_name": {"quotaInfo": {"remainingFraction": 0.5}}
            }
        }"#;
        let quotas = parse_quota(body).expect("quotas");
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].name, "Claude Sonnet");
        assert_eq!(quotas[0].remaining, 0.78);
        assert_eq!(quotas[0].reset.as_deref(), Some("2026-11-14T14:00:00Z"));
        assert_eq!(quotas[1].name, "no_name");
        assert_eq!(quotas[1].reset, None);
    }

    #[test]
    fn reads_a_token_response_and_an_error() {
        let tokens = parse_tokens_body(
            r#"{"access_token":"ya29.x","expires_in":3600,"refresh_token":"1//0gA"}"#,
        )
        .expect("tokens");
        assert_eq!(tokens.access_token, "ya29.x");
        assert_eq!(tokens.refresh_token.as_deref(), Some("1//0gA"));
        assert!(parse_tokens_body(r#"{"error":"invalid_grant"}"#).is_err());
        assert!(parse_tokens_body("not json").is_err());
    }

    #[test]
    fn reads_an_email() {
        assert_eq!(parse_email(r#"{"email":"a@b.c"}"#), Ok("a@b.c".to_owned()));
        assert!(parse_email("{}").is_err());
    }

    #[test]
    fn parses_all_antigravity_manager_import_shapes_and_exports_json() {
        let array = r#"[{"email":"u1@gmail.com","refresh_token":"1//token_one"},{"name":"u2@gmail.com","token":{"refresh_token":"1//token_two"}},{"email":"dup","refresh_token":"1//token_one"},{"email":"bad","refresh_token":"not_google"}]"#;
        let parsed = parse_import_entries(array).expect("array import");
        assert_eq!(
            parsed,
            vec![
                ImportEntry { email: Some("u1@gmail.com".to_owned()), refresh_token: "1//token_one".to_owned() },
                ImportEntry { email: Some("u2@gmail.com".to_owned()), refresh_token: "1//token_two".to_owned() },
            ]
        );

        let wrapped_array = r#"{"accounts":[{"email":"a@b.c","refresh_token":"1//token_a"}]}"#;
        assert_eq!(parse_import_entries(wrapped_array).expect("wrapped").len(), 1);

        let wrapped_map = r#"{"accounts":{"id1":{"email":"b@c.d","refresh_token":"1//token_b"}}}"#;
        assert_eq!(parse_import_entries(wrapped_map).expect("map").len(), 1);

        let top_map = r#"{"k1":{"email":"c@d.e","refresh_token":"1//token_c"}}"#;
        assert_eq!(parse_import_entries(top_map).expect("top map").len(), 1);

        let single_object = "\u{feff}{\"email\":\"d@e.f\",\"refresh_token\":\"1//token_d\"}";
        assert_eq!(parse_import_entries(single_object).expect("single").len(), 1);

        assert!(parse_import_entries("[]").is_err());
        assert!(parse_import_entries("{bad").is_err());

        let exported = format_export_json(&[ExportAccount {
            email: "u1@gmail.com".to_owned(),
            refresh_token: "1//token_one".to_owned(),
        }])
        .expect("export json");
        let reparsed = parse_import_entries(&exported).expect("round trip");
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].email.as_deref(), Some("u1@gmail.com"));
        assert_eq!(reparsed[0].refresh_token, "1//token_one");
    }

    #[test]
    fn summarizes_many_model_variants_into_core_quotas() {
        let body = r#"{
            "deprecatedModelIds": {
                "gemini-2.0-flash": {"newModelId": "gemini-3.8-flash"}
            },
            "models": {
                "chat_20706": {"displayName": "Chat", "quotaInfo": {"remainingFraction": 1.0}},
                "tab_flash_lite_preview": {"displayName": "Tab", "quotaInfo": {"remainingFraction": 1.0}},
                "rev19-uic3-1p": {"displayName": "Internal", "quotaInfo": {"remainingFraction": 1.0}},
                "gemini-2.0-flash": {"displayName": "Gemini 2.0 Flash", "quotaInfo": {"remainingFraction": 1.0}},
                "gemini-2.5-pro": {"displayName": "Gemini 2.5 Pro", "quotaInfo": {"remainingFraction": 1.0}},
                "gemini-2.5-flash": {"displayName": "Gemini 2.5 Flash", "quotaInfo": {"remainingFraction": 1.0}},
                "gemini-3.1-pro-high": {"displayName": "Gemini 3.1 Pro (High)", "quotaInfo": {"remainingFraction": 0.8, "resetTime": "2026-11-18T14:00:00Z"}},
                "gemini-3.1-pro-low": {"displayName": "Gemini 3.1 Pro (Low)", "quotaInfo": {"remainingFraction": 0.8}},
                "gemini-3.8-flash-high": {"displayName": "Gemini 3.8 Flash (High)", "quotaInfo": {"remainingFraction": 0.9}},
                "gemini-3.8-flash-medium": {"displayName": "Gemini 3.8 Flash (Medium)", "quotaInfo": {"remainingFraction": 0.9}},
                "gemini-3.8-flash-low": {"displayName": "Gemini 3.8 Flash (Low)", "quotaInfo": {"remainingFraction": 0.9}},
                "gemini-3-flash": {"displayName": "Gemini 3 Flash", "quotaInfo": {"remainingFraction": 0.5}},
                "gemini-3.1-flash-image": {"displayName": "Gemini 3.1 Flash Image", "quotaInfo": {"remainingFraction": 1.0}},
                "claude-sonnet-4-6": {"displayName": "Claude Sonnet 4.6", "quotaInfo": {"remainingFraction": 0.6, "resetTime": "2026-11-20T00:00:00Z"}},
                "claude-opus-4-6-thinking": {"displayName": "Claude Opus 4.6 (Thinking)", "quotaInfo": {"remainingFraction": 0.6}},
                "gpt-oss-120b-medium": {"displayName": "GPT-OSS 120B (Medium)", "quotaInfo": {"remainingFraction": 0.6}},
                "disabled-one": {"displayName": "Disabled", "disabled": true, "quotaInfo": {"remainingFraction": 0.0}}
            }
        }"#;
        let parsed = parse_quota(body).expect("parse");
        let summary = summarize_quotas(&parsed);
        let names: Vec<&str> = summary.iter().map(|q| q.name.as_str()).collect();
        assert_eq!(names, vec!["Gemini", "Claude 4.6 / GPT"]);
        assert_eq!(summary[0].remaining, 0.8);
        assert_eq!(summary[0].reset.as_deref(), Some("2026-11-18T14:00:00Z"));
        assert_eq!(summary[1].remaining, 0.6);
        assert_eq!(summary[1].reset.as_deref(), Some("2026-11-20T00:00:00Z"));
    }

    #[test]
    fn parses_tier_summary_groups_and_argon_eligibility() {
        let code_assist_ultra = r#"{"cloudaicompanionProject":"proj-1","paidTier":{"id":"g1-ultra-tier","name":"Google AI Ultra"}}"#;
        let (pid, tier) = parse_code_assist(code_assist_ultra);
        assert_eq!(pid.as_deref(), Some("proj-1"));
        assert_eq!(tier, Some(AccountTier::Ultra));

        let code_assist_pro = r#"{"paidTier":{"id":"g1-pro-tier"}}"#;
        assert_eq!(parse_code_assist(code_assist_pro).1, Some(AccountTier::Pro));

        let code_assist_free = r#"{"currentTier":{"id":"free-tier","name":"Antigravity Starter Quota"}}"#;
        assert_eq!(parse_code_assist(code_assist_free).1, Some(AccountTier::Free));

        let summary_json = r#"{
            "groups": [
                {"displayName":"Gemini","buckets":[
                    {"bucketId":"gemini-5h","window":"5h","remainingFraction":0.85,"resetTime":"2026-11-18T17:00:00Z"},
                    {"bucketId":"gemini-weekly","window":"weekly","remainingFraction":0.95,"resetTime":"2026-11-24T00:00:00Z"}
                ]},
                {"displayName":"Claude and GPT","buckets":[
                    {"bucketId":"3p-5h","window":"5h","remainingFraction":0.7,"resetTime":"2026-11-18T17:00:00Z"}
                ]},
                {"displayName":"Gemini 4 Argon","buckets":[
                    {"bucketId":"gemini-argon-wtu-weekly","window":"weekly","remainingFraction":0.65,"resetTime":"2026-11-23T00:00:00Z"}
                ]}
            ]
        }"#;
        let groups = parse_quota_summary(summary_json);
        assert_eq!(groups.len(), 3);

        let models_json = r#"{
            "models": {
                "gemini-3.1-pro-high": {"displayName":"Gemini 3.1 Pro (High)"},
                "gemini-3.8-flash-high": {"displayName":"Gemini 3.8 Flash (High)"},
                "claude-sonnet-4-6": {"displayName":"Claude Sonnet 4.6","quotaInfo":{"remainingFraction":0.7}},
                "claude-opus-5-5-high": {"displayName":"Claude Opus 5.5 (High)","quotaInfo":{"remainingFraction":0.7}},
                "gpt-oss-120b-medium": {"displayName":"GPT-OSS 120B (Medium)","quotaInfo":{"remainingFraction":0.7}}
            }
        }"#;
        // Pro with explicit Argon group receives Argon weekly quota 0.65, Gemini dual 5h (0.85) + weekly (0.95), and Claude 5.5&4.6 / GPT
        let pro_with_argon =
            build_account_quotas(models_json, Some(&groups), Some(AccountTier::Pro))
                .expect("pro argon");
        assert_eq!(pro_with_argon.tier, Some(AccountTier::Pro));
        let names: Vec<&str> = pro_with_argon.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["Gemini", "Claude 5.5&4.6 / GPT", "Gemini Argon"]);
        assert!((pro_with_argon.models[0].remaining - 0.85).abs() < 1e-4);
        assert_eq!(pro_with_argon.models[0].weekly_remaining, Some(0.95));
        assert!(!pro_with_argon.models[0].is_weekly_only);
        assert!((pro_with_argon.models[2].remaining - 0.65).abs() < 1e-4);
        assert!(pro_with_argon.models[2].is_weekly_only);
        assert_eq!(pro_with_argon.models[2].weekly_remaining, Some(0.65));

        // Free without Argon group gets only Gemini (weekly-only 0.95) + Claude (weekly-only)
        let no_argon_groups = &groups[..2];
        let free_res =
            build_account_quotas(models_json, Some(no_argon_groups), Some(AccountTier::Free))
                .expect("free");
        let free_names: Vec<&str> = free_res.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(free_names, vec!["Gemini", "Claude 5.5&4.6 / GPT"]);
        assert!(free_res.models[0].is_weekly_only);
        assert!((free_res.models[0].remaining - 0.95).abs() < 1e-4);

        // Ultra shares Gemini weekly quota for Argon (weekly-only, no 5H) at bottom when no split Argon group exists
        let ultra_res =
            build_account_quotas(models_json, Some(no_argon_groups), Some(AccountTier::Ultra))
                .expect("ultra");
        let ultra_names: Vec<&str> = ultra_res.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(ultra_names, vec!["Gemini", "Claude 5.5&4.6 / GPT", "Gemini Argon"]);
        assert!((ultra_res.models[2].remaining - 0.95).abs() < 1e-4);
        assert_eq!(ultra_res.models[2].weekly_remaining, Some(0.95));
        assert!(ultra_res.models[2].is_weekly_only);
    }
}

#[cfg(test)]
mod proxy_request_tests {
    use super::*;

    /// A request that never leaves the machine: an unused port is refused, and
    /// the error names the proxy rather than the network.
    #[test]
    fn a_dead_proxy_fails_loudly() {
        let proxy = ProxyConfig {
            enabled: true,
            protocol: "http".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9,
        };
        let error = request("GET", USERINFO_URL, &[], None, &proxy).expect_err("no proxy there");
        assert!(error.contains("经代理"), "{error}");
    }

    #[test]
    fn disabled_proxy_bypasses_dead_proxy_and_connects_directly() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("address").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            let mut buffer = [0_u8; 512];
            let _ = std::io::Read::read(&mut stream, &mut buffer);
            let _ = std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            );
        });
        let proxy = ProxyConfig {
            enabled: false,
            protocol: "http".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9,
        };
        let (status, body) =
            request("GET", &format!("http://127.0.0.1:{port}/health"), &[], None, &proxy)
                .expect("direct request should bypass dead proxy");
        server.join().unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, "ok");
    }

    /// The quota request reaches the proxy with the quota host, not the
    /// system proxy: the CONNECT target is the endpoint's own host.
    #[test]
    fn the_quota_request_tunnels_through_the_configured_proxy() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("address").port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured = seen.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut data = Vec::new();
            let mut buffer = [0_u8; 1024];
            while let Ok(read) = std::io::Read::read(&mut stream, &mut buffer) {
                if read == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..read]);
                if data.windows(2).any(|w| w == b"\r\n") {
                    break;
                }
            }
            *captured.lock().unwrap() = String::from_utf8_lossy(&data).to_string();
        });
        let proxy = ProxyConfig {
            enabled: true,
            protocol: "http".to_owned(),
            host: "127.0.0.1".to_owned(),
            port,
        };
        let error = request(
            "POST",
            "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
            &[],
            Some("{}"),
            &proxy,
        )
        .expect_err("the stub proxy never answers");
        server.join().unwrap();
        let request_line = seen.lock().unwrap().clone();
        assert!(
            request_line.contains("CONNECT daily-cloudcode-pa.googleapis.com:443"),
            "{request_line}"
        );
        assert!(error.contains("经代理"), "{error}");
    }

    #[test]
    fn proxy_probe_tunnels_and_rejects_dead_proxy() {
        let dead = ProxyConfig {
            enabled: false,
            protocol: "http".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9,
        };
        assert!(probe_proxy_url(PROXY_PROBE_URL, &dead).is_err());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("address").port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured = seen.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut data = Vec::new();
            let mut buffer = [0_u8; 512];
            while let Ok(read) = std::io::Read::read(&mut stream, &mut buffer) {
                if read == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..read]);
                if data.windows(2).any(|w| w == b"\r\n") {
                    break;
                }
            }
            *captured.lock().unwrap() = String::from_utf8_lossy(&data).to_string();
        });
        let proxy = ProxyConfig {
            enabled: false,
            protocol: "http".to_owned(),
            host: "127.0.0.1".to_owned(),
            port,
        };
        assert!(probe_proxy_url(PROXY_PROBE_URL, &proxy).is_err());
        server.join().unwrap();
        let request_line = seen.lock().unwrap().clone();
        assert!(request_line.contains("CONNECT www.gstatic.com:443"), "{request_line}");
    }

    /// The path that matters: a real request through the configured proxy.
    /// Run with `cargo test -- --ignored` while a proxy listens on 7890.
    #[test]
    #[ignore = "needs a proxy on 127.0.0.1:7890"]
    fn a_request_goes_through_the_proxy() {
        let proxy = ProxyConfig {
            enabled: true,
            protocol: "http".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 7890,
        };
        let (status, body) = request(
            "POST",
            TOKEN_URL,
            &[
                ("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned()),
                ("User-Agent".to_owned(), USER_AGENT.to_owned()),
            ],
            Some("grant_type=refresh_token"),
            &proxy,
        )
        .expect("the proxy answers");
        // Google rejects the bogus form with 400; reaching Google at all is the point.
        assert_eq!(status, 400, "{body}");
        assert!(body.contains("error"), "{body}");
    }
}
