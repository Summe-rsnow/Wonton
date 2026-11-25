//! The app's state: accounts, quotas, the proxy target and the notice line.
//! The signal stays private; views read and change it through these
//! functions, so nothing outside writes state directly.

mod actions;

pub(crate) use actions::{
    add_account_by_oauth, add_tokens, check_for_updates_action, export_accounts_action,
    import_accounts_action, import_local, launch_account_target, launch_target_app,
    refresh_account_now, refresh_all, refresh_selected_or_all, test_proxy_action,
};

use nsh_core::prelude::*;
use nsh_core::storage::Storage;
use nsh_core::time::unix_ms;
use serde::{Deserialize, Deserializer, Serialize};

use crate::google::{self, AccountTier, ModelQuota};
use crate::launch::ProxyConfig;

pub(crate) use bento::shell::{CloseBehavior, Notice, NoticeCenter, NoticeTone as Tone, ThemeMode};
pub(crate) use bento::updater::{ReleaseInfo, UpdateStatus};

const STORAGE_KEY: &str = "wonton.state.v1";
const ENCRYPTED_PREFIX: &str = "wonton:enc:v1:";

fn default_true() -> bool {
    true
}

/// A saved account: what survives a restart. Tokens of the current session
/// are not saved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    accounts: Vec<SavedAccount>,
    #[serde(default)]
    proxy: SavedProxy,
    #[serde(default)]
    theme: ThemeMode,
    #[serde(default)]
    close_behavior: CloseBehavior,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_used_email: Option<String>,
}

fn deserialize_tier_compat<'de, D>(deserializer: D) -> Result<Option<AccountTier>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;
    Ok(raw.and_then(|value| match value.as_str() {
        "Free" => Some(AccountTier::Free),
        "Pro" => Some(AccountTier::Pro),
        "Ultra" => Some(AccountTier::Ultra),
        _ => None,
    }))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SavedAccount {
    email: String,
    refresh_token: String,
    #[serde(default, deserialize_with = "deserialize_tier_compat")]
    tier: Option<AccountTier>,
    #[serde(default)]
    models: Vec<ModelQuota>,
    #[serde(default)]
    fetched_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SavedProxy {
    #[serde(default = "default_true")]
    enabled: bool,
    protocol: String,
    host: String,
    port: u16,
}

impl Default for SavedProxy {
    fn default() -> Self {
        let proxy = ProxyConfig::default();
        Self {
            enabled: proxy.enabled,
            protocol: proxy.protocol,
            host: proxy.host,
            port: proxy.port,
        }
    }
}

/// An account as the screen shows it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Account {
    pub(crate) id: u32,
    pub(crate) email: String,
    pub(crate) refresh_token: String,
    pub(crate) tier: Option<AccountTier>,
    pub(crate) models: Vec<ModelQuota>,
    pub(crate) fetched_at: Option<i64>,
    pub(crate) error: Option<String>,
    pub(crate) needs_login: bool,
    /// The session's access token: never saved.
    pub(crate) access_token: Option<String>,
    pub(crate) expires_at: i64,
}

/// UI-facing projection of an [`Account`], omitting credentials so token
/// renewals do not trigger unnecessary card re-evaluations.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AccountCardView {
    pub(crate) email: String,
    pub(crate) tier: Option<AccountTier>,
    pub(crate) models: Vec<ModelQuota>,
    pub(crate) fetched_at: Option<i64>,
    pub(crate) error: Option<String>,
    pub(crate) needs_login: bool,
    pub(crate) refreshing: bool,
    pub(crate) is_last_used: bool,
}

/// Progress of an in-flight batch JSON import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportProgress {
    pub(crate) current: usize,
    pub(crate) total: usize,
    pub(crate) success: usize,
    pub(crate) failed: usize,
    pub(crate) current_email: String,
}

#[derive(Debug, Clone, PartialEq)]
struct State {
    accounts: Vec<Account>,
    next_id: u32,
    proxy: ProxyConfig,
    theme: ThemeMode,
    close_behavior: CloseBehavior,
    last_used_email: Option<String>,
    busy: bool,
    proxy_testing: bool,
    switching_account_id: Option<u32>,
    refreshing_ids: Vec<u32>,
    target: Option<String>,
    selected_ids: Vec<u32>,
    import_progress: Option<ImportProgress>,
    update_status: UpdateStatus,
}

impl Default for State {
    fn default() -> Self {
        Self {
            accounts: Vec::new(),
            next_id: 1,
            proxy: ProxyConfig::default(),
            theme: ThemeMode::System,
            close_behavior: CloseBehavior::Ask,
            last_used_email: None,
            busy: false,
            proxy_testing: false,
            switching_account_id: None,
            refreshing_ids: Vec::new(),
            target: crate::launch::detect_target().map(|path| path.display().to_string()),
            selected_ids: Vec::new(),
            import_progress: None,
            update_status: UpdateStatus::Idle,
        }
    }
}

fn state() -> Signal<State> {
    global::<State>()
}

/// Reads the saved accounts, proxy, theme, and close preference, once, when the app starts.
pub(crate) fn load() {
    bento::shell::init_autostart("com.xcmm.wonton", Some("Wonton"));
    load_without_os_autostart();
}

