//! `momoctl serve <up|down|status|logs|restart> [--profile X] [--only C] [--app-bin P]`。
//! `up` 已实装：profile 组件表解析 → 按 order 依序 spawn → pidfile + 日志落盘，
//! 带 `ready` URL 的组件做 bounded readiness 轮询；失败即清理本次 spawn 的子进程
//! （momo-infra.md service 操作流程）。external 组件只探活不拉起。
//! down/status/logs/restart 仍 stub。

use std::io::Write;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::cli::{ServeAction, ServeArgs, ServeCommon};
use crate::env_cfg::{self, EnvCfg};
use crate::net;
use crate::output::{not_implemented, AppError, Check, CheckStatus, ExitCode, Summary};
use crate::serve::proc;
use crate::serve::profile::{Component, ServeProfile};

/// bounded readiness 上限（momo-infra.md：bounded readiness 通过才算 active）。
const READY_TIMEOUT: Duration = Duration::from_secs(15);
const READY_POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Serialize)]
struct ServeReport {
    schema: u8,
    command: String,
    profile: String,
    checks: Vec<Check>,
    summary: Summary,
}

pub fn run(args: &ServeArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    let (action, common) = split(&args.action);
    if action == "up" {
        let profile = EnvCfg.profile(common.profile);
        let doc = ServeProfile::load_builtin(profile.as_str())?;
        return up(&doc, common, profile.as_str(), ctx);
    }
    Ok(not_implemented(&format!("serve {action}"), ctx.json))
}

/// `serve up`：按 start_order 逐个拉起；任一 fail → 停止本次 owned children。
fn up(
    doc: &ServeProfile,
    common: &ServeCommon,
    profile: &str,
    ctx: &super::Ctx,
) -> Result<i32, AppError> {
    let llm = EnvCfg.llm_endpoint();
    let selected: Vec<&Component> = doc
        .start_order()
        .into_iter()
        .filter(|c| common.only.is_empty() || common.only.iter().any(|n| n == &c.name))
        .collect();
    if selected.is_empty() {
        return Err(AppError::Config(format!(
            "profile '{profile}': no component matches --only {:?}",
            common.only
        )));
    }

    let mut checks: Vec<Check> = Vec::new();
    let mut owned: Vec<(String, Child)> = Vec::new();
    let mut aborted = false;

    for c in selected {
        if aborted {
            break;
        }
        // external / 条件跳过：探活或标记。
        if !doc.should_spawn(c, llm.as_deref()) {
            if c.external {
                match probe_external(c) {
                    Ok(url) => checks.push(Check::ok(
                        c.name.clone(),
                        format!("external probe ok: {url}"),
                    )),
                    Err(detail) => {
                        checks.push(Check::fail(
                            c.name.clone(),
                            detail,
                            "外部 provider 不可达：检查端点/网络后重试 serve up",
                        ));
                        aborted = true;
                    }
                }
            } else {
                checks.push(Check::warn(
                    c.name.clone(),
                    "skipped: MOMO_LLM_ENDPOINT is not loopback",
                    "local_endpoint_only 组件仅在本地大脑档下拉起",
                ));
            }
            continue;
        }

        // 幂等：pidfile 指向的活进程不再拉起（momo-infra.md：同 identity 运行 → no-op）。
        if let Ok(pid) = proc::read_pidfile(&c.name) {
            if proc::is_alive(pid) {
                checks.push(Check::ok(
                    c.name.clone(),
                    format!("already running (pid {pid})"),
                ));
                continue;
            }
            let _ = proc::remove_pidfile(&c.name);
        }

        match spawn_component(c, common) {
            Ok((child, log_path)) => {
                let pid = child.id();
                proc::write_pidfile(&c.name, pid)?;
                owned.push((c.name.clone(), child));
                let check = match &c.ready {
                    Some(url) => {
                        let (_, child) = owned.last_mut().unwrap();
                        let (ready, exited) = wait_ready(url, child);
                        if ready {
                            Check::ok(
                                c.name.clone(),
                                format!(
                                    "spawned pid {pid}, ready: {url}, log {}",
                                    log_path.display()
                                ),
                            )
                        } else {
                            let why = if exited {
                                "exited before ready"
                            } else {
                                "readiness timeout"
                            };
                            Check::fail(
                                c.name.clone(),
                                format!("spawned pid {pid} but {why}: {url}"),
                                format!("查看 {}", log_path.display()),
                            )
                        }
                    }
                    None => Check::ok(
                        c.name.clone(),
                        format!("spawned pid {pid}, log {}", log_path.display()),
                    ),
                };
                if check.status == CheckStatus::Fail {
                    aborted = true;
                }
                checks.push(check);
            }
            Err(e) => {
                checks.push(Check::fail(
                    c.name.clone(),
                    format!("spawn failed: {e}"),
                    "检查 cmd/cwd；momod 可用 --app-bin 指定二进制",
                ));
                aborted = true;
            }
        }
    }

    // readiness/spawn 失败 → 停掉本次 owned children，attached providers 保留。
    if aborted {
        for (name, mut child) in owned.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = proc::remove_pidfile(&name);
        }
    } else {
        // 标记 active（switch/serve status 读）。
        if let Some(parent) = proc::active_profile_file().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(proc::active_profile_file(), format!("{profile}\n"));
    }

    let mut summary = Summary {
        ok: 0,
        warn: 0,
        fail: 0,
    };
    for c in &checks {
        match c.status {
            CheckStatus::Ok => summary.ok += 1,
            CheckStatus::Warn => summary.warn += 1,
            CheckStatus::Fail => summary.fail += 1,
        }
    }
    let report = ServeReport {
        schema: 1,
        command: "serve up".into(),
        profile: profile.into(),
        checks,
        summary,
    };
    render(&report, ctx)?;

    Ok(if aborted {
        ExitCode::Runtime as i32
    } else {
        ExitCode::Ok as i32
    })
}

