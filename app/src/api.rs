//! `/api/settings`：app 设置面的 REST 后端（docs/01-architecture/settings.md）。
//!
//! GET 返回生效 provider 配置（api_key 只报是否已设，不回显明文）；
//! PUT 校验后热应用（LLM 下请求生效；ASR/TTS 链路断开重连到新版）并持久化
//! 到 `{config_dir}/momo/settings.json`。
//! 鉴权：`MOMO_UI_TOKEN` 已设时须 `Authorization: Bearer <token>`；未设（standalone dev）放行。

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    settings::{ProviderSettings, parse_url},
    ws::server::AppState,
};

/// `GET /api/settings`：生效配置（敏感字段只报 set 状态）。
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = check_auth(&state, &headers) {
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
    if let Err(resp) = check_auth(&state, &headers) {
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
fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = &state.config.ui_token else {
        return Ok(());
    };
    let ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == expected.expose());
    if ok {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(err_body("unauthorized", "bearer token required")),
        )
            .into_response())
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