/// Reads persisted app state without querying the OS autostart registry (used by headless tests).
pub(crate) fn load_without_os_autostart() {
    let Some(raw) = Storage::get_item(STORAGE_KEY) else {
        set_color_scheme(Appearance::System);
        return;
    };
    let Some(json) = decode_storage(&raw) else {
        set_color_scheme(Appearance::System);
        return;
    };
    let Ok(saved) = serde_json::from_str::<Saved>(&json) else {
        set_color_scheme(Appearance::System);
        return;
    };
    let loaded_theme = saved.theme;
    let loaded_close_behavior = saved.close_behavior;
    let loaded_last_used_email = saved.last_used_email.and_then(|email| {
        let trimmed = email.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    });
    state().update(|state| {
        for account in saved.accounts {
            let id = state.next_id;
            state.next_id += 1;
            let models = if account.models.is_empty() {
                Vec::new()
            } else {
                google::summarize_quotas(&account.models)
            };
            state.accounts.push(Account {
                id,
                email: account.email,
                refresh_token: account.refresh_token,
                tier: account.tier,
                models,
                fetched_at: account.fetched_at,
                error: None,
                needs_login: false,
                access_token: None,
                expires_at: 0,
            });
        }
        state.proxy = ProxyConfig {
            enabled: saved.proxy.enabled,
            protocol: "http".to_owned(),
            host: saved.proxy.host,
            port: saved.proxy.port,
        };
        state.theme = loaded_theme;
        state.close_behavior = loaded_close_behavior;
        state.last_used_email = loaded_last_used_email;
    });
    set_color_scheme(loaded_theme.to_appearance());
}

/// Writes the accounts, proxy, theme, and close preference to storage.
fn save() {
    let saved = state().with(|state| Saved {
        accounts: state
            .accounts
            .iter()
            .map(|account| SavedAccount {
                email: account.email.clone(),
                refresh_token: account.refresh_token.clone(),
                tier: account.tier,
                models: account.models.clone(),
                fetched_at: account.fetched_at,
            })
            .collect(),
        proxy: SavedProxy {
            enabled: state.proxy.enabled,
            protocol: state.proxy.protocol.clone(),
            host: state.proxy.host.clone(),
            port: state.proxy.port,
        },
        theme: state.theme,
        close_behavior: state.close_behavior,
        last_used_email: state.last_used_email.clone(),
    });
    if let Ok(json) = serde_json::to_string(&saved)
        && let Some(encoded) = encode_storage(&json)
    {
        let _ = Storage::set_item(STORAGE_KEY, encoded);
    }
}

fn storage_key() -> Option<&'static ring::aead::LessSafeKey> {
    use std::sync::OnceLock;
    use ring::{aead, hkdf};

    static KEY: OnceLock<Option<aead::LessSafeKey>> = OnceLock::new();
    KEY.get_or_init(|| {
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, b"com.xcmm.wonton.storage.salt.v1");
        let prk = salt.extract(b"wonton.local.storage.master.v1");
        let info = [STORAGE_KEY.as_bytes()];
        let okm = prk.expand(&info, &aead::AES_256_GCM).ok()?;
        let mut key_bytes = [0u8; 32];
        okm.fill(&mut key_bytes).ok()?;
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &key_bytes).ok()?;
        Some(aead::LessSafeKey::new(unbound))
    })
    .as_ref()
}

/// Encrypts serialized state with cross-platform AES-256-GCM (`ring`).
fn encode_storage(json: &str) -> Option<String> {
    use base64::Engine;
    use ring::{aead, rand::{SecureRandom, SystemRandom}};

    let key = storage_key()?;
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; aead::NONCE_LEN];
    rng.fill(&mut nonce_bytes).ok()?;
    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);

    let mut buf = Vec::with_capacity(aead::NONCE_LEN + json.len() + aead::AES_256_GCM.tag_len());
    buf.extend_from_slice(&nonce_bytes);
    buf.extend_from_slice(json.as_bytes());
    let tag = key
        .seal_in_place_separate_tag(nonce, aead::Aad::from(STORAGE_KEY.as_bytes()), &mut buf[aead::NONCE_LEN..])
        .ok()?;
    buf.extend_from_slice(tag.as_ref());

    let encoded = base64::engine::general_purpose::STANDARD.encode(buf);
    Some(format!("{ENCRYPTED_PREFIX}{encoded}"))
}

/// Decrypts AES-256-GCM persisted state (no legacy plaintext fallback).
fn decode_storage(raw: &str) -> Option<String> {
    use base64::Engine;
    use ring::aead;

    let encoded = raw.strip_prefix(ENCRYPTED_PREFIX)?;
    let mut payload = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    if payload.len() < aead::NONCE_LEN + aead::AES_256_GCM.tag_len() {
        return None;
    }
    let key = storage_key()?;
    let (nonce_bytes, cipher_and_tag) = payload.split_at_mut(aead::NONCE_LEN);
    let nonce = aead::Nonce::try_assume_unique_for_key(nonce_bytes).ok()?;
    let plain = key
        .open_in_place(nonce, aead::Aad::from(STORAGE_KEY.as_bytes()), cipher_and_tag)
        .ok()?;
    String::from_utf8(plain.to_vec()).ok()
}

/// The accounts' ids, in order (tracked where it is read).
pub(crate) fn account_ids() -> Vec<u32> {
    state().with(|state| state.accounts.iter().map(|account| account.id).collect())
}

