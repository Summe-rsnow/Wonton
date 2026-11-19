//! Reads the refresh token the installed Antigravity app keeps in its
//! `state.vscdb` (key `antigravityUnifiedStateSync.oauthToken`).
//!
//! The value is base64 of a protobuf: Topic(F1 data entries) →
//! DataEntry(F1 key, F2 row) → Row(F1 base64 payload) → OAuthInfo(F3 refresh
//! token). Ported from Antigravity-Manager's `migration.rs` and
//! `utils/protobuf.rs`.

use std::path::{Path, PathBuf};

use base64::Engine as _;

const NEW_KEY: &str = "antigravityUnifiedStateSync.oauthToken";
const NEW_SENTINEL: &str = "oauthTokenInfoSentinelKey";
const USER_STATUS_KEY: &str = "antigravityUnifiedStateSync.userStatus";
const USER_STATUS_SENTINEL: &str = "userStatusSentinelKey";

/// Where the desktop app keeps its state, most likely first.
pub fn default_db_paths() -> Vec<PathBuf> {
    candidate_db_paths_for(false, None)
}

/// Candidate `state.vscdb` paths for Classic (`ide = false`) or Antigravity IDE (`ide = true`).
pub fn candidate_db_paths_for(ide: bool, exe_path: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(parent) = exe_path.and_then(Path::parent) {
        paths.push(
            parent
                .join("data")
                .join("user-data")
                .join("User")
                .join("globalStorage")
                .join("state.vscdb"),
        );
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        let folders: &[&str] = if ide {
            &["Antigravity IDE", "antigravity-ide", "antigravity_ide"]
        } else {
            &["Antigravity"]
        };
        for folder in folders {
            paths.push(
                local
                    .join("Programs")
                    .join(folder)
                    .join("data")
                    .join("user-data")
                    .join("User")
                    .join("globalStorage")
                    .join("state.vscdb"),
            );
        }
    }
    if let Some(roaming) = std::env::var_os("APPDATA") {
        let roaming = PathBuf::from(roaming);
        let folders: &[&str] = if ide {
            &["Antigravity IDE", "antigravity-ide", "antigravity_ide"]
        } else {
            &["Antigravity"]
        };
        for folder in folders {
            paths.push(roaming.join(folder).join("User").join("globalStorage").join("state.vscdb"));
        }
    }
    paths
}

/// The first existing database.
pub fn find_db() -> Option<PathBuf> {
    default_db_paths().into_iter().find(|path| path.is_file())
}

/// The first existing database for Classic (`ide = false`) or Antigravity IDE (`ide = true`).
pub fn find_db_for(ide: bool, exe_path: Option<&Path>) -> Option<PathBuf> {
    candidate_db_paths_for(ide, exe_path)
        .into_iter()
        .find(|path| path.is_file())
}

fn read_varint(data: &[u8], mut at: usize) -> Result<(u64, usize), String> {
    let mut value = 0_u64;
    let mut shift = 0;
    loop {
        let Some(&byte) = data.get(at) else {
            return Err("protobuf 数据在 varint 中断".to_owned());
        };
        at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, at));
        }
        shift += 7;
        if shift > 63 {
            return Err("protobuf varint 过长".to_owned());
        }
    }
}

/// Every length-delimited payload of field `target` at this level.
fn collect_fields(data: &[u8], target: u32) -> Result<Vec<Vec<u8>>, String> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let (tag, next) = read_varint(data, at)?;
        at = next;
        let field = (tag >> 3) as u32;
        match tag & 7 {
            0 => {
                let (_, next) = read_varint(data, at)?;
                at = next;
            }
            1 => at += 8,
            2 => {
                let (length, next) = read_varint(data, at)?;
                at = next;
                let end = at + length as usize;
                if end > data.len() {
                    return Err("protobuf 字段长度越界".to_owned());
                }
                if field == target {
                    found.push(data[at..end].to_vec());
                }
                at = end;
            }
            5 => at += 4,
            wire => return Err(format!("未知的 protobuf wire type {wire}")),
        }
    }
    Ok(found)
}

fn find_field(data: &[u8], target: u32) -> Result<Option<Vec<u8>>, String> {
    Ok(collect_fields(data, target)?.into_iter().next())
}

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
    out
}

fn encode_len_delim_field(field: u32, data: &[u8]) -> Vec<u8> {
    let mut out = encode_varint(((field as u64) << 3) | 2);
    out.extend(encode_varint(data.len() as u64));
    out.extend_from_slice(data);
    out
}

fn encode_string_field(field: u32, value: &str) -> Vec<u8> {
    encode_len_delim_field(field, value.as_bytes())
}

