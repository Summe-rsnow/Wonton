//! Starting Antigravity with a proxy that applies to its own process tree.
//!
//! The app is not one process: its requests come from `language_server`, which
//! is written in Go and reads only environment variables, so the system proxy
//! never reaches it. Environment variables are inherited at process creation,
//! so the launcher has to start the app itself.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

/// Supported Antigravity clients that Wonton can switch accounts for and launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTarget {
    Classic,
    Ide,
    Cli,
}

impl LaunchTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Antigravity",
            Self::Ide => "Antigravity IDE",
            Self::Cli => "Antigravity CLI",
        }
    }
}

/// Refreshed credentials injected into client stores before launching.
#[derive(Debug, Clone)]
pub struct SwitchCredentials {
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: i64,
}

/// Outcome of switching account credentials and launching a client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOutcome {
    Started { pid: u32 },
    CredentialsOnly,
}

/// Where the HTTP proxy lives and how to speak to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyConfig {
    /// Whether Wonton and launched clients route traffic through this proxy.
    pub enabled: bool,
    /// Kept as `"http"` for saved-state compatibility.
    pub protocol: String,
    pub host: String,
    pub port: u16,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self { enabled: true, protocol: "http".to_owned(), host: "127.0.0.1".to_owned(), port: 7890 }
    }
}

impl ProxyConfig {
    /// `http://127.0.0.1:7890`.
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Validates the proxy endpoint itself (used by the test button and when enabled).
    pub fn validate_endpoint(&self) -> Result<(), String> {
        if !self.protocol.is_empty() && self.protocol != "http" {
            return Err("仅支持 HTTP 代理".to_owned());
        }
        if self.host.trim().is_empty() {
            return Err("请填写主机地址".to_owned());
        }
        if self.host.contains(char::is_whitespace) {
            return Err("主机地址不能包含空格".to_owned());
        }
        if self.port == 0 {
            return Err("端口须在 1–65535 之间".to_owned());
        }
        Ok(())
    }

    /// Whether the proxy can be used as configured (direct mode always validates).
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        self.validate_endpoint()
    }
}

/// The variables the app and its child processes inherit. Every value is
/// written twice: Go reads only the variables, and several Node libraries
/// read only the lowercase spelling. Empty when the proxy is disabled.
pub fn env_vars(proxy: &ProxyConfig) -> Vec<(String, String)> {
    if !proxy.enabled {
        return Vec::new();
    }
    let http = proxy.url();
    let no_proxy = "localhost,127.0.0.1,::1";
    [
        ("HTTP_PROXY", http.as_str()),
        ("HTTPS_PROXY", http.as_str()),
        ("http_proxy", http.as_str()),
        ("https_proxy", http.as_str()),
        ("ALL_PROXY", http.as_str()),
        ("all_proxy", http.as_str()),
        ("NO_PROXY", no_proxy),
        ("no_proxy", no_proxy),
    ]
    .iter()
    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
    .collect()
}

/// The Chromium flags: the browser layer does not read the variables.
/// Empty when the proxy is disabled.
///
/// Loopback stays direct on purpose: the client's own UI is served on
/// 127.0.0.1, and routing it through the proxy leaves a blank window.
pub fn chromium_args(proxy: &ProxyConfig) -> Vec<String> {
    if !proxy.enabled {
        return Vec::new();
    }
    vec![
        format!("--proxy-server={}", proxy.url()),
        "--proxy-bypass-list=localhost;127.0.0.1;[::1]".to_owned(),
    ]
}

/// Known install locations for classic Antigravity.
pub fn detect_target() -> Option<PathBuf> {
    detect_target_for(LaunchTarget::Classic)
}

/// Resolves the executable path for `target` across standard Windows install directories and `PATH`.
pub fn detect_target_for(target: LaunchTarget) -> Option<PathBuf> {
    match target {
        LaunchTarget::Classic => find_gui_exe(&["Antigravity", "antigravity"], &["Antigravity.exe", "antigravity.exe"], false),
        LaunchTarget::Ide => find_gui_exe(
            &["Antigravity IDE", "antigravity-ide", "antigravity_ide"],
            &["Antigravity IDE.exe", "antigravity-ide.exe", "antigravity_ide.exe", "Antigravity.exe"],
            true,
        ),
        LaunchTarget::Cli => find_cli_exe(),
    }
}

