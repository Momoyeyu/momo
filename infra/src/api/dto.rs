//! 统一 DTO：CLI `--json` 输出与 REST `/api` 响应共用同一套 serde 类型
//! （issue #46：cmds 各报告类型迁入，web handler 只做 HTTP↔core 适配）。
//! 顶层恒 `"schema": 1`；错误统一 `{schema, error:{code,message}}` 形态
//! （tests/api_contract_test.rs 锁定字段名/类型/必选性）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config_schema::SourceReport;
use crate::jobs::{JobKind, JobRecord};

/// 全部报告/错误顶层的 schema 版本号。
pub const SCHEMA_VERSION: u8 = 1;

// ---------- 检查矩阵基元（doctor/serve/pull/bench/gc/switch 共用） ----------

/// 检查项状态：`ok`/`warn`/`fail`（serde 小写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl CheckStatus {
    /// human 渲染标签（output::render_* 用）。
    pub(crate) fn label(self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }
}

/// 单项检查。`hint` 恒出现在 JSON（schema 稳定），可为 null。
#[derive(Debug, Serialize)]
pub struct Check {
    pub id: String,
    pub status: CheckStatus,
    pub detail: String,
    pub hint: Option<String>,
}

impl Check {
    pub fn ok(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: CheckStatus::Ok,
            detail: detail.into(),
            hint: None,
        }
    }

    pub fn warn(id: impl Into<String>, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: CheckStatus::Warn,
            detail: detail.into(),
            hint: Some(hint.into()),
        }
    }

    pub fn fail(id: impl Into<String>, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
            hint: Some(hint.into()),
        }
    }
}

/// doctor 分组（system/toolchain/python/network/env/models/endpoints）。
#[derive(Debug, Serialize)]
pub struct CheckGroup {
    pub name: String,
    pub checks: Vec<Check>,
}

impl CheckGroup {
    pub fn new(name: impl Into<String>, checks: Vec<Check>) -> Self {
        Self {
            name: name.into(),
            checks,
        }
    }
}

/// ok/warn/fail 计数汇总。
#[derive(Debug, Serialize)]
pub struct Summary {
    pub ok: usize,
    pub warn: usize,
    pub fail: usize,
}

impl Summary {
    /// 从检查项列表计数（cmds 各命令与 DoctorReport 共用口径）。
    pub fn from_checks<'a>(checks: impl IntoIterator<Item = &'a Check>) -> Self {
        let mut s = Self {
            ok: 0,
            warn: 0,
            fail: 0,
        };
        for c in checks {
            match c.status {
                CheckStatus::Ok => s.ok += 1,
                CheckStatus::Warn => s.warn += 1,
                CheckStatus::Fail => s.fail += 1,
            }
        }
        s
    }
}

// ---------- 子命令报告（CLI `--json` 顶层对象；REST 复用同型） ----------

/// `momoctl doctor --json` / `GET /api/doctor`：
/// `{schema, profile, groups:[{name, checks:[{id,status,detail,hint}]}], summary}`。
#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub schema: u8,
    pub profile: String,
    pub groups: Vec<CheckGroup>,
    pub summary: Summary,
}

impl DoctorReport {
    pub fn new(profile: &str, groups: Vec<CheckGroup>) -> Self {
        let summary = Summary::from_checks(groups.iter().flat_map(|g| g.checks.iter()));
        Self {
            schema: SCHEMA_VERSION,
            profile: profile.to_string(),
            groups,
            summary,
        }
    }

    pub fn has_fail(&self) -> bool {
        self.summary.fail > 0
    }
}

/// `momoctl serve <up|down|status|restart> --json` / `GET /api/serve/status`。
#[derive(Debug, Serialize)]
pub struct ServeReport {
    pub schema: u8,
    pub command: String,
    pub profile: String,
    pub checks: Vec<Check>,
    pub summary: Summary,
}