fn encode_oauth_info(access_token: &str, refresh_token: &str, expiry_secs: i64) -> Vec<u8> {
    let mut ts = encode_varint((1 << 3) | 0);
    ts.extend(encode_varint(expiry_secs.max(0) as u64));
    ts.extend(encode_varint((2 << 3) | 0));
    ts.push(0);

    let mut msg = encode_string_field(1, access_token);
    msg.extend(encode_string_field(2, "Bearer"));
    msg.extend(encode_string_field(3, refresh_token));
    msg.extend(encode_len_delim_field(4, &ts));
    msg
}

fn encode_unified_topic_entry(sentinel_key: &str, payload: &[u8]) -> Vec<u8> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
    let row = encode_string_field(1, &encoded);
    let mut entry = encode_string_field(1, sentinel_key);
    entry.extend(encode_len_delim_field(2, &row));
    encode_len_delim_field(1, &entry)
}

fn remove_unified_topic_entry(topic_blob: &[u8], target_key: &str) -> Result<Vec<u8>, String> {
    let mut kept = Vec::new();
    for entry in collect_fields(topic_blob, 1)? {
        let is_target = find_field(&entry, 1)?
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .is_some_and(|key| key == target_key);
        if !is_target {
            kept.extend(encode_len_delim_field(1, &entry));
        }
    }
    Ok(kept)
}

/// Injects `access_token`, `refresh_token` and `email` into Antigravity's `state.vscdb`.
#[cfg(windows)]
pub fn inject_credentials(
    db_path: &Path,
    email: &str,
    access_token: &str,
    refresh_token: &str,
    expiry_secs: i64,
) -> Result<(), String> {
    if !db_path.is_file() {
        return Err(format!("数据库不存在：{}", db_path.display()));
    }
    let backup_path = db_path.with_extension("vscdb.backup");
    let _ = std::fs::copy(db_path, &backup_path);

    let connection =
        rusqlite::Connection::open(db_path).map_err(|e| format!("打开数据库失败：{e}"))?;
    connection
        .busy_timeout(std::time::Duration::from_millis(2000))
        .map_err(|e| format!("设置数据库忙等待失败：{e}"))?;

    let existing_b64: Option<String> = match connection.query_row(
        "SELECT value FROM ItemTable WHERE key = ?",
        [NEW_KEY],
        |row| row.get(0),
    ) {
        Ok(val) => Some(val),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => return Err(format!("读取 oauthToken 记录失败：{error}")),
    };
    let existing_blob = existing_b64
        .as_deref()
        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok())
        .unwrap_or_default();

    let oauth_info = encode_oauth_info(access_token, refresh_token, expiry_secs);
    let mut topic = remove_unified_topic_entry(&existing_blob, NEW_SENTINEL).unwrap_or_default();
    topic.extend(encode_unified_topic_entry(NEW_SENTINEL, &oauth_info));
    let topic_b64 = base64::engine::general_purpose::STANDARD.encode(&topic);

    connection
        .execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
            [NEW_KEY, &topic_b64],
        )
        .map_err(|e| format!("写入 oauthToken 失败：{e}"))?;

    let mut status_payload = encode_string_field(3, email);
    status_payload.extend(encode_string_field(7, email));
    let status_b64 = base64::engine::general_purpose::STANDARD
        .encode(encode_unified_topic_entry(USER_STATUS_SENTINEL, &status_payload));
    connection
        .execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
            [USER_STATUS_KEY, &status_b64],
        )
        .map_err(|e| format!("写入 userStatus 失败：{e}"))?;

    connection
        .execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
            ["antigravityOnboarding", "true"],
        )
        .map_err(|e| format!("写入 onboarding 状态失败：{e}"))?;

    let _ = connection.execute(
        "DELETE FROM ItemTable WHERE key = ?",
        ["jetskiStateSync.agentManagerInitState"],
    );
    Ok(())
}

#[cfg(not(windows))]
pub fn inject_credentials(
    _db_path: &Path,
    _email: &str,
    _access_token: &str,
    _refresh_token: &str,
    _expiry_secs: i64,
) -> Result<(), String> {
    Err("数据库凭据注入目前仅支持 Windows".to_owned())
}

/// The sentinel key and the decoded OAuthInfo payload of one unified-state entry.
pub fn decode_unified_entry(outer_b64: &str, preferred: Option<&str>) -> Result<(String, Vec<u8>), String> {
    let outer = base64::engine::general_purpose::STANDARD
        .decode(outer_b64.trim())
        .map_err(|e| format!("外层 base64 解码失败：{e}"))?;
    let entries = collect_fields(&outer, 1)?;
    if entries.is_empty() {
        return Err("找不到 topic 数据条目".to_owned());
    }
    let mut first: Option<(String, Vec<u8>)> = None;
    for entry in entries {
        let (Some(key), Some(row)) = (find_field(&entry, 1)?, find_field(&entry, 2)?) else {
            continue;
        };
        let Some(payload_b64) = find_field(&row, 1)? else {
            continue;
        };
        let Ok(sentinel) = String::from_utf8(key) else {
            continue;
        };
        let Ok(payload_b64) = String::from_utf8(payload_b64) else {
            continue;
        };
        let payload = base64::engine::general_purpose::STANDARD
            .decode(payload_b64.trim())
            .unwrap_or_else(|_| payload_b64.into_bytes());
        if preferred == Some(sentinel.as_str()) {
            return Ok((sentinel, payload));
        }
        first.get_or_insert((sentinel, payload));
    }
    first.ok_or_else(|| "统一状态条目里没有可用的数据".to_owned())
}

