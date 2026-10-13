//! `MOMO_*` env 加载与校验（权威定义：momo-docs `01-architecture/config.md` §2.2）。
//! 只认环境变量（dotenvy 读 `.env`）；非法值启动即报错并列出全部问题。

use std::{fmt, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

use thiserror::Error;
use url::Url;

/// momod 生效配置（config.md §2.2 中由 app 读取的键）。
#[derive(Debug, Clone)]
pub struct Config {
    /// `MOMO_ORCH_LISTEN`，WS + 静态 UI 监听。
    pub orch_listen: SocketAddr,
    /// `MOMO_WEB_ORIGIN`，浏览器来源（CORS/WS Origin 白名单）。
    pub web_origin: String,
    /// `MOMO_UI_TOKEN`，`/ws/ui` hello 的期望 `auth_token`；敏感。
    /// 空=standalone dev，不比对（hello.auth_token 字段仍必填）。
    pub ui_token: Option<SecretString>,
    /// `MOMO_LLM_ENDPOINT`，大脑 OpenAI-compatible base URL（直连，无网关）。
    pub llm_endpoint: Url,
    /// `MOMO_LLM_MODEL`，传给大脑的 model 字段。
    pub llm_model: String,
    /// `MOMO_LLM_API_KEY`，敏感；本地可为空。
    pub llm_api_key: Option<SecretString>,
    /// `MOMO_LLM_TIMEOUT_MS`，LLM 请求总超时。
    pub llm_timeout: Duration,
    /// `MOMO_TTS_ENDPOINT`，momo-svc-tts（momo-ws v1）。
    pub tts_endpoint: Url,
    /// `MOMO_ASR_ENDPOINT`，momo-svc-asr（momo-ws v1）。
    pub asr_endpoint: Url,
    /// `MOMO_UI_DIR`，设了且存在则 ServeDir 托管 `web/dist`。
    pub ui_dir: Option<PathBuf>,
    /// `MOMO_LOG_LEVEL`。
    pub log_level: String,
    /// `MOMO_LOG_FORMAT`。
    pub log_format: LogFormat,
    /// `MOMO_BARGEIN_ENABLED`。
    pub barge_in_enabled: bool,
    /// `MOMO_BARGEIN_MIN_MS`，speech_start 防抖时长。
    pub barge_in_min: Duration,
    /// `MOMO_IDLE_TIMEOUT_S`，空闲自发行为触发秒数（0=关闭）。
    pub idle_timeout: Duration,
    /// `MOMO_SENT_SOFT_CHARS`，句切弱切阈值。
    pub sent_soft_chars: usize,
    /// `MOMO_SENT_MAX_CHARS`，句切硬上限。
    pub sent_max_chars: usize,
    /// `MOMO_HISTORY_TURNS`，注入 LLM 的历史回合数。
    pub history_turns: usize,
    /// `MOMO_FILTER_ENABLED`，输入/输出过滤开关（P5 前实装）。
    pub filter_enabled: bool,
}

impl Config {
    /// 读取 `.env`（存在则加载，不报错）后从环境变量构建配置。
    pub fn from_env() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        Self::load(|key| std::env::var(key).ok())
    }

    /// 从任意键值来源构建配置（测试可注入，进程内用 [`Config::from_env`]）。
    ///
    /// 聚合所有非法/缺失项，一次报错列全，而不是遇到第一个就退出。
    #[doc(hidden)]
    pub fn load(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let mut errors: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();

        // config.md §2.2 全部键都有内置默认值，暂无比填项；
        // 机制保留：把 key 名 push 进 `required` 即变为必填。
        let required: &[&str] = &[];
        for key in required {
            if get(key).is_none() {
                missing.push((*key).to_string());
            }
        }

        let mut parse = |key: &str, default: &str| -> Option<String> {
            match get(key) {
                Some(v) if !v.is_empty() => Some(v),
                _ => Some(default.to_string()),
            }
        };

        let orch_listen = typed(
            &mut parse,
            "MOMO_ORCH_LISTEN",
            "127.0.0.1:7800",
            &mut errors,
        );
        let web_origin = parse("MOMO_WEB_ORIGIN", "http://localhost:5173").unwrap_or_default();
        let ui_token = get("MOMO_UI_TOKEN")
            .filter(|v| !v.is_empty())
            .map(SecretString::new);
        let llm_endpoint = typed(
            &mut parse,
            "MOMO_LLM_ENDPOINT",
            "http://mac-studio.local:7820/v1",
            &mut errors,
        );
        let llm_model = parse("MOMO_LLM_MODEL", "qwen3-30b-a3b-4bit").unwrap_or_default();
        let llm_api_key = get("MOMO_LLM_API_KEY")
            .filter(|v| !v.is_empty())
            .map(SecretString::new);
        let llm_timeout_ms = typed::<u64>(&mut parse, "MOMO_LLM_TIMEOUT_MS", "30000", &mut errors);
        let tts_endpoint = typed(
            &mut parse,
            "MOMO_TTS_ENDPOINT",
            "ws://127.0.0.1:7811/ws",
            &mut errors,
        );
        let asr_endpoint = typed(
            &mut parse,
            "MOMO_ASR_ENDPOINT",
            "ws://127.0.0.1:7810/ws",
            &mut errors,
        );
        let ui_dir = get("MOMO_UI_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let log_level = parse("MOMO_LOG_LEVEL", "info").unwrap_or_else(|| "info".into());
        let log_format = typed(&mut parse, "MOMO_LOG_FORMAT", "pretty", &mut errors);
        let barge_in_enabled = typed(&mut parse, "MOMO_BARGEIN_ENABLED", "true", &mut errors);
        let barge_in_min_ms = typed::<u64>(&mut parse, "MOMO_BARGEIN_MIN_MS", "250", &mut errors);
        let idle_timeout_s = typed::<u64>(&mut parse, "MOMO_IDLE_TIMEOUT_S", "20", &mut errors);
        let sent_soft_chars = typed(&mut parse, "MOMO_SENT_SOFT_CHARS", "25", &mut errors);
        let sent_max_chars = typed(&mut parse, "MOMO_SENT_MAX_CHARS", "60", &mut errors);
        let history_turns = typed(&mut parse, "MOMO_HISTORY_TURNS", "20", &mut errors);
        let filter_enabled = typed(&mut parse, "MOMO_FILTER_ENABLED", "false", &mut errors);

        if !missing.is_empty() {
            return Err(ConfigError::Missing(missing));
        }
        if !errors.is_empty() {
            return Err(ConfigError::Invalid(errors));
        }

        Ok(Config {
            orch_listen: orch_listen.unwrap(),
            web_origin,
            ui_token,
            llm_endpoint: llm_endpoint.unwrap(),
            llm_model,
            llm_api_key,
            llm_timeout: Duration::from_millis(llm_timeout_ms.unwrap()),
            tts_endpoint: tts_endpoint.unwrap(),
            asr_endpoint: asr_endpoint.unwrap(),
            ui_dir,
            log_level,
            log_format: log_format.unwrap(),
            barge_in_enabled: barge_in_enabled.unwrap(),
            barge_in_min: Duration::from_millis(barge_in_min_ms.unwrap()),
            idle_timeout: Duration::from_secs(idle_timeout_s.unwrap()),
            sent_soft_chars: sent_soft_chars.unwrap(),
            sent_max_chars: sent_max_chars.unwrap(),
            history_turns: history_turns.unwrap(),
            filter_enabled: filter_enabled.unwrap(),
        })
    }
}

/// 解析单个 env：取值（或默认）后 `FromStr`，失败记入 `errors`。
fn typed<T: FromStr>(
    parse: &mut impl FnMut(&str, &str) -> Option<String>,
    key: &str,
    default: &str,
    errors: &mut Vec<String>,
) -> Option<T>
where
    T::Err: fmt::Display,
{
    let raw = parse(key, default)?;
    raw.parse::<T>()
        .map_err(|e| errors.push(format!("{key}={raw:?}: {e}")))
        .ok()
}

/// 日志输出格式（`MOMO_LOG_FORMAT`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}

impl FromStr for LogFormat {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pretty" => Ok(LogFormat::Pretty),
            "json" => Ok(LogFormat::Json),
            other => Err(ConfigError::Invalid(vec![format!(
                "expected pretty|json, got {other:?}"
            )])),
        }
    }
}

/// 敏感字符串：日志/Debug 输出打码（前 4 位 + `…`，config.md §4）。
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// 仅在真正需要明文的边界（如拼 LLM 请求头）调用。
    pub fn expose(&self) -> &str {
        &self.0
    }

    fn masked(&self) -> String {
        let head: String = self.0.chars().take(4).collect();
        format!("{head}…")
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString({})", self.masked())
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.masked())
    }
}

/// 配置加载失败。
#[derive(Debug, Error)]
pub enum ConfigError {
    /// 缺失必需 env，全部列出。
    #[error("missing required env vars: {}", .0.join(", "))]
    Missing(Vec<String>),
    /// env 存在但解析失败，逐条列出。
    #[error("invalid env values:\n{}", .0.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n"))]
    Invalid(Vec<String>),
}
