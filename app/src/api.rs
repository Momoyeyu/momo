//! `/api/settings` + `/api/avatars`：app 设置面的 REST 后端
//! （docs/01-architecture/settings.md）。
//!
//! GET /api/settings 返回生效 provider 配置（api_key 只报是否已设，不回显明文）；
//! PUT 校验后热应用（LLM 下请求生效；ASR/TTS 链路断开重连到新版）并持久化
//! 到 `{config_dir}/momo/settings.json`。
//! GET /api/avatars 返回形象 manifest：内置 Hiyori + `{config_dir}/momo/avatars/*/`
//! 下的模型包扫描项（该目录由 momod 静态托管在 `/avatars/`）。
//! 鉴权：`MOMO_UI_TOKEN` 已设时须 `Authorization: Bearer <token>`；未设（standalone dev）放行。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

use crate::{
    settings::{ProviderSettings, parse_url},
    ws::server::AppState,
};

/// `GET /api/settings`：生效配置（敏感字段只报 set 状态）。
pub async fn get_settings(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let s = state.settings.read().await;
    (StatusCode::OK, Json(view(&s))).into_response()
}

/// `PUT /api/settings` 请求体：字段缺省=不修改该项。
#[derive(Debug, Deserialize)]
pub struct PutBody {
    llm: Option<PutLlm>,
    asr: Option<PutEndpoint>,
    tts: Option<PutEndpoint>,
}

#[derive(Debug, Deserialize)]
pub struct PutLlm {
    endpoint: Option<String>,
    model: Option<String>,
    /// 显式传 null → 清除 key；缺省 → 不变。
    api_key: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct PutEndpoint {
    endpoint: Option<String>,
}

/// `PUT /api/settings`：校验 → 热应用 → 落盘。任何一步失败都不改生效配置。
pub async fn put_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PutBody>,
) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let mut s = state.settings.read().await.clone();
    if let Some(llm) = body.llm {
        if let Some(ep) = llm.endpoint {
            match parse_url(&ep, &["http", "https"], "llm.endpoint") {
                Ok(u) => s.llm_endpoint = u,
                Err(e) => return invalid(e.to_string(), "llm.endpoint"),
            }
        }
        if let Some(m) = llm.model {
            if m.trim().is_empty() {
                return invalid("model must not be empty".into(), "llm.model");
            }
            s.llm_model = m;
        }
        if let Some(key) = llm.api_key {
            s.llm_api_key = key.map(crate::config::SecretString::new);
        }
    }
    if let Some(asr) = body.asr.and_then(|a| a.endpoint) {
        match parse_url(&asr, &["ws", "wss"], "asr.endpoint") {
            Ok(u) => s.asr_endpoint = u,
            Err(e) => return invalid(e.to_string(), "asr.endpoint"),
        }
    }
    if let Some(tts) = body.tts.and_then(|t| t.endpoint) {
        match parse_url(&tts, &["ws", "wss"], "tts.endpoint") {
            Ok(u) => s.tts_endpoint = u,
            Err(e) => return invalid(e.to_string(), "tts.endpoint"),
        }
    }

    // 落盘先于热应用：持久化失败时不出现「生效了但没记住」的半状态。
    if let Err(e) = s.save(&state.settings_path) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(err_body("internal", &e.to_string())),
        )
            .into_response();
    }
    state.llm.set_conf(
        s.llm_endpoint.clone(),
        s.llm_model.clone(),
        s.llm_api_key.clone(),
    );
    state.asr.set_endpoint(s.asr_endpoint.clone()).await;
    state.tts.set_endpoint(s.tts_endpoint.clone()).await;
    *state.settings.write().await = s.clone();
    (StatusCode::OK, Json(view(&s))).into_response()
}

/// `{config_dir}/momo/avatars`：用户 Live2D 模型包根目录（settings.md §形象
/// manifest，与 settings.json 同父目录）；无 config dir 时退到当前目录。
pub fn avatars_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("momo").join("avatars"))
        .unwrap_or_else(|| PathBuf::from("momo-avatars"))
}

/// `/api/avatars` 返回项：`{id, name, url}`。
#[derive(Debug, Serialize)]
pub struct AvatarEntry {
    id: String,
    name: String,
    url: String,
}

/// `GET /api/avatars`：内置 Hiyori + avatars_dir 扫描的模型包 manifest。
/// 内置项（web dist 分发）恒在首位，id 固定 `builtin`。
pub async fn get_avatars(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let mut avatars = vec![AvatarEntry {
        id: "builtin".into(),
        name: "Hiyori（内置）".into(),
        url: "/live2d/hiyori/Hiyori.model3.json".into(),
    }];
    avatars.extend(scan_avatars(&avatars_dir()));
    (
        StatusCode::OK,
        Json(json!({ "schema": 1, "avatars": avatars })),
    )
        .into_response()
}

