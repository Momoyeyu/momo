//! exec_local 执行器与 job 生命周期入口。
//!
//! 模型：`momoctl jobs submit` 建档（queued）后 spawn 一个 detached helper
//! （`momoctl jobs exec --id <id>`）。helper 是状态机的唯一写入方：
//! `queued → preparing → running → succeeded|failed|canceled`，
//! payload 的 stdout/stderr 增量并入 `logs/<id>.log`，payload pid 记为
//! process identity，exit code 定 succeeded/failed。
//! cancel（`jobs cancel` / API）写 `canceling` 并 kill payload pid；
//! 确认停止的收尾（→ canceled）由 helper 的 wait 返回或 cancel 自身完成。
//!
//! helper detached 运行：momoctl 主进程退出后 job 继续；web server 与 CLI
//! 读写同一个 store，不共享内存态。

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::jobs::store::JobStore;
use crate::jobs::{JobKind, JobRecord, JobStatus};
use crate::output::{AppError, ExitCode};
use crate::serve::proc;

/// cancel 时 SIGTERM 后的宽限期（与 serve down 同口径），超时升级强杀。
const TERM_GRACE: Duration = Duration::from_secs(10);

/// 提交 job：建档(queued) → spawn detached helper。helper 接管后续迁移，
/// submit 立即返回（`jobs status`/`log -f` 跟进）。
pub fn submit(
    store: &JobStore,
    kind: JobKind,
    args: Vec<String>,
    desc: String,
) -> Result<JobRecord, AppError> {
    if args.is_empty() || args[0].trim().is_empty() {
        return Err(AppError::Config(
            "jobs submit: empty command (expected `-- CMD [ARGS...]`)".into(),
        ));
    }
    let mut rec = store.create(kind, args, desc)?;
    if let Err(e) = spawn_helper(store, &rec.id) {
        let detail = format!("runner spawn failed: {e}");
        // helper 起不来：直接收口 failed，不留 queued 孤儿。
        let _ = store.transition(&mut rec, JobStatus::Failed, Some(detail));
        return Err(AppError::Io(e));
    }
    Ok(rec)
}

/// spawn `momoctl jobs exec --id <id>`：stdin 置空 detach；
/// stdout/stderr 并入 job 日志（runner 诊断与 panic 不落终端）。
fn spawn_helper(store: &JobStore, id: &str) -> io::Result<Child> {
    let exe = std::env::current_exe()?;
    let log = store.log_path(id).map_err(io::Error::other)?;
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = OpenOptions::new().create(true).append(true).open(&log)?;
    let err = out.try_clone()?;
    Command::new(exe)
        .args(["jobs", "exec", id])
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
}