/// How many accounts there are (tracked where it is read).
pub(crate) fn account_count() -> usize {
    state().with(|state| state.accounts.len())
}

/// One account, or none once it is removed (tracked where it is read).
pub(crate) fn account_by_id(id: u32) -> Option<Account> {
    state().with(|state| state.accounts.iter().find(|account| account.id == id).cloned())
}

/// UI-only projection for an account card (tracked where it is read).
pub(crate) fn account_card_view(id: u32) -> Option<AccountCardView> {
    state().with(|state| {
        let refreshing = state.refreshing_ids.contains(&id);
        let last_used = state.last_used_email.as_deref();
        state
            .accounts
            .iter()
            .find(|account| account.id == id)
            .map(|account| AccountCardView {
                email: account.email.clone(),
                tier: account.tier,
                models: account.models.clone(),
                fetched_at: account.fetched_at,
                error: account.error.clone(),
                needs_login: account.needs_login,
                refreshing,
                is_last_used: last_used.is_some_and(|e| e.eq_ignore_ascii_case(&account.email)),
            })
    })
}

/// Records `email` as the most recently launched/switched account and persists it.
pub(crate) fn mark_last_used_account(email: &str) {
    let trimmed = email.trim();
    if trimmed.is_empty() {
        return;
    }
    state().update(|state| state.last_used_email = Some(trimmed.to_owned()));
    save();
}

/// Returns the email of the last launched/switched account, if any (tracked where read).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn last_used_email() -> Option<String> {
    state().with(|state| state.last_used_email.clone())
}

/// The selected appearance preference (`system`, `light`, or `dark`).
pub(crate) fn theme_mode() -> ThemeMode {
    state().with(|state| state.theme)
}

/// Applies and persists the color scheme mode.
pub(crate) fn set_theme_mode(mode: ThemeMode) {
    state().update(|state| state.theme = mode);
    set_color_scheme(mode.to_appearance());
    save();
}

/// What clicking the window close button does (`Ask`, `Tray`, or `Exit`).
pub(crate) fn close_behavior() -> CloseBehavior {
    state().with(|state| state.close_behavior)
}

/// Persists the preferred close button behavior.
pub(crate) fn set_close_behavior(behavior: CloseBehavior) {
    state().update(|state| state.close_behavior = behavior);
    save();
}

/// The proxy the app will hand to Antigravity (tracked where it is read).
pub(crate) fn proxy() -> ProxyConfig {
    state().with(|state| state.proxy.clone())
}

/// Toggles whether HTTP proxy routing is active, persisting immediately.
pub(crate) fn set_proxy_enabled(enabled: bool) {
    state().update(|state| state.proxy.enabled = enabled);
    save();
    if enabled {
        set_notice("已启用 HTTP 代理", Tone::Info);
    } else {
        set_notice("已关闭 HTTP 代理（将使用直连）", Tone::Info);
    }
}

/// The notice line, if there is one (tracked where it is read).
pub(crate) fn notice() -> Option<Notice> {
    NoticeCenter::global().current()
}

/// Whether a network operation is running (tracked where it is read).
pub(crate) fn busy() -> bool {
    state().with(|state| state.busy)
}

pub(crate) fn set_notice(text: impl Into<String>, tone: Tone) {
    NoticeCenter::global().notify(text, tone);
}

pub(crate) fn clear_notice() {
    NoticeCenter::global().clear();
}

pub(crate) fn set_busy(busy: bool) {
    state().update(|state| state.busy = busy);
}

pub(crate) fn proxy_testing() -> bool {
    state().with(|state| state.proxy_testing)
}

fn set_proxy_testing(testing: bool) {
    state().update(|state| state.proxy_testing = testing);
}

pub(crate) fn switching_account_id() -> Option<u32> {
    state().with(|state| state.switching_account_id)
}

fn set_switching_account_id(id: Option<u32>) {
    state().update(|state| state.switching_account_id = id);
}

pub(crate) fn is_account_refreshing(id: u32) -> bool {
    state().with(|state| state.refreshing_ids.contains(&id))
}

pub(crate) fn is_any_refreshing() -> bool {
    state().with(|state| !state.refreshing_ids.is_empty())
}

pub(crate) fn set_account_refreshing(id: u32, refreshing: bool) {
    state().update(|state| {
        if refreshing {
            if !state.refreshing_ids.contains(&id) {
                state.refreshing_ids.push(id);
            }
        } else {
            state.refreshing_ids.retain(|existing| *existing != id);
        }
    });
}

pub(crate) fn import_progress() -> Option<ImportProgress> {
    state().with(|state| state.import_progress.clone())
}

fn set_import_progress(progress: Option<ImportProgress>) {
    state().update(|state| state.import_progress = progress);
}

pub(crate) fn update_status() -> UpdateStatus {
    state().with(|state| state.update_status.clone())
}

pub(crate) fn update_checking() -> bool {
    state().with(|state| matches!(state.update_status, UpdateStatus::Checking))
}

pub(crate) fn available_update() -> Option<ReleaseInfo> {
    state().with(|state| match &state.update_status {
        UpdateStatus::Available(info) => Some(info.clone()),
        _ => None,
    })
}

fn set_update_status(status: UpdateStatus) {
    state().update(|state| state.update_status = status);
}

pub(crate) fn selected_count() -> usize {
    state().with(|state| state.selected_ids.len())
}

