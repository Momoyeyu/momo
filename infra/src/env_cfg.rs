//! dotenvy 加载 + `MOMO_*` 读取与 profile 定位。
//! env 全集权威：momo-docs `01-architecture/config.md`；本模块只读不定义新键。

use std::path::{Path, PathBuf};

use crate::cli::Profile;
use crate::output::AppError;

/// 加载 `.env`：显式 `--env-file` 不存在时报错（exit 2）；默认自动发现失败时静默。
pub fn load_dotenv(path: Option<&Path>) -> Result<(), AppError> {
    match path {
        Some(p) => {
            dotenvy::from_filename(p).map_err(|e| {
                AppError::Config(format!("cannot load env file {}: {e}", p.display()))
            })?;
        }
        None => {
            let _ = dotenvy::dotenv();
        }
    }
    Ok(())
}

/// `MOMO_*` 读取门面（dotenvy 已注入进程 env）。
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvCfg;

/// 无默认值的必需键：缺失在 doctor 中记 fail。
/// （其余键均有 `config.md` 内置默认，缺失仅提示。）
pub const REQUIRED_KEYS: &[&str] = &[
    "MOMO_ORCH_LISTEN",
    "MOMO_LLM_ENDPOINT",
    "MOMO_LLM_MODEL",
    "MOMO_ASR_ENDPOINT",
    "MOMO_TTS_ENDPOINT",
];

/// 敏感键：doctor 只查「是否设置」，不回显值（config.md §4）。
pub const SENSITIVE_KEYS: &[&str] = &["MOMO_LLM_API_KEY"];

/// 有内置默认、但值得在 doctor 中报告取值的键。
pub const DEFAULTED_KEYS: &[&str] = &[
    "MOMO_MODELS_DIR",
    "MOMO_ASR_LISTEN",
    "MOMO_TTS_LISTEN",
    "MOMO_BRAIN_BACKEND",
    "MOMO_BRAIN_PORT",
    "MOMO_BRAIN_MODEL_PATH",
];

impl EnvCfg {
    pub fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    /// 档位解析：CLI flag > `MOMO_PROFILE` > 默认 normal。
    pub fn profile(&self, cli: Option<Profile>) -> Profile {
        if let Some(p) = cli {
            return p;
        }
        match self.get("MOMO_PROFILE").as_deref() {
            Some("degraded") => Profile::Degraded,
            _ => Profile::Normal,
        }
    }

    /// `MOMO_MODELS_DIR`，默认 `~/.cache/momo/models`，展开 `~`。
    pub fn models_dir(&self) -> PathBuf {
        match self.get("MOMO_MODELS_DIR") {
            Some(v) => expand_tilde(&v),
            None => home_dir().join(".cache/momo/models"),
        }
    }

    /// `MOMO_ORCH_LISTEN`，默认 `127.0.0.1:7800`。
    pub fn orch_listen(&self) -> String {
        self.get("MOMO_ORCH_LISTEN")
            .unwrap_or_else(|| "127.0.0.1:7800".into())
    }

    /// `MOMO_ASR_LISTEN`，默认 `127.0.0.1:7810`。
    pub fn asr_listen(&self) -> String {
        self.get("MOMO_ASR_LISTEN")
            .unwrap_or_else(|| "127.0.0.1:7810".into())
    }

    /// `MOMO_TTS_LISTEN`，默认 `127.0.0.1:7811`。
    pub fn tts_listen(&self) -> String {
        self.get("MOMO_TTS_LISTEN")
            .unwrap_or_else(|| "127.0.0.1:7811".into())
    }

    /// `MOMO_LLM_ENDPOINT`（config.md 默认 `http://mac-studio:7820/v1`）。
    /// 返回 None 表示未配置，doctor 端点组按 fail 处理。
    pub fn llm_endpoint(&self) -> Option<String> {
        self.get("MOMO_LLM_ENDPOINT")
    }

    /// `MOMO_BRAIN_PORT`，默认 7820。
    pub fn brain_port(&self) -> u16 {
        self.get("MOMO_BRAIN_PORT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(7820)
    }
}

/// `~` / `~/...` 展开为 `$HOME`；其余原样返回。
pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(path)
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}
