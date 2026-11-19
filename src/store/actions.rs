//! What the buttons do: the network work behind adding, refreshing and
//! starting. Every flow reports through the notice line and keeps the
//! account list usable while it runs.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::rc::Rc;
use std::time::Duration;

use nsh_core::file_dialog::{FileFilter, OpenDialog, SaveDialog};
use nsh_core::prelude::*;
use nsh_core::time::unix_ms;

use super::{
    account_by_id, account_count, account_ids, busy, clear_notice, clear_selection, export_items,
    insert_account, preferred_kind_for, proxy, proxy_testing, refresh_account, set_busy,
    set_import_progress, set_notice, set_proxy_testing, set_switching_account_id, set_token,
    switching_account_id, ImportProgress, Tone,
};
use crate::google::{self, ClientKind, ImportEntry};
use crate::launch;

/// Refreshes every account's quota, one after another: one notice at the end.
pub(crate) fn refresh_all() {
    if account_count() == 0 {
        set_notice("还没有账号", Tone::Info);
        return;
    }
    set_busy(true);
    spawn(async move {
        let ids = account_ids();
        let mut failed = 0;
        for id in ids {
            refresh_account(id).await;
            if account_by_id(id).is_some_and(|account| account.error.is_some()) {
                failed += 1;
            }
        }
        set_busy(false);
        if failed == 0 {
            set_notice("额度已刷新", Tone::Ok);
        } else {
            set_notice(format!("{failed} 个账号刷新失败"), Tone::Bad);
        }
    });
}

/// Adds every refresh token in `text`, with the client that issued them.
/// Failures are counted, not fatal: one bad token does not stop the rest.
pub(crate) fn add_tokens(text: String, kind: ClientKind) {
    let tokens = google::parse_tokens(&text);
    if tokens.is_empty() {
        set_notice("没有识别到 refresh_token", Tone::Bad);
        return;
    }
    set_busy(true);
    spawn(async move {
        let proxy = proxy();
        let mut added = 0;
        let mut failed = Vec::new();
        for token in tokens {
            match google::refresh(&token, kind, proxy.clone()).await {
                Ok(tokens) => match google::email(&tokens.access_token, proxy.clone()).await {
                    Ok(email) => {
                        let id = insert_account(&email, &token, kind);
                        set_token(id, tokens);
                        refresh_account(id).await;
                        added += 1;
                    }
                    Err(error) => failed.push(error),
                },
                Err(error) => failed.push(error),
            }
        }
        set_busy(false);
        if failed.is_empty() {
            set_notice(format!("已添加 {added} 个账号"), Tone::Ok);
        } else {
            set_notice(
                format!("已添加 {added} 个，{} 个失败：{}", failed.len(), failed[0]),
                Tone::Bad,
            );
        }
    });
}

/// Opens the browser, waits for Google's redirect on a loopback port, and
/// adds the account it belongs to.
pub(crate) fn add_account_by_oauth(kind: ClientKind) {
    set_busy(true);
    set_notice("等待浏览器授权…", Tone::Info);
    spawn(async move {
        match login(kind).await {
            Ok((email, id)) => {
                set_busy(false);
                set_notice(format!("已添加 {email}"), Tone::Ok);
                refresh_account(id).await;
            }
            Err(error) => {
                set_busy(false);
                set_notice(format!("授权失败：{error}"), Tone::Bad);
            }
        }
    });
}

/// The whole browser flow: listen, open, wait, exchange, read the e-mail.
async fn login(kind: ClientKind) -> Result<(String, u32), String> {
    let proxy = proxy();
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("无法监听本地端口：{e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth-callback");
    let state = format!("{:x}{:x}", unix_ms() as u64, std::process::id());
    let url = google::auth_url(kind, &redirect_uri, &state);
    let _ = open_url(url).await;

    let expected = state.clone();
    let code = blocking(move || wait_for_code(listener, &expected))
        .await
        .map_err(|e| e.to_string())??;
    let tokens = google::exchange_code(&code, &redirect_uri, kind, proxy.clone()).await?;
    let email = google::email(&tokens.access_token, proxy).await?;
    let refresh_token = tokens.refresh_token.clone().ok_or("Google 没有返回 refresh_token")?;
    let id = insert_account(&email, &refresh_token, kind);
    set_token(id, tokens);
    Ok((email, id))
}