pub(crate) fn is_selected(id: u32) -> bool {
    state().with(|state| state.selected_ids.contains(&id))
}

pub(crate) fn all_selected() -> bool {
    state().with(|state| {
        !state.accounts.is_empty() && state.selected_ids.len() == state.accounts.len()
    })
}

pub(crate) fn some_selected() -> bool {
    state().with(|state| {
        !state.selected_ids.is_empty() && state.selected_ids.len() < state.accounts.len()
    })
}

pub(crate) fn toggle_select(id: u32) {
    state().update(|state| {
        if let Some(pos) = state.selected_ids.iter().position(|&selected| selected == id) {
            state.selected_ids.remove(pos);
        } else if state.accounts.iter().any(|account| account.id == id) {
            state.selected_ids.push(id);
        }
    });
}

pub(crate) fn toggle_select_all() {
    state().update(|state| {
        if !state.accounts.is_empty() && state.selected_ids.len() == state.accounts.len() {
            state.selected_ids.clear();
        } else {
            state.selected_ids = state.accounts.iter().map(|account| account.id).collect();
        }
    });
}

pub(crate) fn clear_selection() {
    state().update(|state| state.selected_ids.clear());
}

/// Selected account ids in their current list order.
pub(crate) fn selected_account_ids() -> Vec<u32> {
    state().with(filter_selected_account_ids)
}

fn filter_selected_account_ids(state: &State) -> Vec<u32> {
    state
        .accounts
        .iter()
        .filter(|account| state.selected_ids.contains(&account.id))
        .map(|account| account.id)
        .collect()
}

/// Accounts formatted for export: a single id, selected ids, or all accounts when none selected.
pub(crate) fn export_items(single_id: Option<u32>) -> Vec<google::ExportAccount> {
    state().with(|state| select_export_items(state, single_id))
}

fn select_export_items(state: &State, single_id: Option<u32>) -> Vec<google::ExportAccount> {
    let to_item = |account: &Account| google::ExportAccount {
        email: account.email.clone(),
        refresh_token: account.refresh_token.clone(),
    };
    if let Some(id) = single_id {
        return state
            .accounts
            .iter()
            .find(|account| account.id == id)
            .map(to_item)
            .into_iter()
            .collect();
    }
    if state.selected_ids.is_empty() {
        return state.accounts.iter().map(to_item).collect();
    }
    state
        .accounts
        .iter()
        .filter(|account| state.selected_ids.contains(&account.id))
        .map(to_item)
        .collect()
}

/// Saves the proxy the fields describe; the form validates before calling.
pub(crate) fn set_proxy(proxy: ProxyConfig) {
    state().update(|state| state.proxy = proxy);
    save();
}

/// Removes one or more accounts, and says so, since the rows simply disappear.
pub(crate) fn remove_accounts(ids: &[u32]) {
    if ids.is_empty() {
        return;
    }
    let mut removed = 0usize;
    state().update(|state| {
        removed = remove_accounts_from_state(state, ids);
    });
    if removed == 0 {
        return;
    }
    save();
    if removed == 1 {
        set_notice("已删除账号", Tone::Info);
    } else {
        set_notice(format!("已删除 {removed} 个账号"), Tone::Info);
    }
}

fn remove_accounts_from_state(state: &mut State, ids: &[u32]) -> usize {
    let before = state.accounts.len();
    let clears_last_used = state.last_used_email.as_deref().is_some_and(|last| {
        state
            .accounts
            .iter()
            .any(|account| ids.contains(&account.id) && account.email.eq_ignore_ascii_case(last))
    });
    state.accounts.retain(|account| !ids.contains(&account.id));
    state
        .selected_ids
        .retain(|selected| !ids.contains(selected) && state.accounts.iter().any(|a| a.id == *selected));
    state.refreshing_ids.retain(|id| !ids.contains(id));
    if clears_last_used {
        state.last_used_email = None;
    }
    before.saturating_sub(state.accounts.len())
}

/// Moves account `id` to `to_index` cleanly and persists the new order.
pub(crate) fn reorder_account(id: u32, to_index: usize) {
    let mut changed = false;
    state().update(|state| {
        changed = move_account_in_list(&mut state.accounts, id, to_index);
    });
    if changed {
        save();
    }
}

fn move_account_in_list(accounts: &mut Vec<Account>, id: u32, to_index: usize) -> bool {
    let Some(from_index) = accounts.iter().position(|account| account.id == id) else {
        return false;
    };
    let clamped = to_index.min(accounts.len().saturating_sub(1));
    if from_index == clamped {
        return false;
    }
    let account = accounts.remove(from_index);
    accounts.insert(clamped, account);
    true
}

fn update_account(id: u32, change: impl FnOnce(&mut Account)) {
    state().update(|state| {
        if let Some(account) = state.accounts.iter_mut().find(|account| account.id == id) {
            change(account);
        }
    });
}

/// Adds an account that already has a token and an e-mail, at the end.
fn insert_account(email: &str, refresh_token: &str) -> u32 {
    let mut id = 0;
    state().update(|state| {
        if let Some(existing) = state.accounts.iter_mut().find(|account| {
            account.email.eq_ignore_ascii_case(email) || account.refresh_token == refresh_token
        }) {
            existing.email = email.to_owned();
            existing.refresh_token = refresh_token.to_owned();
            existing.error = None;
            existing.needs_login = false;
            existing.access_token = None;
            existing.expires_at = 0;
            id = existing.id;
            return;
        }
        id = state.next_id;
        state.next_id += 1;
        state.accounts.push(Account {
            id,
            email: email.to_owned(),
            refresh_token: refresh_token.to_owned(),
            tier: None,
            models: Vec::new(),
            fetched_at: None,
            error: None,
            needs_login: false,
            access_token: None,
            expires_at: 0,
        });
    });
    save();
    id
}