/// 拉起单个组件：展开 args → 叠 env（含 HF_ENDPOINT 默认注入）→ spawn。
/// stdout/stderr 重定向到 `logs/<name>.log`；返回 Child 与日志路径。
fn spawn_component(
    c: &Component,
    common: &ServeCommon,
) -> Result<(Child, std::path::PathBuf), AppError> {
    let program = if c.name == "momod" {
        common
            .app_bin
            .clone()
            .map(|p| p.display().to_string())
            .or_else(|| c.cmd.clone())
    } else {
        c.cmd.clone()
    }
    .ok_or_else(|| AppError::Config(format!("component '{}': no cmd", c.name)))?;

    let (log_path, out, err) = proc::log_stdio(&c.name)?;

    let mut cmd = Command::new(&program);
    cmd.args(ServeProfile::expand_args(c))
        .stdout(out)
        .stderr(err)
        // 子进程 detach：stdin 置空，momoctl 退出后组件继续运行。
        .stdin(std::process::Stdio::null());

    // config.md §2.5：infra 负责向 Python 服务注入 HF_ENDPOINT（默认 hf-mirror）。
    if std::env::var_os("HF_ENDPOINT").is_none() && !c.env.contains_key("HF_ENDPOINT") {
        cmd.env("HF_ENDPOINT", "https://hf-mirror.com");
    }
    for (k, v) in &c.env {
        cmd.env(k, v);
    }
    if let Some(cwd) = &c.cwd {
        let dir = env_cfg::expand_tilde(cwd);
        if !dir.is_dir() {
            return Err(AppError::Config(format!(
                "component '{}': cwd {} not found",
                c.name,
                dir.display()
            )));
        }
        cmd.current_dir(dir);
    }

    cmd.spawn()
        .map(|child| (child, log_path))
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("{program}: {e}"))))
}

/// external 组件探活：`probe_env` 端点 + `probe_path` 拼 URL，GET 探 2xx。
fn probe_external(c: &Component) -> Result<String, String> {
    let base = c
        .probe_env
        .as_deref()
        .and_then(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .map(|ep| ep.trim_end_matches('/').to_string());
    let url = match (base, &c.probe_path, &c.ready) {
        (Some(ep), Some(path), _) => format!("{ep}{path}"),
        (Some(ep), None, _) => ep,
        (None, _, Some(ready)) => ready.clone(),
        (None, _, None) => {
            return Err(format!(
                "{} unset",
                c.probe_env.as_deref().unwrap_or("endpoint env")
            ))
        }
    };
    let p = net::probe_get(&url);
    if p.ready() {
        Ok(format!("{url} → {}", p.describe()))
    } else {
        Err(format!("{url} → {}", p.describe()))
    }
}

/// bounded readiness：轮询 GET 至 2xx；子进程提前退出立即返回。
/// 返回 (ready, exited)。
fn wait_ready(url: &str, child: &mut Child) -> (bool, bool) {
    let start = Instant::now();
    loop {
        if net::probe_get(url).ready() {
            return (true, false);
        }
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return (false, true),
            Ok(None) => {}
        }
        if start.elapsed() >= READY_TIMEOUT {
            return (false, false);
        }
        thread::sleep(READY_POLL);
    }
}

fn render(report: &ServeReport, ctx: &super::Ctx) -> Result<(), AppError> {
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return Ok(());
    }
    let mut w = std::io::stdout().lock();
    writeln!(w, "== serve up (profile={}) ==", report.profile)?;
    let id_width = report.checks.iter().map(|c| c.id.len()).max().unwrap_or(0);
    for c in &report.checks {
        let label = match c.status {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        };
        let pad = " ".repeat(4usize.saturating_sub(label.len()));
        writeln!(w, "  {pad}{label}  {:id_width$}  {}", c.id, c.detail)?;
        if let Some(hint) = &c.hint {
            writeln!(w, "  {pad}      {:id_width$}  hint: {hint}", "")?;
        }
    }
    writeln!(
        w,
        "summary: {} ok, {} warn, {} fail",
        report.summary.ok, report.summary.warn, report.summary.fail
    )?;
    Ok(())
}