/// Serves the one callback request and returns its `code`. Runs on a
/// blocking thread, so the wait does not stall the UI.
fn wait_for_code(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("无法设置监听超时：{e}"))?;
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(300) {
            return Err("等待授权超时".to_owned());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let line = request.lines().next().unwrap_or("").to_owned();
                let page = |message: &str| {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                         Connection: close\r\n\r\n<!doctype html><meta charset=\"utf-8\">\
                         <body style=\"font-family:system-ui;padding:40px\"><p>{message}</p>\
                         <p>可以关闭此页面。</p></body>"
                    )
                };
                let Some((code, state)) = google::callback_query(&line) else {
                    let _ = stream.write_all(page("已忽略这次请求。").as_bytes());
                    continue;
                };
                if state.as_deref() != Some(expected_state) {
                    let _ = stream.write_all(page("state 不匹配，已拒绝。").as_bytes());
                    return Err("state 不匹配".to_owned());
                }
                let Some(code) = code else {
                    let _ = stream.write_all(page("授权被取消。").as_bytes());
                    return Err("授权被取消".to_owned());
                };
                let _ = stream.write_all(page("授权成功。").as_bytes());
                return Ok(code);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(error) => return Err(format!("接收回调失败：{error}")),
        }
    }
}

/// Imports the account already signed in on this machine, from the app's
/// own database.
pub(crate) fn import_local() {
    set_busy(true);
    set_notice("正在读取本机数据库…", Tone::Info);
    spawn(async move {
        let read = match blocking(crate::localdb::read_local_token).await {
            Ok(result) => result,
            Err(error) => Err(error.to_string()),
        };
        let token = match read {
            Ok(token) => token,
            Err(error) => {
                set_busy(false);
                set_notice(format!("导入失败：{error}"), Tone::Bad);
                return;
            }
        };
        let proxy = proxy();
        match google::refresh_with_fallback(&token, Some(ClientKind::Personal), proxy.clone()).await {
            Ok((tokens, kind)) => match google::email(&tokens.access_token, proxy).await {
                Ok(email) => {
                    let id = insert_account(&email, &token, kind);
                    set_token(id, tokens);
                    set_busy(false);
                    set_notice(format!("已导入 {email}"), Tone::Ok);
                    refresh_account(id).await;
                }
                Err(error) => {
                    set_busy(false);
                    set_notice(format!("导入失败：{error}"), Tone::Bad);
                }
            },
            Err(error) => {
                set_busy(false);
                set_notice(format!("导入失败：{error}"), Tone::Bad);
            }
        }
    });
}

/// Tests whether the configured HTTP proxy reaches Google cleanly and reports latency.
pub(crate) fn test_proxy_action(config: launch::ProxyConfig) {
    if proxy_testing() {
        return;
    }
    if let Err(error) = config.validate_endpoint() {
        set_notice(error, Tone::Bad);
        return;
    }
    set_proxy_testing(true);
    spawn(async move {
        let result = google::test_proxy(config).await;
        set_proxy_testing(false);
        match result {
            Ok(ms) => set_notice(format!("代理可用 · 延迟 {ms}ms"), Tone::Ok),
            Err(error) => set_notice(format!("代理测试失败：{error}"), Tone::Bad),
        }
    });
}

/// Starts Antigravity behind the saved proxy (or directly if disabled) without switching credentials.
pub(crate) fn launch_app() {
    clear_notice();
    let proxy = proxy();
    match launch::launch(&proxy) {
        Ok(pid) if proxy.enabled => set_notice(format!("已启动（pid {pid}），代理 {}", proxy.url()), Tone::Ok),
        Ok(pid) => set_notice(format!("已启动（pid {pid}），直连"), Tone::Ok),
        Err(error) => set_notice(error, Tone::Bad),
    }
}