/// helper 主流程（exec_local）。只处理 queued；其他状态直接退出
/// ——状态是唯一的 claim，绝不重复 launch。
/// 返回值是 runner 自身的退出码（payload 成败写入 store，不回传）。
pub fn exec(store: &JobStore, id: &str) -> Result<i32, AppError> {
    let mut rec = match store.load(id)? {
        Some(r) => r,
        None => {
            return Err(AppError::Config(format!("job {id} not found")));
        }
    };
    if rec.status != JobStatus::Queued {
        return Ok(ExitCode::Ok as i32);
    }

    // claim：写入 runner_pid + preparing；写后回读确认 claim 未被其他
    // runner 覆盖（两个 exec 并发时 last-writer-wins，输家自行退出）。
    rec.runner_pid = Some(std::process::id());
    store.transition(&mut rec, JobStatus::Preparing, None)?;
    if store
        .load(id)?
        .and_then(|r| r.runner_pid)
        .is_some_and(|p| p != std::process::id())
    {
        return Ok(ExitCode::Ok as i32);
    }

    // launch 前复查 cancel（submit→spawn 窗口内可能已取消）。
    if store
        .load(id)?
        .is_some_and(|r| r.status.is_terminal() || r.status == JobStatus::Canceling)
    {
        store.transition(
            &mut rec,
            JobStatus::Canceled,
            Some("canceled before launch".into()),
        )?;
        runner_log(&rec, "canceled before launch");
        return Ok(ExitCode::Ok as i32);
    }

    let spawned = Command::new(&rec.args[0])
        .args(&rec.args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            let detail = format!("spawn failed: {}: {e}", rec.args[0]);
            runner_log(&rec, &detail);
            store.transition(&mut rec, JobStatus::Failed, Some(detail))?;
            return Ok(ExitCode::Ok as i32);
        }
    };
    rec.pid = Some(child.id());
    store.transition(&mut rec, JobStatus::Running, None)?;
    runner_log(&rec, &format!("spawned pid {}", child.id()));

    // spawn 后复查：cancel 落在 preparing→running 窗口时自行补杀。
    if store
        .load(id)?
        .is_some_and(|r| matches!(r.status, JobStatus::Canceling | JobStatus::Canceled))
    {
        let _ = proc::stop_pid(child.id(), TERM_GRACE);
    }

    // stdout/stderr 各起一个 pump 线程，增量并入日志文件。
    let pumps = spawn_pumps(&mut child, &rec.log_path);
    let waited = child.wait();
    for p in pumps {
        let _ = p.join();
    }

    let outcome = match &waited {
        Ok(es) => {
            rec.exit = es.code();
            match es.code() {
                Some(c) => format!("exited (code {c})"),
                None => "terminated by signal".into(),
            }
        }
        Err(e) => format!("wait failed: {e}"),
    };
    runner_log(&rec, &outcome);

    // 收尾以最新 meta 为准：cancel 的 canceling/canceled 不得被 exit code 覆盖。
    let mut fresh = store.load(id)?.unwrap_or_else(|| rec.clone());
    fresh.exit = rec.exit;
    match fresh.status {
        JobStatus::Canceling | JobStatus::Canceled => {
            if fresh.status != JobStatus::Canceled {
                store.transition(
                    &mut fresh,
                    JobStatus::Canceled,
                    Some("process stopped".into()),
                )?;
            }
        }
        s if s.is_terminal() => {
            // 已被外部收口（罕见）：只补 exit 字段。
            store.save(&fresh)?;
        }
        _ => match &waited {
            Ok(es) if es.success() => {
                store.transition(&mut fresh, JobStatus::Succeeded, None)?;
            }
            Ok(es) => {
                let detail = match es.code() {
                    Some(c) => format!("exited with code {c}"),
                    None => "terminated by signal".into(),
                };
                store.transition(&mut fresh, JobStatus::Failed, Some(detail))?;
            }
            Err(e) => {
                store.transition(
                    &mut fresh,
                    JobStatus::Failed,
                    Some(format!("wait failed: {e}")),
                )?;
            }
        },
    }
    Ok(ExitCode::Ok as i32)
}

/// payload stdout/stderr → `logs/<id>.log` 追加（O_APPEND 多写者安全）。
fn spawn_pumps(child: &mut Child, log_path: &str) -> Vec<JoinHandle<()>> {
    let mut pumps = Vec::new();
    if let Some(stream) = child.stdout.take() {
        pumps.push(pump(stream, log_path.to_string()));
    }
    if let Some(stream) = child.stderr.take() {
        pumps.push(pump(stream, log_path.to_string()));
    }
    pumps
}

fn pump(mut stream: impl io::Read + Send + 'static, log_path: String) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut w = match OpenOptions::new().create(true).append(true).open(&log_path) {
            Ok(w) => w,
            Err(_) => return,
        };
        let _ = io::copy(&mut stream, &mut w);
        let _ = w.flush();
    })
}

/// runner 侧诊断行：与 payload 输出同文件，前缀 `[runner]` 区分。
fn runner_log(rec: &JobRecord, line: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rec.log_path)
    {
        let _ = writeln!(f, "[runner] {line}");
    }
}