/// `GET /api/serve/components` 单组件条目（内置 profile 组件表）。
#[derive(Debug, Serialize)]
pub struct ComponentInfo {
    pub name: String,
    pub order: u32,
    /// 外部端点：只探活不拉起。
    pub external: bool,
    /// 仅当 MOMO_LLM_ENDPOINT 指 loopback 时才拉起。
    pub local_endpoint_only: bool,
}

/// `GET /api/serve/components`：组件表（order 升序）+ profile。
#[derive(Debug, Serialize)]
pub struct ServeComponentsReport {
    pub schema: u8,
    pub profile: String,
    pub components: Vec<ComponentInfo>,
}

/// `POST /api/serve/{up|down|restart}` 请求体；`only` 为空 = 全组件。
#[derive(Debug, Deserialize)]
pub struct ServeActionRequest {
    #[serde(default)]
    pub only: Vec<String>,
}

/// `serve logs` 单组件条目。
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub component: String,
    /// 日志文件路径；尚无日志文件时为 null。
    pub path: Option<String>,
    pub lines: Vec<String>,
}

/// `momoctl serve logs --json` 顶层对象。
#[derive(Debug, Serialize)]
pub struct ServeLogsReport {
    pub schema: u8,
    pub command: String,
    pub profile: String,
    pub logs: Vec<LogEntry>,
}

/// `serve logs -f --json` 的 NDJSON 行（每批一条）。
#[derive(Debug, Serialize)]
pub struct LogBatch<'a> {
    pub component: &'a str,
    pub lines: Vec<&'a str>,
}

/// `momoctl pull [--check] --json`。
#[derive(Debug, Serialize)]
pub struct PullReport {
    pub schema: u8,
    pub command: String,
    pub manifest: String,
    pub models_dir: String,
    /// 下载源（--check 模式为 null）。
    pub endpoint: Option<String>,
    pub checks: Vec<Check>,
    pub summary: Summary,
}

/// bench 单回合测量。
#[derive(Debug, Serialize)]
pub struct TurnResult {
    pub turn: u32,
    /// 发→`input.accepted`。
    pub accepted_ms: Option<u64>,
    /// 发→首个 `output.sentence`。
    pub first_sentence_ms: Option<u64>,
    /// 发→首个下行 `audio.chunk`（二进制帧）。
    pub first_audio_ms: Option<u64>,
    /// 发→`turn.finished`。
    pub finished_ms: Option<u64>,
    pub outcome: Option<String>,
    /// rejected/timeout/异常说明。
    pub note: Option<String>,
}

/// bench 阶段统计（p50/p95/max）。
#[derive(Debug, Serialize)]
pub struct StageStats {
    pub samples: usize,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub max_ms: u64,
}

/// `momoctl bench --json`。
#[derive(Debug, Serialize)]
pub struct BenchReport {
    pub schema: u8,
    pub command: String,
    pub endpoint: String,
    pub turns: u32,
    pub target_first_audio_ms: u64,
    pub results: Vec<TurnResult>,
    pub stats: BTreeMap<String, StageStats>,
    pub checks: Vec<Check>,
    pub summary: Summary,
}

/// `momoctl gc --json`。
#[derive(Debug, Serialize)]
pub struct GcReport {
    pub schema: u8,
    pub command: String,
    pub models_dir: String,
    /// `dry-run`（默认）或 `delete`（--yes 且 pull 未在运行）。
    pub mode: String,
    pub keep: u32,
    /// 回收候选数与字节数（dry-run 口径为「可回收」，delete 口径为「已删除」）。
    pub candidates: usize,
    pub candidate_bytes: u64,
    pub deleted_bytes: u64,
    pub checks: Vec<Check>,
    pub summary: Summary,
}