/// 扫描 `dir/*/`：每个含 `*.model3.json` 入口的子目录一项，按目录名排序。
/// 无入口的包、隐藏目录、顶层散落文件跳过；目录不存在返回空。
fn scan_avatars(dir: &Path) -> Vec<AvatarEntry> {
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Ok(read) = fs::read_dir(dir) {
        for e in read.flatten() {
            if !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Some(id) = e.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if id.starts_with('.') {
                continue;
            }
            if let Some(file) = first_model3(&e.path()) {
                entries.push((id, file));
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
        .into_iter()
        .map(|(id, file)| AvatarEntry {
            url: avatar_url(&id, &file),
            name: id.clone(),
            id,
        })
        .collect()
}

/// 包目录内按文件名排序的第一个 `*.model3.json`。
fn first_model3(dir: &Path) -> Option<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.ends_with(".model3.json"))
        .collect();
    names.sort();
    names.into_iter().next()
}

/// `/avatars/{id}/{file}`：段级 percent-encode，含空格/井号的包名不出错。
fn avatar_url(id: &str, file: &str) -> String {
    let mut base = Url::parse("http://localhost/").expect("static base url parses");
    base.path_segments_mut()
        .expect("http url is not cannot-be-a-base")
        .extend(["avatars", id, file]);
    base.path().to_string()
}

/// 敏感字段不外泄：api_key 只报 set 布尔。
fn view(s: &ProviderSettings) -> Value {
    json!({
        "schema": 1,
        "settings": {
            "llm": {
                "endpoint": s.llm_endpoint.to_string(),
                "model": s.llm_model,
                "api_key_set": s.llm_api_key.is_some(),
            },
            "asr": { "endpoint": s.asr_endpoint.to_string() },
            "tts": { "endpoint": s.tts_endpoint.to_string() },
        }
    })
}

/// `MOMO_UI_TOKEN` 已设 → Bearer 校验；未设放行（与 ws hello 的 standalone 语义一致）。
fn check_auth(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let expected = state.config.ui_token.as_ref()?;
    let ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == expected.expose());
    if ok {
        None
    } else {
        Some(
            (
                StatusCode::UNAUTHORIZED,
                Json(err_body("unauthorized", "bearer token required")),
            )
                .into_response(),
        )
    }
}

fn err_body(code: &str, message: &str) -> Value {
    json!({
        "schema": 1,
        "error": { "code": code, "message": message }
    })
}

fn invalid(message: String, field: &str) -> Response {
    let mut err = err_body("invalid", &message);
    err["error"]["fields"] = json!([field]);
    (StatusCode::BAD_REQUEST, Json(err)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 隔离的临时 avatars 根目录（返回即已创建）。
    fn temp_avatars_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "momo-avatars-test-{}-{}",
            std::process::id(),
            ulid::Ulid::new()
        ));
        fs::create_dir_all(&dir).expect("create temp avatars root");
        dir
    }

    #[test]
    fn scan_skips_hidden_dirs_and_dirs_without_model3() {
        let root = temp_avatars_root();
        fs::create_dir_all(root.join("haru")).unwrap();
        fs::write(root.join("haru/haru.model3.json"), "{}").unwrap();
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join(".hidden/x.model3.json"), "{}").unwrap();
        // 顶层散落文件不是包。
        fs::write(root.join("stray.model3.json"), "{}").unwrap();

        let got = scan_avatars(&root);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "haru");
        assert_eq!(got[0].name, "haru");
        assert_eq!(got[0].url, "/avatars/haru/haru.model3.json");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_sorts_entries_and_picks_first_model3() {
        let root = temp_avatars_root();
        fs::create_dir_all(root.join("zzz")).unwrap();
        fs::write(root.join("zzz/b.model3.json"), "{}").unwrap();
        fs::write(root.join("zzz/a.model3.json"), "{}").unwrap();
        fs::create_dir_all(root.join("aaa")).unwrap();
        fs::write(root.join("aaa/aaa.model3.json"), "{}").unwrap();

        let got = scan_avatars(&root);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, "aaa");
        assert_eq!(got[1].id, "zzz");
        // 同包多入口取文件名排序首个。
        assert_eq!(got[1].url, "/avatars/zzz/a.model3.json");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_encodes_special_chars_in_dir_name() {
        let root = temp_avatars_root();
        fs::create_dir_all(root.join("my pack#1")).unwrap();
        fs::write(root.join("my pack#1/m.model3.json"), "{}").unwrap();

        let got = scan_avatars(&root);
        assert_eq!(got[0].url, "/avatars/my%20pack%231/m.model3.json");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_missing_dir_returns_empty() {
        let got = scan_avatars(Path::new("definitely-missing-momo-avatars-dir"));
        assert!(got.is_empty());
    }
}
