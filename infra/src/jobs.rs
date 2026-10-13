//! jobs 子系统：job 描述与状态机（本模块）、store（`jobs/store.rs`）、
//! exec_local 执行器（`jobs/runner.rs`）。
//!
//! 状态机口径锁定 momo-infra.md「训练 lifecycle 与恢复」：
//! `queued → preparing → running → succeeded/failed`，
//! `canceling/canceled` 独立终态不写成 failed；
//! `recovering` 属 SSH runner（#51）占位，`promoted` 属 promotion issue——
//! 两者进枚举（store/API 要能反序列化），本 runner 不主动迁入。
//!
//! 面向用户输出英文；源码注释中文。

pub mod runner;
pub mod store;

use serde::{Deserialize, Serialize};

/// job 来源域（JobRecord.kind）。`exec_local` 是执行器不是 kind。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum JobKind {
    Pull,
    Serve,
    Train,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::Pull => "pull",
            JobKind::Serve => "serve",
            JobKind::Train => "train",
        }
    }
}

/// job 状态（store 与 API 的序列化值即小写名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Preparing,
    Running,
    Canceling,
    Recovering,
    Succeeded,
    Failed,
    Canceled,
    Promoted,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Preparing => "preparing",
            JobStatus::Running => "running",
            JobStatus::Canceling => "canceling",
            JobStatus::Recovering => "recovering",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
            JobStatus::Canceled => "canceled",
            JobStatus::Promoted => "promoted",
        }
    }

    /// 终态：succeeded/failed/canceled/promoted（canceled 独立终态，不并入 failed）。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Canceled | JobStatus::Promoted
        )
    }

    /// 合法迁移表。momo-infra.md 状态图：
    /// - queued → preparing（launch）/ canceled（launch 前取消）；
    ///   canceling 为带 runner 的取消路径，queued/preparing → canceling 同样合法；
    /// - preparing → running（spawn 成功+process identity 已记）/ failed / canceling / canceled；
    /// - running → canceling / recovering / succeeded / failed；
    /// - canceling → canceled（确认停止）/ failed / recovering / running（rescan 确认存活）；
    /// - recovering → running / failed / canceled（SSH runner 的重连收口，#51）；
    /// - failed → preparing（retry from checkpoint，预留给 retry 命令）；
    /// - succeeded → promoted（frozen gate + 显式 promote，预留给 promotion issue）。
    pub fn can_transition(self, to: JobStatus) -> bool {
        use JobStatus::*;
        match self {
            Queued => matches!(to, Preparing | Canceling | Canceled | Failed),
            Preparing => matches!(to, Running | Failed | Canceling | Canceled),
            Running => matches!(to, Canceling | Recovering | Succeeded | Failed),
            Canceling => matches!(to, Canceled | Failed | Running | Recovering),
            Recovering => matches!(to, Running | Failed | Canceled),
            Failed => matches!(to, Preparing),
            Succeeded => matches!(to, Promoted),
            Canceled | Promoted => false,
        }
    }
}

/// meta/<id>.json 全量记录（#54 JobRecord 口径）。
/// `created_at`/`updated_at` 为 unix epoch 毫秒；`log_path` 为绝对路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub kind: JobKind,
    pub desc: String,
    /// exec_local 载荷 argv（args[0] 为可执行程序）。
    pub args: Vec<String>,
    pub status: JobStatus,
    pub created_at: u64,
    pub updated_at: u64,
    /// SSH runner 的远端 job id（#51；本地 job 恒为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    /// payload 子进程 pid（process identity）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// exec_local runner（`momoctl jobs exec` helper）pid。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_pid: Option<u32>,
    pub log_path: String,
    /// payload 退出码；signal 终止为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    /// failed 的原因（spawn/exit/interrupted 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// unix epoch 毫秒（record 时间戳与 jobs.jsonl ts 共用）。
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 状态机：关键迁移合法、明显非法迁移被拒。
    #[test]
    fn transition_table_matches_doc() {
        use JobStatus::*;
        assert!(Queued.can_transition(Preparing));
        assert!(Queued.can_transition(Canceled));
        assert!(Preparing.can_transition(Running));
        assert!(Running.can_transition(Succeeded));
        assert!(Running.can_transition(Canceling));
        assert!(Canceling.can_transition(Canceled));
        assert!(Failed.can_transition(Preparing));
        assert!(Succeeded.can_transition(Promoted));
        // canceled 是独立终态；running 不得直达 canceled
        assert!(!Running.can_transition(Canceled));
        assert!(!Canceled.can_transition(Failed));
        assert!(!Succeeded.can_transition(Failed));
    }

    /// 终态集合与文档一致。
    #[test]
    fn terminal_states() {
        use JobStatus::*;
        for s in [Succeeded, Failed, Canceled, Promoted] {
            assert!(s.is_terminal(), "{s:?}");
        }
        for s in [Queued, Preparing, Running, Canceling, Recovering] {
            assert!(!s.is_terminal(), "{s:?}");
        }
    }
}