fn unexpired_reset(reset: &Option<String>, now_ms: i64) -> Option<String> {
    reset.as_ref().and_then(|ts| {
        crate::format::parse_rfc3339(ts)
            .filter(|&at| at > now_ms)
            .map(|_| ts.clone())
    })
}

fn merge_quota_models(
    prev_models: &[ModelQuota],
    mut next_models: Vec<ModelQuota>,
    now_ms: i64,
) -> Vec<ModelQuota> {
    for row in &mut next_models {
        let prev = prev_models
            .iter()
            .find(|m| m.name == row.name)
            .or_else(|| prev_models.iter().find(|m| google::same_quota_bucket(&m.name, &row.name)));
        if let Some(prev) = prev {
            if !row.is_weekly_only
                && row.weekly_remaining.is_none()
                && prev.weekly_remaining.is_some()
            {
                row.weekly_remaining = prev.weekly_remaining;
                row.weekly_reset = prev.weekly_reset.clone();
            } else if row.is_weekly_only
                && !prev.is_weekly_only
                && !row.name.to_lowercase().contains("argon")
            {
                row.remaining = prev.remaining;
                row.reset = prev.reset.clone();
                row.is_weekly_only = false;
            }

            if row.reset.is_none() {
                if row.is_weekly_only {
                    row.reset = unexpired_reset(&prev.weekly_reset, now_ms)
                        .or_else(|| unexpired_reset(&prev.reset, now_ms));
                } else if !prev.is_weekly_only {
                    row.reset = unexpired_reset(&prev.reset, now_ms);
                }
            }
            if (row.is_weekly_only || row.weekly_remaining.is_some()) && row.weekly_reset.is_none() {
                row.weekly_reset = unexpired_reset(&prev.weekly_reset, now_ms);
                if row.is_weekly_only && row.reset.is_none() {
                    row.reset = row.weekly_reset.clone();
                }
            }
        }
    }
    next_models
}

fn set_quota(id: u32, snapshot: google::AccountQuotaSnapshot) {
    let now_ms = unix_ms() as i64;
    update_account(id, |account| {
        account.models = merge_quota_models(&account.models, snapshot.models, now_ms);
        if let Some(tier) = snapshot.tier {
            account.tier = Some(tier);
        }
        account.fetched_at = Some(now_ms);
        account.error = None;
        account.needs_login = false;
    });
    save();
}

fn set_error(id: u32, error: String, needs_login: bool) {
    update_account(id, |account| {
        account.error = Some(error);
        account.needs_login = needs_login;
        if needs_login {
            account.access_token = None;
        }
    });
}

fn set_token(id: u32, tokens: google::Tokens) {
    update_account(id, |account| {
        account.access_token = Some(tokens.access_token);
        account.expires_at = tokens.expires_at;
    });
}

/// A usable access token for `account`, renewing it when it is close to
/// expiry. Writes the renewed token to the state.
async fn access_token(account: &Account) -> Result<String, String> {
    let valid_for = account.expires_at - unix_ms() as i64;
    if let Some(token) = &account.access_token
        && valid_for > TOKEN_SKEW_MS
    {
        return Ok(token.clone());
    }
    let tokens = google::refresh(&account.refresh_token, proxy()).await?;
    let access = tokens.access_token.clone();
    set_token(account.id, tokens);
    Ok(access)
}

/// An access token is renewed this long before it expires.
const TOKEN_SKEW_MS: i64 = 15 * 60 * 1000;

fn is_dead_token(error: &str) -> bool {
    error.contains("invalid_grant") || error.contains("HTTP 400") || error.contains("HTTP 401")
}