/// cancel：非终态 → `canceling` + kill payload pid；runner 缺席时由本函数
/// 确认停止后写 `canceled`。终态幂等返回。
/// 返回 (record, note)——note 给 human 输出描述结果。
pub fn cancel(store: &JobStore, id: &str) -> Result<(JobRecord, &'static str), AppError> {
    let mut rec = store
        .load(id)?
        .ok_or_else(|| AppError::Config(format!("job {id} not found")))?;
    if rec.status.is_terminal() {
        return Ok((rec, "already terminal"));
    }
    if rec.status == JobStatus::Recovering {
        return Ok((
            rec,
            "cannot confirm process identity (recovering); not canceled",
        ));
    }

    if rec.status != JobStatus::Canceling {
        store.transition(
            &mut rec,
            JobStatus::Canceling,
            Some("cancel requested".into()),
        )?;
    }

    let note = match rec.pid {
        Some(pid) if proc::is_alive(pid) => {
            if proc::stop_pid(pid, TERM_GRACE) {
                "process stopped"
            } else {
                "process did not exit after TERM+KILL; still canceling"
            }
        }
        _ => "cancel requested",
    };

    // 收尾：重读 meta，进程已确认停止且仍是 canceling → 落 canceled。
    // runner 在场时它的 wait 也会收敛到 canceled，两条路径殊途同归。
    if let Some(mut fresh) = store.load(id)? {
        let payload_dead = fresh.pid.map(|p| !proc::is_alive(p)).unwrap_or(true);
        if fresh.status == JobStatus::Canceling && payload_dead {
            store.transition(
                &mut fresh,
                JobStatus::Canceled,
                Some("process confirmed stopped".into()),
            )?;
        }
        rec = fresh;
    }
    Ok((rec, note))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::store::JobStore;
    use std::fs;
    use std::path::PathBuf;

    fn store(tag: &str) -> (JobStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("momo-runner-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        (JobStore::new(dir.clone()), dir)
    }

    /// `exec` 直接驱动 queued job：不存在的程序 → failed（spawn failed）。
    #[test]
    fn exec_marks_spawn_failure() {
        let (s, dir) = store("spawnfail");
        let rec = s
            .create(
                JobKind::Train,
                vec!["momo-no-such-bin-42".into()],
                String::new(),
            )
            .unwrap();
        assert_eq!(exec(&s, &rec.id).unwrap(), 0);
        let done = s.load(&rec.id).unwrap().unwrap();
        assert_eq!(done.status, JobStatus::Failed);
        assert!(done.error.unwrap().contains("spawn failed"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// exec_local 全链路：re-exec 当前测试二进制当 payload（平台无关）。
    /// payload 打印一行即退出 → succeeded + exit 0 + 日志捕获。
    #[test]
    fn exec_runs_payload_to_success() {
        let (s, dir) = store("okpath");
        let exe = std::env::current_exe().unwrap();
        // `--exact` 单测名：re-exec 测试二进制只跑一个秒过的测试函数。
        let rec = s
            .create(
                JobKind::Train,
                vec![
                    exe.display().to_string(),
                    "--exact".into(),
                    "jobs::runner::tests::payload_entry".into(),
                    "--nocapture".into(),
                ],
                String::new(),
            )
            .unwrap();
        std::env::set_var("MOMO_TEST_PAYLOAD", "1");
        assert_eq!(exec(&s, &rec.id).unwrap(), 0);
        let done = s.load(&rec.id).unwrap().unwrap();
        assert_eq!(done.status, JobStatus::Succeeded);
        assert_eq!(done.exit, Some(0));
        let log = fs::read_to_string(&done.log_path).unwrap();
        assert!(log.contains("MOMO_PAYLOAD_OK"), "log: {log}");
        std::env::remove_var("MOMO_TEST_PAYLOAD");
        let _ = fs::remove_dir_all(&dir);
    }

    /// payload 入口：被 re-exec 时打标记行退出；正常跑到时直接通过。
    #[test]
    fn payload_entry() {
        if std::env::var_os("MOMO_TEST_PAYLOAD").is_some() {
            println!("MOMO_PAYLOAD_OK");
        }
    }

    /// 非 queued 状态的 exec 是 no-op（绝不重复 launch）。
    #[test]
    fn exec_is_noop_for_nonqueued() {
        let (s, dir) = store("noop");
        let mut rec = s
            .create(JobKind::Train, vec!["x".into()], String::new())
            .unwrap();
        s.transition(&mut rec, JobStatus::Canceled, None).unwrap();
        assert_eq!(exec(&s, &rec.id).unwrap(), 0);
        assert_eq!(
            s.load(&rec.id).unwrap().unwrap().status,
            JobStatus::Canceled
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// cancel 终态幂等；queued（无 runner）→ canceled。
    #[test]
    fn cancel_terminal_idempotent_and_queued() {
        let (s, dir) = store("cancel");
        let mut rec = s
            .create(JobKind::Train, vec!["x".into()], String::new())
            .unwrap();
        let (r, _note) = cancel(&s, &rec.id).unwrap();
        assert_eq!(r.status, JobStatus::Canceled);
        // 幂等：再 cancel 不报错
        let (r2, note) = cancel(&s, &rec.id).unwrap();
        assert_eq!(r2.status, JobStatus::Canceled);
        assert_eq!(note, "already terminal");
        // 未知 id → usage 错误
        assert!(cancel(&s, "01JUNKNOWN").is_err());
        rec.status = JobStatus::Succeeded;
        let _ = fs::remove_dir_all(&dir);
    }
}