/// `momoctl switch` 的 env 差异行；敏感键为打码值，None = 该侧未设置。
#[derive(Debug, Serialize)]
pub struct DiffEntry {
    pub key: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// `momoctl switch --json`。
#[derive(Debug, Serialize)]
pub struct SwitchReport {
    pub schema: u8,
    pub command: String,
    pub to: String,
    pub env_file: String,
    pub target: String,
    pub checks: Vec<Check>,
    pub env_diff: Vec<DiffEntry>,
    pub summary: Summary,
}

/// `momoctl config check --json`。
#[derive(Debug, Serialize)]
pub struct ConfigCheckReport {
    pub schema: u8,
    pub root: String,
    pub sources: Vec<SourceReport>,
    pub summary: ConfigCheckSummary,
}

#[derive(Debug, Serialize)]
pub struct ConfigCheckSummary {
    /// 被检查来源文件数。
    pub sources: usize,
    /// 漂移条目总数。
    pub drift: usize,
}

/// `momoctl web --json` 启动对象。
#[derive(Debug, Serialize)]
pub struct WebReport {
    pub schema: u8,
    pub command: String,
    pub listen: String,
    pub url: String,
}

/// 未实现子命令的统一 JSON（`{schema, command, status:"not_implemented"}`）。
#[derive(Debug, Serialize)]
pub struct CommandStatus {
    pub schema: u8,
    pub command: String,
    pub status: String,
}

// ---------- REST `/api` 出入参 ----------

/// `GET /api[/]`：路由索引。
#[derive(Debug, Serialize)]
pub struct IndexReport {
    pub schema: u8,
    pub status: &'static str,
    pub routes: Vec<&'static str>,
}

/// `GET /api/healthz` / `readyz`：进程级探针。
#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub schema: u8,
    pub status: &'static str,
}

/// `GET /api/csrf` 响应：进程级 CSRF token。
/// state-changing 请求须以 `X-CSRF-Token` 头原样回带（web/guard.rs）。
#[derive(Debug, Serialize)]
pub struct CsrfToken {
    pub schema: u8,
    pub csrf_token: String,
}

/// 读端点公共查询参数：`?profile=normal|degraded`（缺省读 MOMO_PROFILE env）。
#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    pub profile: Option<String>,
}

// ---------- jobs ----------

/// `POST /api/jobs` 请求体：`{kind?, args, desc?}`。
#[derive(Debug, Deserialize)]
pub struct JobSubmitRequest {
    /// job 类别；缺省 `train`。
    #[serde(default)]
    pub kind: Option<JobKind>,
    /// exec_local 载荷 argv（args[0] 为可执行程序）。
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub desc: Option<String>,
}

/// `GET /api/jobs` 列表响应。
#[derive(Debug, Serialize)]
pub struct JobListReport {
    pub schema: u8,
    pub status: &'static str,
    pub jobs: Vec<JobRecord>,
}

/// `GET /api/jobs/{id}` / `POST /api/jobs` / `POST /api/jobs/{id}/cancel` 响应。
#[derive(Debug, Serialize)]
pub struct JobReport {
    pub schema: u8,
    pub status: &'static str,
    pub job: JobRecord,
    /// cancel 等操作的附加说明（幂等/降级提示）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `GET /api/jobs/{id}/logs` SSE `event: log` 的 data。
#[derive(Debug, Serialize)]
pub struct JobLogChunk {
    /// 字节偏移（重连续传用）。
    pub cursor: u64,
    pub chunk: String,
}

/// `GET /api/jobs/{id}/logs` SSE `event: end` 的 data（终态排空后关流）。
#[derive(Debug, Serialize)]
pub struct JobLogEnd {
    pub status: String,
/// `GET /api/serve/logs/{name}/stream` 查询参数：
/// `profile` 用于组件名校验；`cursor` 为字节偏移（SSE 断线续传，
/// 前端也可走 `Last-Event-ID` 头）。
#[derive(Debug, Deserialize)]
pub struct LogStreamQuery {
    pub profile: Option<String>,
    pub cursor: Option<u64>,
}

/// 统一错误体。
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// 机器可读短标识：unauthorized / forbidden / csrf / not_found /
    /// not_implemented / method_not_allowed / bad_request / internal。
    pub code: String,
    /// 面向用户的一句话说明（英文）。
    pub message: String,
}

/// 统一错误响应：`{schema:1, error:{code,message}}`。
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub schema: u8,
    pub error: ErrorBody,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            error: ErrorBody {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}