fn split(action: &ServeAction) -> (&'static str, &ServeCommon) {
    match action {
        ServeAction::Up(c) => ("up", c),
        ServeAction::Down(c) => ("down", c),
        ServeAction::Status(c) => ("status", c),
        ServeAction::Logs(c) => ("logs", c),
        ServeAction::Restart(c) => ("restart", c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    /// env 变量（XDG_STATE_HOME/MOMO_*）进程级共享：测试串行化。
    static LOCK: Mutex<()> = Mutex::new(());

    fn test_ctx() -> super::super::Ctx {
        super::super::Ctx {
            json: false,
            verbose: 0,
        }
    }

    fn common() -> ServeCommon {
        ServeCommon {
            profile: None,
            only: vec![],
            app_bin: None,
        }
    }

    /// 隔离状态目录 + 返回其路径（XDG_STATE_HOME 约定）。
    fn state_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("momo-serve-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        std::env::set_var("XDG_STATE_HOME", &dir);
        dir.join("momo")
    }

    /// 跨平台长驻进程 stub：python3 是 doctor 必需工具。
    const SLEEPER: &str = r#"
[profile]
name = "t"

[[component]]
name = "stub"
order = 10
cmd = "python3"
args = ["-c", "import time; time.sleep(60)"]
"#;

    #[test]
    fn up_spawns_pidfile_and_log() {
        let _g = LOCK.lock().unwrap();
        let state = state_dir("spawn");
        let doc = ServeProfile::parse(SLEEPER).unwrap();
        let code = up(&doc, &common(), "t", &test_ctx()).unwrap();
        assert_eq!(code, 0);

        let pid = proc::read_pidfile("stub").unwrap();
        assert!(proc::is_alive(pid), "spawned process must be alive");
        assert!(state.join("logs/stub.log").exists());
        assert!(state.join("active_profile").exists());

        // 幂等：再次 up → already running，不重复拉起
        let code = up(&doc, &common(), "t", &test_ctx()).unwrap();
        assert_eq!(code, 0);
        assert_eq!(proc::read_pidfile("stub").unwrap(), pid);

        assert!(proc::kill_pid(pid));
        let _ = proc::remove_pidfile("stub");
        std::env::remove_var("XDG_STATE_HOME");
        let _ = fs::remove_dir_all(state.parent().unwrap());
    }

    #[test]
    fn failed_external_probe_stops_owned_children() {
        let _g = LOCK.lock().unwrap();
        let state = state_dir("cleanup");
        // stub 先拉起，随后的 external 探活指向无监听端口 → fail → 清理 stub。
        std::env::set_var("MOMO_TEST_EP", "http://127.0.0.1:1");
        let toml = r#"
[profile]
name = "t"

[[component]]
name = "stub"
order = 10
cmd = "python3"
args = ["-c", "import time; time.sleep(60)"]

[[component]]
name = "brain"
order = 20
external = true
probe_env = "MOMO_TEST_EP"
probe_path = "/models"
"#;
        let doc = ServeProfile::parse(toml).unwrap();
        let code = up(&doc, &common(), "t", &test_ctx()).unwrap();
        assert_eq!(code, ExitCode::Runtime as i32);

        // 本次 spawn 的 stub 已被回收：pidfile 删除、进程终止。
        assert!(proc::read_pidfile("stub").is_err());
        std::env::remove_var("MOMO_TEST_EP");
        std::env::remove_var("XDG_STATE_HOME");
        let _ = fs::remove_dir_all(state.parent().unwrap());
    }

    #[test]
    fn only_unknown_component_is_usage_error() {
        let _g = LOCK.lock().unwrap();
        let doc = ServeProfile::parse(SLEEPER).unwrap();
        let c = ServeCommon {
            profile: None,
            only: vec!["nope".into()],
            app_bin: None,
        };
        assert!(up(&doc, &c, "t", &test_ctx()).is_err());
    }

    #[test]
    fn local_endpoint_only_skipped_off_loopback() {
        let _g = LOCK.lock().unwrap();
        let state = state_dir("skip");
        std::env::set_var("MOMO_LLM_ENDPOINT", "https://api.deepseek.com/v1");
        let toml = r#"
[profile]
name = "t"

[[component]]
name = "llama-server"
order = 5
local_endpoint_only = true
cmd = "llama-server-nonexistent-bin"

[[component]]
name = "stub"
order = 10
cmd = "python3"
args = ["-c", "import time; time.sleep(60)"]
"#;
        let doc = ServeProfile::parse(toml).unwrap();
        let code = up(&doc, &common(), "t", &test_ctx()).unwrap();
        assert_eq!(code, 0);
        // llama-server 跳过未拉起，stub 正常 spawn
        assert!(proc::read_pidfile("llama-server").is_err());
        let pid = proc::read_pidfile("stub").unwrap();
        assert!(proc::kill_pid(pid));
        std::env::remove_var("MOMO_LLM_ENDPOINT");
        std::env::remove_var("XDG_STATE_HOME");
        let _ = fs::remove_dir_all(state.parent().unwrap());
    }
}