/// Reads one account's quota and writes it; the last step of every add, and
/// what the refresh commands do.
async fn refresh_account(id: u32) {
    let Some(account) = account_by_id(id) else {
        set_account_refreshing(id, false);
        return;
    };
    set_account_refreshing(id, true);
    let token = match access_token(&account).await {
        Ok(token) => token,
        Err(error) => {
            set_error(id, error.clone(), is_dead_token(&error));
            set_account_refreshing(id, false);
            return;
        }
    };
    match google::quota(&token, proxy()).await {
        Ok(snapshot) => set_quota(id, snapshot),
        Err(error) => set_error(id, error, false),
    }
    set_account_refreshing(id, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_state_round_trips() {
        let saved = Saved {
            accounts: vec![SavedAccount {
                email: "a@b.c".to_owned(),
                refresh_token: "1//0gTOKEN".to_owned(),
                tier: Some(AccountTier::Ultra),
                models: vec![ModelQuota {
                    name: "Claude Sonnet".to_owned(),
                    remaining: 0.5,
                    reset: None,
                    weekly_remaining: Some(0.8),
                    weekly_reset: Some("2026-11-20T00:00:00Z".to_owned()),
                    is_weekly_only: false,
                }],
                fetched_at: Some(1_794_659_696_000),
            }],
            proxy: SavedProxy {
                enabled: false,
                protocol: "http".to_owned(),
                host: "127.0.0.1".to_owned(),
                port: 7890,
            },
            theme: ThemeMode::Dark,
            close_behavior: CloseBehavior::Tray,
            last_used_email: Some("a@b.c".to_owned()),
        };
        let json = serde_json::to_string(&saved).expect("json");
        assert!(json.contains("\"last_used_email\":\"a@b.c\""));
        assert_eq!(serde_json::from_str::<Saved>(&json).expect("saved"), saved);

        let without_last_used = Saved {
            last_used_email: None,
            ..saved
        };
        let omitted_json = serde_json::to_string(&without_last_used).expect("json");
        assert!(!omitted_json.contains("last_used_email"));
        assert_eq!(
            serde_json::from_str::<Saved>(&omitted_json).expect("saved"),
            without_last_used
        );
    }

    #[test]
    fn reads_an_older_state_without_the_newer_fields_and_ignores_legacy_enterprise() {
        let saved: Saved = serde_json::from_str(
            r#"{"accounts":[{"email":"a@b.c","refresh_token":"1//0gT"},{"email":"old@b.c","refresh_token":"1//0gE","kind":"Enterprise","tier":"Enterprise"}]}"#,
        )
        .expect("saved");
        assert_eq!(saved.accounts.len(), 2);
        assert_eq!(saved.accounts[0].tier, None);
        assert_eq!(saved.accounts[1].tier, None);
        assert!(saved.accounts[0].models.is_empty());
        assert_eq!(saved.proxy.host, "127.0.0.1");
        assert!(saved.proxy.enabled);
        assert_eq!(saved.theme, ThemeMode::System);
        assert_eq!(saved.close_behavior, CloseBehavior::Ask);
        assert_eq!(saved.last_used_email, None);
    }

    #[test]
    fn knows_a_dead_refresh_token() {
        assert!(is_dead_token(r#"续期失败：HTTP 400 {"error":"invalid_grant"}"#));
        assert!(is_dead_token("HTTP 401"));
        assert!(!is_dead_token("HTTP 500"));
    }

    #[test]
    fn selection_and_export_items_respect_selected_or_all() {
        let state = State {
            accounts: vec![
                Account {
                    id: 1,
                    email: "a@b.c".to_owned(),
                    refresh_token: "1//a".to_owned(),
                    tier: Some(AccountTier::Pro),
                    models: Vec::new(),
                    fetched_at: None,
                    error: None,
                    needs_login: false,
                    access_token: None,
                    expires_at: 0,
                },
                Account {
                    id: 2,
                    email: "d@e.f".to_owned(),
                    refresh_token: "1//d".to_owned(),
                    tier: Some(AccountTier::Ultra),
                    models: Vec::new(),
                    fetched_at: None,
                    error: None,
                    needs_login: false,
                    access_token: None,
                    expires_at: 0,
                },
            ],
            next_id: 3,
            selected_ids: vec![2],
            ..State::default()
        };
        assert_eq!(select_export_items(&state, Some(1)).len(), 1);
        assert_eq!(select_export_items(&state, Some(1))[0].email, "a@b.c");
        let selected = select_export_items(&state, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].email, "d@e.f");
        let unselected = State { selected_ids: Vec::new(), ..state };
        assert_eq!(select_export_items(&unselected, None).len(), 2);
    }

    #[test]
    fn reorders_accounts_and_clamps_bounds() {
        let make = |id: u32, email: &str| Account {
            id,
            email: email.to_owned(),
            refresh_token: format!("1//{id}"),
            tier: None,
            models: Vec::new(),
            fetched_at: None,
            error: None,
            needs_login: false,
            access_token: None,
            expires_at: 0,
        };
        let mut accounts = vec![make(1, "a"), make(2, "b"), make(3, "c")];
        assert!(move_account_in_list(&mut accounts, 3, 0));
        assert_eq!(accounts.iter().map(|a| a.id).collect::<Vec<_>>(), vec![3, 1, 2]);
        assert!(move_account_in_list(&mut accounts, 3, 99));
        assert_eq!(accounts.iter().map(|a| a.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(!move_account_in_list(&mut accounts, 2, 1));
    }

    #[test]
    fn storage_roundtrip_uses_aead_and_rejects_plaintext() {
        let json = r#"{"accounts":[{"email":"secret@example.com","refresh_token":"1//secret"}]}"#;
        let encoded = encode_storage(json).expect("encoded");
        assert!(encoded.starts_with(ENCRYPTED_PREFIX));
        assert!(!encoded.contains("secret@example.com"));
        assert!(!encoded.contains("1//secret"));
        assert_eq!(decode_storage(&encoded), Some(json.to_owned()));
        assert_eq!(decode_storage(json), None);
    }

    #[test]
    fn selected_account_ids_and_batch_remove_preserve_order_and_clean_selection() {
        let make = |id: u32, email: &str| Account {
            id,
            email: email.to_owned(),
            refresh_token: format!("1//{id}"),
            tier: None,
            models: Vec::new(),
            fetched_at: None,
            error: None,
            needs_login: false,
            access_token: None,
            expires_at: 0,
        };
        let mut state = State {
            accounts: vec![make(1, "a@b.c"), make(2, "d@e.f"), make(3, "g@h.i")],
            next_id: 4,
            selected_ids: vec![3, 999, 1],
            refreshing_ids: vec![1, 2],
            ..State::default()
        };
        assert_eq!(filter_selected_account_ids(&state), vec![1, 3]);
        assert_eq!(remove_accounts_from_state(&mut state, &[1, 3]), 2);
        assert_eq!(state.accounts.iter().map(|a| a.id).collect::<Vec<_>>(), vec![2]);
        assert!(state.selected_ids.is_empty());
        assert_eq!(state.refreshing_ids, vec![2]);
    }

    #[test]
    fn deleting_last_used_account_clears_marker_case_insensitively() {
        let make = |id: u32, email: &str| Account {
            id,
            email: email.to_owned(),
            refresh_token: format!("1//{id}"),
            tier: None,
            models: Vec::new(),
            fetched_at: None,
            error: None,
            needs_login: false,
            access_token: None,
            expires_at: 0,
        };
        let mut state = State {
            accounts: vec![make(1, "a@b.c"), make(2, "User@Example.com")],
            next_id: 3,
            last_used_email: Some("user@example.com".to_owned()),
            ..State::default()
        };

        // Deleting an unrelated account preserves the marker.
        assert_eq!(remove_accounts_from_state(&mut state, &[1]), 1);
        assert_eq!(state.last_used_email.as_deref(), Some("user@example.com"));

        // Deleting the matching account clears the marker case-insensitively.
        assert_eq!(remove_accounts_from_state(&mut state, &[2]), 1);
        assert_eq!(state.last_used_email, None);
    }

    #[test]
    fn mark_last_used_account_moves_marker_persists_and_clears_on_delete() {
        use std::collections::BTreeMap;
        use std::sync::{Arc, Mutex};
        use nsh_core::element::Root;
        use nsh_core::storage::{StorageBackend, StorageChange};
        use nsh_core::{ApproxTextEngine, Core};

        struct MemBackend {
            entries: Mutex<BTreeMap<String, String>>,
        }
        impl MemBackend {
            fn new(initial: (String, String)) -> Arc<Self> {
                let mut map = BTreeMap::new();
                map.insert(initial.0, initial.1);
                Arc::new(Self {
                    entries: Mutex::new(map),
                })
            }
        }
        impl StorageBackend for MemBackend {
            fn load(&self) -> Result<BTreeMap<String, String>, String> {
                Ok(self.entries.lock().unwrap().clone())
            }
            fn write(&self, change: StorageChange) {
                let mut entries = self.entries.lock().unwrap();
                match change {
                    StorageChange::Set(k, v) => {
                        entries.insert(k, v);
                    }
                    StorageChange::Remove(k) => {
                        entries.remove(&k);
                    }
                    StorageChange::Clear => entries.clear(),
                }
            }
        }

        let initial = seed_storage_entry_with_last_used_for_test(
            vec![
                ("a@b.c", AccountTier::Ultra, Vec::new(), None),
                ("d@e.f", AccountTier::Pro, Vec::new(), None),
            ],
            ThemeMode::Light,
            Some("a@b.c"),
        );
        let backend = MemBackend::new(initial);

        #[component]
        fn StoreDriver() -> impl IntoElement {
            load();
            assert_eq!(last_used_email().as_deref(), Some("a@b.c"));
            assert!(account_card_view(1).unwrap().is_last_used);
            assert!(!account_card_view(2).unwrap().is_last_used);

            // Whitespace-only email is ignored.
            mark_last_used_account("   ");
            assert_eq!(last_used_email().as_deref(), Some("a@b.c"));

            // Launching a new account moves the marker and trims surrounding spaces.
            mark_last_used_account("  D@E.F  ");
            assert_eq!(last_used_email().as_deref(), Some("D@E.F"));
            assert!(!account_card_view(1).unwrap().is_last_used);
            assert!(account_card_view(2).unwrap().is_last_used);
            view()
        }

        let mut core = Core::new(ApproxTextEngine, size(px(640.0), px(480.0)));
        core.set_storage(Some(backend.clone()));
        core.mount_root(&Root::of::<StoreDriver>());
        let _ = core.commit();

        // Reload on a fresh surface from the persisted storage to verify restart durability + delete cleanup.
        #[component]
        fn ReloadDriver() -> impl IntoElement {
            load();
            assert_eq!(last_used_email().as_deref(), Some("D@E.F"));
            assert!(account_card_view(2).unwrap().is_last_used);

            remove_accounts(&[2]);
            assert_eq!(last_used_email(), None);
            assert!(!account_card_view(1).unwrap().is_last_used);
            view()
        }

        let mut core2 = Core::new(ApproxTextEngine, size(px(640.0), px(480.0)));
        core2.set_storage(Some(backend.clone()));
        core2.mount_root(&Root::of::<ReloadDriver>());
        let _ = core2.commit();

        // Confirm the cleared state was persisted back to storage.
        let persisted = backend
            .entries
            .lock()
            .unwrap()
            .get(STORAGE_KEY)
            .and_then(|raw| decode_storage(raw))
            .and_then(|json| serde_json::from_str::<Saved>(&json).ok())
            .expect("persisted saved state");
        assert_eq!(persisted.last_used_email, None);
    }

    #[test]
    fn merge_quota_models_preserves_unexpired_resets_and_drops_expired_on_refresh() {
        let now_ms = crate::format::parse_rfc3339("2026-11-25T10:00:00Z").expect("now");
        let prev = vec![
            ModelQuota {
                name: "Gemini".to_owned(),
                remaining: 0.84,
                reset: Some("2026-11-25T09:30:00Z".to_owned()), // expired
                weekly_remaining: Some(0.88),
                weekly_reset: Some("2026-12-02T00:00:00Z".to_owned()), // unexpired
                is_weekly_only: false,
            },
            ModelQuota {
                name: "Claude 4.6 / GPT".to_owned(),
                remaining: 0.90,
                reset: Some("2026-11-25T13:00:00Z".to_owned()), // unexpired
                weekly_remaining: Some(0.95),
                weekly_reset: Some("2026-12-01T18:00:00Z".to_owned()), // unexpired
                is_weekly_only: false,
            },
        ];

        // Refresh returns 100% with omitted resetTime fields and a renamed Claude label.
        let next = vec![
            ModelQuota {
                name: "Gemini".to_owned(),
                remaining: 1.0,
                reset: None,
                weekly_remaining: Some(1.0),
                weekly_reset: None,
                is_weekly_only: false,
            },
            ModelQuota {
                name: "Claude 5.5 / GPT".to_owned(),
                remaining: 1.0,
                reset: None,
                weekly_remaining: Some(1.0),
                weekly_reset: None,
                is_weekly_only: false,
            },
        ];

        let merged = merge_quota_models(&prev, next, now_ms);
        assert_eq!(merged[0].name, "Gemini");
        // Expired 5H reset must NOT be resurrected; unexpired weekly reset IS preserved.
        assert_eq!(merged[0].reset, None);
        assert_eq!(merged[0].weekly_reset.as_deref(), Some("2026-12-02T00:00:00Z"));

        // Renamed Claude bucket still matches prev via same_quota_bucket and keeps unexpired resets.
        assert_eq!(merged[1].name, "Claude 5.5 / GPT");
        assert_eq!(merged[1].reset.as_deref(), Some("2026-11-25T13:00:00Z"));
        assert_eq!(merged[1].weekly_reset.as_deref(), Some("2026-12-01T18:00:00Z"));

        // Fresh upstream reset timestamps always win over prev.
        let fresh = vec![ModelQuota {
            name: "Claude 5.5 / GPT".to_owned(),
            remaining: 0.80,
            reset: Some("2026-11-25T15:00:00Z".to_owned()),
            weekly_remaining: Some(0.90),
            weekly_reset: Some("2026-12-03T00:00:00Z".to_owned()),
            is_weekly_only: false,
        }];
        let merged_fresh = merge_quota_models(&prev, fresh, now_ms);
        assert_eq!(merged_fresh[0].reset.as_deref(), Some("2026-11-25T15:00:00Z"));
        assert_eq!(merged_fresh[0].weekly_reset.as_deref(), Some("2026-12-03T00:00:00Z"));
    }
}

fn encode_seed_storage_entry(
    accounts: Vec<(&str, AccountTier, Vec<ModelQuota>, Option<i64>)>,
    theme: ThemeMode,
    last_used_email: Option<&str>,
) -> (String, String) {
    let saved = Saved {
        accounts: accounts
            .into_iter()
            .enumerate()
            .map(|(idx, (email, tier, models, fetched_at))| SavedAccount {
                email: email.to_owned(),
                refresh_token: format!("1//demo_{idx}"),
                tier: Some(tier),
                models,
                fetched_at,
            })
            .collect(),
        proxy: SavedProxy::default(),
        theme,
        close_behavior: CloseBehavior::Ask,
        last_used_email: last_used_email.map(str::to_owned),
    };
    let json = serde_json::to_string(&saved).expect("saved json");
    let encoded = encode_storage(&json).expect("encoded");
    (STORAGE_KEY.to_owned(), encoded)
}

#[cfg(test)]
pub(crate) fn seed_storage_entry_for_test(
    accounts: Vec<(&str, AccountTier, Vec<ModelQuota>, Option<i64>)>,
    theme: ThemeMode,
) -> (String, String) {
    encode_seed_storage_entry(accounts, theme, None)
}

#[cfg(test)]
pub(crate) fn seed_storage_entry_with_last_used_for_test(
    accounts: Vec<(&str, AccountTier, Vec<ModelQuota>, Option<i64>)>,
    theme: ThemeMode,
    last_used_email: Option<&str>,
) -> (String, String) {
    encode_seed_storage_entry(accounts, theme, last_used_email)
}

pub(crate) fn seed_headless_storage_entry_for_test(
    accounts: &[(&str, &str)],
    last_used_email: Option<&str>,
) -> (String, String) {
    let now = unix_ms() as i64;
    let items = accounts
        .iter()
        .map(|(email, tier_name)| {
            let tier = match tier_name.trim().to_ascii_lowercase().as_str() {
                "ultra" => AccountTier::Ultra,
                "pro" => AccountTier::Pro,
                _ => AccountTier::Free,
            };
            let models = vec![ModelQuota {
                name: "Gemini".to_owned(),
                remaining: 0.9,
                reset: None,
                weekly_remaining: Some(0.95),
                weekly_reset: None,
                is_weekly_only: false,
            }];
            (*email, tier, models, Some(now))
        })
        .collect();
    encode_seed_storage_entry(items, ThemeMode::Light, last_used_email)
}