/// Refreshes credentials if needed, injects them for `target`, and launches it behind the proxy.
pub(crate) fn launch_account_target(id: u32, target: launch::LaunchTarget) {
    if switching_account_id().is_some() {
        return;
    }
    let Some(account) = account_by_id(id) else {
        return;
    };
    let proxy_cfg = proxy();
    if let Err(error) = proxy_cfg.validate() {
        set_notice(error, Tone::Bad);
        return;
    }
    set_switching_account_id(Some(id));
    set_notice(
        format!("正在切换 {} 至 {}…", account.email, target.label()),
        Tone::Info,
    );
    spawn(async move {
        let access = match super::access_token(&account).await {
            Ok(token) => token,
            Err(error) => {
                set_switching_account_id(None);
                set_notice(format!("凭据续期失败：{error}"), Tone::Bad);
                return;
            }
        };
        let refreshed = account_by_id(id);
        let expires_at_ms = refreshed
            .as_ref()
            .map(|a| a.expires_at)
            .filter(|&exp| exp > 0)
            .unwrap_or_else(|| unix_ms() as i64 + 3_600_000);
        let creds = launch::SwitchCredentials {
            email: account.email.clone(),
            access_token: access,
            refresh_token: account.refresh_token.clone(),
            expires_at_ms,
        };
        let proxy_for_blocking = proxy_cfg.clone();
        let result = blocking(move || launch::switch_and_launch(target, &proxy_for_blocking, &creds))
            .await
            .map_err(|e| e.to_string())
            .and_then(|res| res);
        set_switching_account_id(None);
        match result {
            Ok(launch::LaunchOutcome::Started { pid }) => {
                let route = if proxy_cfg.enabled {
                    format!("代理 {}", proxy_cfg.url())
                } else {
                    "直连".to_owned()
                };
                set_notice(
                    format!("已切换 {} 并启动 {}（pid {pid}），{route}", account.email, target.label()),
                    Tone::Ok,
                );
            }
            Ok(launch::LaunchOutcome::CredentialsOnly) => set_notice(
                format!(
                    "已写入 {} 的 {} 凭据（未检测到 agy.exe，可在终端运行 agy）",
                    account.email,
                    target.label()
                ),
                Tone::Ok,
            ),
            Err(error) => set_notice(error, Tone::Bad),
        }
    });
}

/// Refreshes one account now, from its own row.
pub(crate) fn refresh_account_now(id: u32) {
    spawn(async move {
        super::refresh_account(id).await;
    });
}

/// Exports either a single account, selected accounts, or all accounts as JSON.
pub(crate) fn export_accounts_action(single_id: Option<u32>) {
    let items = export_items(single_id);
    if items.is_empty() {
        set_notice("没有可导出的账号", Tone::Info);
        return;
    }
    let json = match google::format_export_json(&items) {
        Ok(json) => json,
        Err(error) => {
            set_notice(error, Tone::Bad);
            return;
        }
    };
    let count = items.len();
    let default_name = crate::format::default_export_filename(unix_ms() as i64);
    spawn(async move {
        let dialog = SaveDialog::new()
            .file_name(&default_name)
            .filter(FileFilter::new("JSON", ["json"]))
            .title("导出账号");
        let path = match pick_save_path(dialog).await {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                set_notice(format!("打开保存对话框失败：{error}"), Tone::Bad);
                return;
            }
        };
        let write_path = path.clone();
        let written = blocking(move || std::fs::write(&write_path, json))
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| format!("写入文件失败：{error}")));
        match written {
            Ok(()) => {
                if single_id.is_none() {
                    clear_selection();
                }
                set_notice(
                    format!("已导出 {count} 个账号至 {}", path.display()),
                    Tone::Ok,
                );
            }
            Err(error) => set_notice(error, Tone::Bad),
        }
    });
}

