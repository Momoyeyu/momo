//! `momoctl jobs <submit|list|status|log|cancel>`：job runner 的 CLI 面。
//! `jobs exec --id` 是 submit spawn 的内部 helper（hidden），不是公开操作。
//! 读写同一个 job store（state dir 约定），与 `/api/jobs` 同口径。
//! 退出码：0 成功；1 运行时；2 用法/配置（未知 job id、空命令等）。

use std::io::Write;
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::cli::{JobsAction, JobsArgs, JobsLogArgs};
use crate::jobs::store::{self, JobStore};
use crate::jobs::{runner, JobRecord};
use crate::output::{AppError, ExitCode};

/// `jobs log -f` 轮询间隔（同 serve logs 口径）。
const FOLLOW_POLL: Duration = Duration::from_millis(300);

#[derive(Debug, Serialize)]
struct JobsReport {
    schema: u8,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    job: Option<JobRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    jobs: Option<Vec<JobRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

pub fn run(args: &JobsArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    let store = JobStore::open_default();
    match &args.action {
        JobsAction::Submit(a) => {
            let desc = a.desc.clone().unwrap_or_else(|| a.cmd.join(" "));
            let rec = runner::submit(&store, a.kind, a.cmd.clone(), desc)?;
            emit_job("jobs submit", &rec, None, ctx)?;
            Ok(ExitCode::Ok as i32)
        }
        JobsAction::Exec(a) => runner::exec(&store, &a.id),
        JobsAction::List => {
            let (jobs, warnings) = store.list()?;
            warn_stderr(&warnings);
            if ctx.json {
                let report = JobsReport {
                    schema: 1,
                    command: "jobs list".into(),
                    job: None,
                    jobs: Some(jobs),
                    note: None,
                };
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                render_list(&jobs);
            }
            Ok(ExitCode::Ok as i32)
        }
        JobsAction::Status(a) => {
            let rec = must_load(&store, &a.id)?;
            emit_job("jobs status", &rec, None, ctx)?;
            Ok(ExitCode::Ok as i32)
        }
        JobsAction::Cancel(a) => {
            let (rec, note) = runner::cancel(&store, &a.id)?;
            emit_job("jobs cancel", &rec, Some(note), ctx)?;
            Ok(ExitCode::Ok as i32)
        }
        JobsAction::Log(a) => log(&store, a, ctx),
    }
}

fn must_load(store: &JobStore, id: &str) -> Result<JobRecord, AppError> {
    store
        .load(id)?
        .ok_or_else(|| AppError::Config(format!("job {id} not found")))
}

fn emit_job(
    command: &str,
    rec: &JobRecord,
    note: Option<&str>,
    ctx: &super::Ctx,
) -> Result<(), AppError> {
    if ctx.json {
        let report = JobsReport {
            schema: 1,
            command: command.into(),
            job: Some(rec.clone()),
            jobs: None,
            note: note.map(str::to_string),
        };
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        let mut w = std::io::stdout().lock();
        if let Some(n) = note {
            writeln!(w, "{n}")?;
        }
        render_record(&mut w, rec)?;
    }
    Ok(())
}

/// `jobs log`：tail `logs/<id>.log` 最后 N 行；
/// `-f` 跟随新增直到 job 进终态且日志排空。
fn log(store: &JobStore, args: &JobsLogArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    let rec = must_load(store, &args.id)?;
    let path = store.log_path(&args.id)?;

    let lines = if path.is_file() {
        store::read_tail(&path, args.lines)?
    } else {
        Vec::new()
    };

    if ctx.json {
        let report = serde_json::json!({
            "schema": 1,
            "command": "jobs log",
            "job": rec.id,
            "path": if path.is_file() { Some(path.display().to_string()) } else { None },
            "lines": lines,
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        let mut w = std::io::stdout().lock();
        if path.is_file() {
            writeln!(w, "== {} ({}) ==", rec.id, path.display())?;
        } else {
            writeln!(w, "== {} (no log file) ==", rec.id)?;
        }
        for line in &lines {
            writeln!(w, "{line}")?;
        }
    }

    if args.follow {
        follow(&store.clone(), args, &path, ctx)?;
    }
    Ok(ExitCode::Ok as i32)
}

/// `-f` 跟随：从当前文件末尾增量读，打到 stdout；job 终态且排空后退出。
/// json 模式每批一行 NDJSON `{"lines":[...]}`，收尾一行 `{"done":true,"status":...}`。
fn follow(
    store: &JobStore,
    args: &JobsLogArgs,
    path: &std::path::Path,
    ctx: &super::Ctx,
) -> Result<(), AppError> {
    let mut offset = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    loop {
        let (cur, chunk) = store::read_chunk(path, offset)?;
        offset = cur;
        if !chunk.is_empty() {
            if ctx.json {
                let lines: Vec<&str> = chunk.lines().collect();
                println!("{}", serde_json::json!({"lines": lines}));
            } else {
                let mut w = std::io::stdout().lock();
                write!(w, "{chunk}")?;
                w.flush()?;
            }
        }
        let terminal = store
            .load(&args.id)?
            .is_some_and(|r| r.status.is_terminal());
        if terminal && chunk.is_empty() {
            if ctx.json {
                let status = store
                    .load(&args.id)?
                    .map(|r| r.status.as_str().to_string())
                    .unwrap_or_else(|| "unknown".into());
                println!("{}", serde_json::json!({"done": true, "status": status}));
            }
            return Ok(());
        }
        thread::sleep(FOLLOW_POLL);
    }
}

fn render_list(jobs: &[JobRecord]) {
    let mut w = std::io::stdout().lock();
    if jobs.is_empty() {
        let _ = writeln!(w, "== jobs ==\n  (none)");
        return;
    }
    let _ = writeln!(w, "== jobs ==");
    for r in jobs {
        let exit = r
            .exit
            .map(|c| format!("exit={c}"))
            .unwrap_or_else(|| "-".into());
        let _ = writeln!(
            w,
            "  {}  {:<5}  {:<10}  {:<7}  {:>6}  {}",
            r.id,
            r.kind.as_str(),
            r.status.as_str(),
            exit,
            fmt_age(r.created_at),
            r.desc
        );
    }
}

fn render_record(w: &mut impl Write, r: &JobRecord) -> Result<(), AppError> {
    writeln!(w, "job {}", r.id)?;
    writeln!(w, "  status   {}", r.status.as_str())?;
    writeln!(w, "  kind     {}", r.kind.as_str())?;
    if !r.desc.is_empty() {
        writeln!(w, "  desc     {}", r.desc)?;
    }
    writeln!(w, "  args     {}", r.args.join(" "))?;
    writeln!(w, "  created  {} ago", fmt_age(r.created_at))?;
    if let Some(pid) = r.pid {
        writeln!(w, "  pid      {pid}")?;
    }
    if let Some(rp) = r.runner_pid {
        writeln!(w, "  runner   pid {rp}")?;
    }
    writeln!(w, "  log      {}", r.log_path)?;
    if let Some(c) = r.exit {
        writeln!(w, "  exit     {c}")?;
    }
    if let Some(e) = &r.error {
        writeln!(w, "  error    {e}")?;
    }
    Ok(())
}

/// epoch 毫秒 → 紧凑「多久前」（`12s`/`3m`/`2h`/`1d`）。
fn fmt_age(created_ms: u64) -> String {
    let now = crate::jobs::now_ms();
    let s = now.saturating_sub(created_ms) / 1000;
    if s >= 86400 {
        format!("{}d", s / 86400)
    } else if s >= 3600 {
        format!("{}h", s / 3600)
    } else if s >= 60 {
        format!("{}m", s / 60)
    } else {
        format!("{s}s")
    }
}

fn warn_stderr(warnings: &[String]) {
    for w in warnings {
        eprintln!("warn: {w}");
    }
}