fn is_ide_path(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_lowercase();
    [
        "antigravity ide",
        "antigravity-ide",
        "antigravity_ide",
        "antigravityide",
        "antigravity.ide",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn find_gui_exe(folders: &[&str], exes: &[&str], ide: bool) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        roots.push(local.join("Programs"));
        roots.push(local);
    }
    for env_key in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(val) = std::env::var_os(env_key) {
            roots.push(PathBuf::from(val));
        }
    }
    for root in &roots {
        for folder in folders {
            for exe in exes {
                let candidate = root.join(folder).join(exe);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    find_in_path(exes, |p| ide || !is_ide_path(p))
}

fn find_cli_exe() -> Option<PathBuf> {
    let mut direct = Vec::new();
    if let Some(home) = home_dir() {
        let bin = home.join(".local").join("bin");
        direct.push(bin.join("agy.exe"));
        direct.push(bin.join("agy.cmd"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let programs = PathBuf::from(local).join("Programs");
        direct.push(programs.join("agy").join("agy.exe"));
        direct.push(programs.join("antigravity-cli").join("agy.exe"));
    }
    if let Some(found) = direct.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }
    find_in_path(&["agy.exe", "agy.cmd", "agy"], |_| true)
}

fn find_in_path(names: &[&str], filter: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() && filter(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Starts Classic Antigravity with `proxy` without altering account credentials.
pub fn launch(proxy: &ProxyConfig) -> Result<u32, String> {
    proxy.validate()?;
    let target = detect_target_for(LaunchTarget::Classic)
        .ok_or_else(|| "没有找到 Antigravity.exe，请确认已安装或已改到默认目录".to_owned())?;
    spawn_gui(&target, proxy)
}

/// Writes `creds` for `target`, closes any stale GUI instance of `target`,
/// and launches it behind `proxy`.
pub fn switch_and_launch(
    target: LaunchTarget,
    proxy: &ProxyConfig,
    creds: &SwitchCredentials,
) -> Result<LaunchOutcome, String> {
    proxy.validate()?;
    match target {
        LaunchTarget::Classic | LaunchTarget::Ide => {
            let exe = detect_target_for(target).ok_or_else(|| match target {
                LaunchTarget::Classic => "没有找到 Antigravity.exe，请确认已安装".to_owned(),
                LaunchTarget::Ide => "没有找到 Antigravity IDE，请确认已安装".to_owned(),
                LaunchTarget::Cli => unreachable!(),
            })?;
            #[cfg(windows)]
            close_processes_matching_exe(&exe);

            #[cfg(windows)]
            write_windows_keyring(creds)?;
            let _ = write_file_credentials(creds, false);
            if let Some(db_path) =
                crate::localdb::find_db_for(target == LaunchTarget::Ide, Some(&exe))
            {
                let expiry_secs = creds.expires_at_ms / 1000;
                if let Err(err) = crate::localdb::inject_credentials(
                    &db_path,
                    &creds.email,
                    &creds.access_token,
                    &creds.refresh_token,
                    expiry_secs,
                ) {
                    log::warn!("注入本地数据库失败（已回退至系统凭据）：{err}");
                }
            }
            let pid = spawn_gui(&exe, proxy)?;
            Ok(LaunchOutcome::Started { pid })
        }
        LaunchTarget::Cli => {
            #[cfg(windows)]
            write_windows_keyring(creds)?;
            write_file_credentials(creds, true)?;
            if let Some(cli) = detect_target_for(LaunchTarget::Cli) {
                let pid = spawn_cli_console(&cli, proxy)?;
                Ok(LaunchOutcome::Started { pid })
            } else {
                Ok(LaunchOutcome::CredentialsOnly)
            }
        }
    }
}

fn spawn_gui(target: &Path, proxy: &ProxyConfig) -> Result<u32, String> {
    let directory = target.parent().map(PathBuf::from).unwrap_or_default();
    let mut command = Command::new(target);
    command
        .args(chromium_args(proxy))
        .envs(env_vars(proxy))
        .current_dir(directory);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Its own process group: closing this app does not close the client.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    command
        .spawn()
        .map(|child| child.id())
        .map_err(|e| format!("启动失败：{e}"))
}

fn spawn_cli_console(cli: &Path, proxy: &ProxyConfig) -> Result<u32, String> {
    let directory = home_dir().unwrap_or_else(|| cli.parent().map(PathBuf::from).unwrap_or_default());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let mut command = Command::new("cmd");
        command
            .arg("/k")
            .arg(cli)
            .envs(env_vars(proxy))
            .current_dir(directory)
            .creation_flags(CREATE_NEW_CONSOLE);
        command
            .spawn()
            .map(|child| child.id())
            .map_err(|e| format!("启动 CLI 失败：{e}"))
    }
    #[cfg(not(windows))]
    {
        Command::new(cli)
            .envs(env_vars(proxy))
            .current_dir(directory)
            .spawn()
            .map(|child| child.id())
            .map_err(|e| format!("启动 CLI 失败：{e}"))
    }
}

fn build_keyring_json(creds: &SwitchCredentials) -> Result<String, String> {
    #[derive(Serialize)]
    struct TokenDetails<'a> {
        access_token: &'a str,
        token_type: &'static str,
        refresh_token: &'a str,
        expiry: String,
        expires_in: i64,
    }
    #[derive(Serialize)]
    struct KeyringPayload<'a> {
        token: TokenDetails<'a>,
        auth_method: &'static str,
        id_token_email: &'a str,
    }
    let expiry_secs = if creds.expires_at_ms > 10_000_000_000 {
        creds.expires_at_ms / 1000
    } else {
        creds.expires_at_ms
    };
    let payload = KeyringPayload {
        token: TokenDetails {
            access_token: &creds.access_token,
            token_type: "Bearer",
            refresh_token: &creds.refresh_token,
            expiry: crate::format::rfc3339_utc_secs(expiry_secs),
            expires_in: 3599,
        },
        auth_method: "consumer",
        id_token_email: &creds.email,
    };
    serde_json::to_string(&payload).map_err(|e| format!("序列化凭据失败：{e}"))
}

fn write_file_credentials(creds: &SwitchCredentials, sync_agy_cli: bool) -> Result<(), String> {
    let home = home_dir().ok_or_else(|| "无法定位用户主目录".to_owned())?;
    write_file_credentials_to_dir(&home.join(".gemini"), creds, sync_agy_cli)
}

fn write_file_credentials_to_dir(
    gemini_dir: &Path,
    creds: &SwitchCredentials,
    sync_agy_cli: bool,
) -> Result<(), String> {
    std::fs::create_dir_all(gemini_dir)
        .map_err(|e| format!("创建 .gemini 目录失败：{e}"))?;

    let keyring_json = build_keyring_json(creds)?;
    let _ = std::fs::write(gemini_dir.join("jetski-standalone-oauth-token"), &keyring_json);

    if sync_agy_cli {
        let cli_dir = gemini_dir.join("antigravity-cli");
        if std::fs::create_dir_all(&cli_dir).is_ok() {
            let _ = std::fs::write(cli_dir.join("antigravity-oauth-token"), &keyring_json);
        }
    }

    #[derive(Serialize)]
    struct OAuthCreds<'a> {
        access_token: &'a str,
        refresh_token: &'a str,
        token_type: &'static str,
        expiry_date: i64,
        scope: &'static str,
    }
    let expiry_ms = if creds.expires_at_ms > 10_000_000_000 {
        creds.expires_at_ms
    } else {
        creds.expires_at_ms * 1000
    };
    let oauth_json = serde_json::to_string_pretty(&OAuthCreds {
        access_token: &creds.access_token,
        refresh_token: &creds.refresh_token,
        token_type: "Bearer",
        expiry_date: expiry_ms,
        scope: "https://www.googleapis.com/auth/userinfo.email openid https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.profile",
    })
    .map_err(|e| format!("序列化 oauth_creds 失败：{e}"))?;
    std::fs::write(gemini_dir.join("oauth_creds.json"), oauth_json)
        .map_err(|e| format!("写入 oauth_creds.json 失败：{e}"))?;

    #[derive(Serialize)]
    struct GoogleAccounts<'a> {
        active: &'a str,
        old: [&'a str; 0],
    }
    if let Ok(accounts_json) = serde_json::to_string_pretty(&GoogleAccounts {
        active: &creds.email,
        old: [],
    }) {
        let _ = std::fs::write(gemini_dir.join("google_accounts.json"), accounts_json);
    }
    Ok(())
}

#[cfg(windows)]
fn write_windows_keyring(creds: &SwitchCredentials) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[repr(C)]
    struct FILETIME {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    #[repr(C)]
    struct CREDENTIALW {
        flags: u32,
        cred_type: u32,
        target_name: *const u16,
        comment: *const u16,
        last_written: FILETIME,
        credential_blob_size: u32,
        credential_blob: *const u8,
        persist: u32,
        attribute_count: u32,
        attributes: *const std::ffi::c_void,
        target_alias: *const u16,
        user_name: *const u16,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const CREDENTIALW, flags: u32) -> i32;
        fn CredDeleteW(target_name: *const u16, type_: u32, flags: u32) -> i32;
    }

    let json = build_keyring_json(creds)?;
    let target_wide: Vec<u16> = std::ffi::OsStr::new("gemini:antigravity")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let user_wide: Vec<u16> = std::ffi::OsStr::new("antigravity")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let secret = json.as_bytes();

    let cred = CREDENTIALW {
        flags: 0,
        cred_type: 1,
        target_name: target_wide.as_ptr(),
        comment: ptr::null(),
        last_written: FILETIME {
            dw_low_date_time: 0,
            dw_high_date_time: 0,
        },
        credential_blob_size: secret.len() as u32,
        credential_blob: secret.as_ptr(),
        persist: 2,
        attribute_count: 0,
        attributes: ptr::null(),
        target_alias: ptr::null(),
        user_name: user_wide.as_ptr(),
    };

    unsafe {
        let _ = CredDeleteW(target_wide.as_ptr(), 1, 0);
        if CredWriteW(&cred, 0) == 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("写入系统凭据管理器失败：{err}"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn close_processes_matching_exe(target_exe: &Path) {
    use std::os::windows::process::CommandExt;

    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const INVALID_HANDLE: isize = -1;

    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
        fn Process32FirstW(snapshot: isize, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: isize, entry: *mut ProcessEntry32W) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn QueryFullProcessImageNameW(
            process: isize,
            flags: u32,
            exe_name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }

    let Some(target_file) = target_exe.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let canonical_target = target_exe
        .canonicalize()
        .unwrap_or_else(|_| target_exe.to_path_buf())
        .to_string_lossy()
        .to_lowercase();
    let self_pid = std::process::id();
    let mut matching_pids = Vec::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == 0 || snapshot == INVALID_HANDLE {
            return;
        }
        let mut entry = ProcessEntry32W {
            dw_size: std::mem::size_of::<ProcessEntry32W>() as u32,
            cnt_usage: 0,
            th32_process_id: 0,
            th32_default_heap_id: 0,
            th32_module_id: 0,
            cnt_threads: 0,
            th32_parent_process_id: 0,
            pc_pri_class_base: 0,
            dw_flags: 0,
            sz_exe_file: [0; 260],
        };
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let pid = entry.th32_process_id;
                if pid != 0 && pid != self_pid {
                    let end = entry.sz_exe_file.iter().position(|&c| c == 0).unwrap_or(260);
                    let exe_name = String::from_utf16_lossy(&entry.sz_exe_file[..end]);
                    if exe_name.eq_ignore_ascii_case(target_file) {
                        let proc_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                        if proc_handle != 0 {
                            let mut buf = [0u16; 1024];
                            let mut len = buf.len() as u32;
                            if QueryFullProcessImageNameW(proc_handle, 0, buf.as_mut_ptr(), &mut len)
                                != 0
                            {
                                let full_path =
                                    String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
                                let normalized = PathBuf::from(&full_path)
                                    .canonicalize()
                                    .map(|p| p.to_string_lossy().to_lowercase())
                                    .unwrap_or(full_path);
                                if normalized == canonical_target {
                                    matching_pids.push(pid);
                                }
                            }
                            let _ = CloseHandle(proc_handle);
                        }
                    }
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    if matching_pids.is_empty() {
        return;
    }
    for pid in matching_pids {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_url_and_the_variables() {
        let proxy = ProxyConfig {
            enabled: true,
            protocol: "http".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 7890,
        };
        assert_eq!(proxy.url(), "http://127.0.0.1:7890");
        let vars = env_vars(&proxy);
        let get = |name: &str| vars.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
        assert_eq!(get("HTTPS_PROXY"), Some("http://127.0.0.1:7890"));
        assert_eq!(get("all_proxy"), Some("http://127.0.0.1:7890"));
        assert_eq!(get("NO_PROXY"), Some("localhost,127.0.0.1,::1"));
        assert_eq!(vars.len(), 8);

        let direct = ProxyConfig { enabled: false, ..proxy };
        assert!(env_vars(&direct).is_empty());
        assert!(chromium_args(&direct).is_empty());
    }

    #[test]
    fn keeps_loopback_out_of_the_proxy() {
        let args = chromium_args(&ProxyConfig::default());
        assert_eq!(args[0], "--proxy-server=http://127.0.0.1:7890");
        assert!(args[1].contains("127.0.0.1"));
    }

    #[test]
    fn rejects_an_unusable_proxy() {
        let mut proxy = ProxyConfig { host: "  ".to_owned(), ..ProxyConfig::default() };
        assert!(proxy.validate().is_err());
        proxy.enabled = false;
        assert!(proxy.validate().is_ok());
        assert!(proxy.validate_endpoint().is_err());
        proxy.enabled = true;
        proxy.host = "127.0.0.1".to_owned();
        proxy.protocol = "socks5".to_owned();
        assert!(proxy.validate().is_err());
        proxy.protocol = "http".to_owned();
        proxy.port = 0;
        assert!(proxy.validate().is_err());
        proxy.port = 7890;
        assert!(proxy.validate().is_ok());
    }

    #[test]
    fn writes_credentials_with_cli_file_isolation() {
        let dir = std::env::temp_dir().join(format!("wonton_creds_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let creds = SwitchCredentials {
            email: "user@gmail.com".to_owned(),
            access_token: "ya29.test".to_owned(),
            refresh_token: "1//0gTEST".to_owned(),
            expires_at_ms: 1_800_000_000_000,
        };

        write_file_credentials_to_dir(&dir, &creds, false).expect("classic creds");
        assert!(dir.join("jetski-standalone-oauth-token").is_file());
        assert!(dir.join("oauth_creds.json").is_file());
        assert!(dir.join("google_accounts.json").is_file());
        assert!(!dir.join("antigravity-cli").join("antigravity-oauth-token").exists());

        write_file_credentials_to_dir(&dir, &creds, true).expect("cli creds");
        let cli_raw =
            std::fs::read_to_string(dir.join("antigravity-cli").join("antigravity-oauth-token"))
                .expect("cli token");
        assert!(cli_raw.contains("\"id_token_email\":\"user@gmail.com\""));
        assert!(cli_raw.contains("\"refresh_token\":\"1//0gTEST\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_ide_paths_without_false_positives_on_classic() {
        assert!(is_ide_path(Path::new(r"C:\Users\u\AppData\Local\Programs\Antigravity IDE\Antigravity IDE.exe")));
        assert!(is_ide_path(Path::new(r"/usr/bin/antigravity-ide")));
        assert!(!is_ide_path(Path::new(r"C:\Users\u\AppData\Local\Programs\Antigravity\Antigravity.exe")));
    }
}