/// Imports an Antigravity-Manager JSON account export via a file open dialog,
/// refreshing tokens concurrently with Personal ↔ Enterprise fallback.
pub(crate) fn import_accounts_action() {
    if busy() {
        return;
    }
    set_busy(true);
    spawn(async move {
        let dialog = OpenDialog::files()
            .filter(FileFilter::new("JSON", ["json"]))
            .title("导入账号");
        let path = match pick_paths(dialog).await {
            Ok(Some(paths)) => paths.into_iter().next(),
            Ok(None) => None,
            Err(error) => {
                set_busy(false);
                set_notice(format!("打开文件选择器失败：{error}"), Tone::Bad);
                return;
            }
        };
        let Some(path) = path else {
            set_busy(false);
            return;
        };

        let read_path = path.clone();
        let contents = blocking(move || std::fs::read_to_string(&read_path))
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| format!("读取文件失败：{error}")));
        let raw = match contents {
            Ok(raw) => raw,
            Err(error) => {
                set_busy(false);
                set_notice(error, Tone::Bad);
                return;
            }
        };

        let entries = match google::parse_import_entries(&raw) {
            Ok(entries) => entries,
            Err(error) => {
                set_busy(false);
                set_notice(error, Tone::Bad);
                return;
            }
        };

        run_batch_import(entries);
    });
}

struct ImportJob {
    queue: VecDeque<ImportEntry>,
    proxy: launch::ProxyConfig,
    total: usize,
    completed: usize,
    success: usize,
    first_error: Option<String>,
    remaining_workers: usize,
}

fn update_import_progress_ui(job: &ImportJob, email: impl Into<String>) {
    set_import_progress(Some(ImportProgress {
        current: job.completed,
        total: job.total,
        success: job.success,
        failed: job.completed.saturating_sub(job.success),
        current_email: email.into(),
    }));
}

fn run_batch_import(entries: Vec<ImportEntry>) {
    let total = entries.len();
    let worker_count = total.min(6);
    let initial_email = entries
        .first()
        .and_then(|entry| entry.email.clone())
        .unwrap_or_else(|| "准备导入…".to_owned());

    let job = Rc::new(RefCell::new(ImportJob {
        queue: VecDeque::from(entries),
        proxy: proxy(),
        total,
        completed: 0,
        success: 0,
        first_error: None,
        remaining_workers: worker_count,
    }));
    update_import_progress_ui(&job.borrow(), initial_email);

    for worker_index in 0..worker_count {
        let job = Rc::clone(&job);
        spawn(async move {
            if worker_index > 0 {
                timer(Duration::from_millis(worker_index as u64 * 45)).await;
            }
            loop {
                let (entry, proxy_cfg) = {
                    let mut state = job.borrow_mut();
                    let Some(entry) = state.queue.pop_front() else {
                        break;
                    };
                    let active_label = entry
                        .email
                        .clone()
                        .unwrap_or_else(|| "正在验证 refresh_token…".to_owned());
                    update_import_progress_ui(&state, active_label);
                    (entry, state.proxy.clone())
                };

                let result = import_one_entry(entry, proxy_cfg).await;

                let mut state = job.borrow_mut();
                state.completed += 1;
                let label = match result {
                    Ok(email) => {
                        state.success += 1;
                        email
                    }
                    Err(error) => {
                        if state.first_error.is_none() {
                            state.first_error = Some(error);
                        }
                        "导入失败".to_owned()
                    }
                };
                update_import_progress_ui(&state, label);
            }

            let mut state = job.borrow_mut();
            state.remaining_workers -= 1;
            if state.remaining_workers == 0 {
                let success = state.success;
                let failed = state.total.saturating_sub(success);
                let first_error = state.first_error.take().unwrap_or_default();
                drop(state);

                set_import_progress(None);
                set_busy(false);
                if failed == 0 {
                    set_notice(format!("已导入 {success} 个账号"), Tone::Ok);
                } else if success > 0 {
                    set_notice(
                        format!("导入完成：{success} 个成功，{failed} 个失败：{first_error}"),
                        Tone::Bad,
                    );
                } else {
                    set_notice(format!("导入失败：{first_error}"), Tone::Bad);
                }
            }
        });
    }
}

async fn import_one_entry(
    entry: ImportEntry,
    proxy_cfg: launch::ProxyConfig,
) -> Result<String, String> {
    let preferred = preferred_kind_for(&entry);
    let (tokens, kind) =
        google::refresh_with_fallback(&entry.refresh_token, preferred, proxy_cfg.clone()).await?;
    let email = google::email(&tokens.access_token, proxy_cfg).await?;
    let id = insert_account(&email, &entry.refresh_token, kind);
    set_token(id, tokens);
    refresh_account(id).await;
    Ok(email)
}
