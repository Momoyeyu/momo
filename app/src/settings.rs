//! 用户在 app 设置面维护的 provider 配置（docs/01-architecture/settings.md）。
//!
//! 生效顺序：env 默认值 < 本文件覆盖。文件只含用户改过的 provider 字段，
//! 其余配置仍走 `Config::from_env`。路径固定为平台 config dir 下
//! `momo/settings.json`（dirs::config_dir），不引入新的 MOMO_ env 键。

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    config::{Config, ConfigError, SecretString},
    error::AppError,
};

/// 用户可改的 provider 单槽位配置：每类能力一个 endpoint（ADR-028）。
#[derive(Debug, Clone)]
pub struct ProviderSettings {
    /// OpenAI-compatible LLM base URL。
    pub llm_endpoint: Url,
    /// 传给大脑的 model 字段。
    pub llm_model: String,
    /// LLM 上游鉴权 key（本地可空）。
    pub llm_api_key: Option<SecretString>,
    /// ASR 服务 WS 地址。
    pub asr_endpoint: Url,
    /// TTS 服务 WS 地址。
    pub tts_endpoint: Url,
}

/// settings.json 的落盘结构：字段缺省表示「沿用 env」。
#[derive(Debug, Serialize, Deserialize, Default)]
struct SettingsFile {
    llm: Option<LlmSection>,
    asr: Option<EndpointSection>,
    tts: Option<EndpointSection>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LlmSection {
    endpoint: String,
    model: String,
    /// 明文 key 落用户机器本地文件（~/.aws/credentials 同类）；
    /// 文件模式 0600（unix），Debug 仍打码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EndpointSection {
    endpoint: String,
}

/// 设置文件路径：`{config_dir}/momo/settings.json`；无 config dir 时退到当前目录。
pub fn settings_path() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("momo").join("settings.json"))
        .unwrap_or_else(|| PathBuf::from("momo-settings.json"))
}

impl ProviderSettings {
    /// env 配置 → provider 配置（不读 settings.json；测试与纯 env 路径用）。
    pub fn from_config(config: &Config) -> Self {
        Self {
            llm_endpoint: config.llm_endpoint.clone(),
            llm_model: config.llm_model.clone(),
            llm_api_key: config.llm_api_key.clone(),
            asr_endpoint: config.asr_endpoint.clone(),
            tts_endpoint: config.tts_endpoint.clone(),
        }
    }

    /// env 配置 → 生效 provider 配置（settings.json 存在则按字段覆盖）。
    pub fn load(config: &Config) -> Result<Self, AppError> {
        let mut s = Self::from_config(config);
        s.apply_file(&settings_path())?;
        Ok(s)
    }

    /// 读 settings.json 并按节覆盖；文件不存在/读不到内容时静默跳过，
    /// 存在但解析失败才报错（用户手改坏文件时宁可启动失败也不静默吃错）。
    fn apply_file(&mut self, path: &Path) -> Result<(), AppError> {
        let Ok(text) = fs::read_to_string(path) else {
            return Ok(());
        };
        let file: SettingsFile = serde_json::from_str(&text).map_err(|e| {
            AppError::Config(ConfigError::Invalid(vec![format!(
                "settings file {}: {e}",
                path.display()
            )]))
        })?;
        if let Some(llm) = file.llm {
            self.llm_endpoint = parse_url(&llm.endpoint, &["http", "https"], "llm.endpoint")?;
            self.llm_model = llm.model;
            self.llm_api_key = llm.api_key.map(SecretString::new);
        }
        if let Some(asr) = file.asr {
            self.asr_endpoint = parse_url(&asr.endpoint, &["ws", "wss"], "asr.endpoint")?;
        }
        if let Some(tts) = file.tts {
            self.tts_endpoint = parse_url(&tts.endpoint, &["ws", "wss"], "tts.endpoint")?;
        }
        Ok(())
    }

    /// 持久化到 settings.json（tmp + rename 原子写；unix 0600）。
    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        let file = SettingsFile {
            llm: Some(LlmSection {
                endpoint: self.llm_endpoint.to_string(),
                model: self.llm_model.clone(),
                api_key: self.llm_api_key.as_ref().map(|k| k.expose().to_string()),
            }),
            asr: Some(EndpointSection {
                endpoint: self.asr_endpoint.to_string(),
            }),
            tts: Some(EndpointSection {
                endpoint: self.tts_endpoint.to_string(),
            }),
        };
        let text = serde_json::to_string_pretty(&file)
            .map_err(|e| AppError::Internal(format!("serialize settings: {e}")))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| AppError::Internal(format!("create {}: {e}", dir.display())))?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text)
            .map_err(|e| AppError::Internal(format!("write {}: {e}", tmp.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&tmp, path)
            .map_err(|e| AppError::Internal(format!("rename to {}: {e}", path.display())))
    }
}

/// URL 校验：解析 + scheme 白名单。
pub fn parse_url(raw: &str, schemes: &[&str], field: &str) -> Result<Url, AppError> {
    let url = Url::parse(raw.trim()).map_err(|e| {
        AppError::Config(ConfigError::Invalid(vec![format!("{field}: invalid url: {e}")]))
    })?;
    if !schemes.contains(&url.scheme()) {
        return Err(AppError::Config(ConfigError::Invalid(vec![format!(
            "{field}: scheme must be one of {schemes:?}"
        )])));
    }
    Ok(url)
}