/// The whole path: database → OAuth entry → refresh token.
#[cfg(windows)]
pub fn read_refresh_token(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!("数据库不存在：{}", path.display()));
    }
    let connection = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("打开数据库失败：{e}"))?;
    let value: String = connection
        .query_row("SELECT value FROM ItemTable WHERE key = ?", [NEW_KEY], |row| row.get(0))
        .map_err(|_| "未找到 oauthToken 记录（该数据库可能来自旧版，或尚未登录）".to_owned())?;
    let (sentinel, payload) = decode_unified_entry(&value, Some(NEW_SENTINEL))?;
    if sentinel != NEW_SENTINEL {
        return Err(format!("意外的 sentinel：{sentinel}"));
    }
    let refresh = find_field(&payload, 3)?.ok_or("OAuthInfo 里没有 refresh_token（字段 3）".to_owned())?;
    String::from_utf8(refresh).map_err(|_| "refresh_token 不是 UTF-8".to_owned())
}

/// Reading `state.vscdb` needs SQLite, which this build only links on Windows.
#[cfg(not(windows))]
pub fn read_refresh_token(_path: &Path) -> Result<String, String> {
    Err("本机导入目前仅支持 Windows".to_owned())
}

/// The detected database's refresh token.
pub fn read_local_token() -> Result<String, String> {
    let path = find_db().ok_or_else(|| {
        let tried = default_db_paths()
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("、");
        format!("没有找到 Antigravity 数据库，已查找：{tried}")
    })?;
    read_refresh_token(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes one unified-state entry the way the app writes it.
    fn entry(sentinel: &str, payload: &[u8]) -> String {
        let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
        let mut row = Vec::new();
        row.push(0x0a);
        row.push(encoded.len() as u8);
        row.extend_from_slice(encoded.as_bytes());
        let mut data_entry = Vec::new();
        data_entry.push(0x0a);
        data_entry.push(sentinel.len() as u8);
        data_entry.extend_from_slice(sentinel.as_bytes());
        data_entry.push(0x12);
        data_entry.push(row.len() as u8);
        data_entry.extend_from_slice(&row);
        let mut topic = Vec::new();
        topic.push(0x0a);
        topic.push(data_entry.len() as u8);
        topic.extend_from_slice(&data_entry);
        base64::engine::general_purpose::STANDARD.encode(topic)
    }

    #[test]
    fn decodes_the_nested_entry() {
        // OAuthInfo with the refresh token in field 3.
        let mut oauth_info = Vec::new();
        let token = b"1//0gTESTTOKEN";
        oauth_info.extend_from_slice(&[0x1a, token.len() as u8]);
        oauth_info.extend_from_slice(token);
        let outer = entry(NEW_SENTINEL, &oauth_info);
        let (sentinel, payload) = decode_unified_entry(&outer, Some(NEW_SENTINEL)).expect("entry");
        assert_eq!(sentinel, NEW_SENTINEL);
        let refresh = find_field(&payload, 3).expect("field").expect("refresh");
        assert_eq!(refresh, token);
    }

    #[test]
    fn reports_an_unreadable_entry() {
        assert!(decode_unified_entry("not base64!!", None).is_err());
        let empty = base64::engine::general_purpose::STANDARD.encode([0_u8; 0]);
        assert!(decode_unified_entry(&empty, None).is_err());
    }

    #[test]
    fn encodes_and_replaces_unified_oauth_entry_preserving_siblings() {
        let mut existing = encode_unified_topic_entry("otherKey", b"keep-me");
        existing.extend(encode_unified_topic_entry(NEW_SENTINEL, b"stale-oauth"));

        let stripped = remove_unified_topic_entry(&existing, NEW_SENTINEL).expect("strip");
        let oauth_info = encode_oauth_info("ya29.access", "1//0gFRESH", 1_800_000_000);
        let mut updated = stripped;
        updated.extend(encode_unified_topic_entry(NEW_SENTINEL, &oauth_info));
        let outer_b64 = base64::engine::general_purpose::STANDARD.encode(&updated);

        let (_, decoded_oauth) =
            decode_unified_entry(&outer_b64, Some(NEW_SENTINEL)).expect("oauth");
        let refresh = find_field(&decoded_oauth, 3)
            .expect("field")
            .expect("refresh");
        assert_eq!(refresh, b"1//0gFRESH");

        let (_, decoded_other) =
            decode_unified_entry(&outer_b64, Some("otherKey")).expect("sibling");
        assert_eq!(decoded_other, b"keep-me");
    }
}
