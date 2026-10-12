//! spawn/kill/pidfile/日志落盘。
//! 状态目录固定 XDG 默认：`${XDG_STATE_HOME:-~/.local/state}/momo/`
//! （pid/、logs/、active_profile），不提供 env 覆盖（momo-infra.md）。

use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};

use crate::output::AppError;

/// `${XDG_STATE_HOME:-~/.local/state}/momo`
pub fn state_dir() -> PathBuf {
    match std::env::var("XDG_STATE_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v).join("momo"),
        _ => crate::env_cfg::expand_tilde("~/.local/state/momo"),
    }
}

pub fn pid_dir() -> PathBuf {
    state_dir().join("pid")
}

pub fn log_dir() -> PathBuf {
    state_dir().join("logs")
}

/// `active_profile` 状态文件（switch 写，serve 读）。
pub fn active_profile_file() -> PathBuf {
    state_dir().join("active_profile")
}

pub fn pidfile(component: &str) -> PathBuf {
    pid_dir().join(format!("{component}.pid"))
}

pub fn read_pidfile(component: &str) -> Result<u32, AppError> {
    let text = fs::read_to_string(pidfile(component))?;
    text.trim()
        .parse()
        .map_err(|e| AppError::Config(format!("bad pidfile for {component}: {e}")))
}

pub fn write_pidfile(component: &str, pid: u32) -> Result<(), AppError> {
    fs::create_dir_all(pid_dir())?;
    fs::write(pidfile(component), format!("{pid}\n"))?;
    Ok(())
}

pub fn remove_pidfile(component: &str) -> Result<(), AppError> {
    let p = pidfile(component);
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}

/// 组件日志落盘：`logs/<name>.log`，stdout/stderr 追加写同一文件。
pub fn log_stdio(component: &str) -> Result<(PathBuf, Stdio, Stdio), AppError> {
    fs::create_dir_all(log_dir())?;
    let path = log_dir().join(format!("{component}.log"));
    let out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let err = out.try_clone()?;
    Ok((path, Stdio::from(out), Stdio::from(err)))
}

/// 优雅停止：先发 SIGTERM（win: taskkill），`grace` 内轮询存活；
/// 未退出再 SIGKILL（win: taskkill /F）。返回进程最终已退出。
pub fn stop_pid(pid: u32, grace: Duration) -> bool {
    if !is_alive(pid) {
        return true;
    }
    term_pid(pid);
    if wait_dead(pid, grace) {
        return true;
    }
    force_pid(pid);
    wait_dead(pid, Duration::from_secs(2))
}

/// 单次终止信号：unix `kill`（SIGTERM），windows `taskkill /F`，其他平台 no-op。
pub fn kill_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        signal(pid, None)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// 停止前置信号：unix SIGTERM；windows `taskkill /PID`（不带 /F 的温和请求）。
fn term_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        signal(pid, None)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// 升级强杀：unix `kill -9`；windows `taskkill /PID /F`。
fn force_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        signal(pid, Some("-9"))
    }
    #[cfg(windows)]
    {
        kill_pid(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[cfg(unix)]
fn signal(pid: u32, sig: Option<&str>) -> bool {
    let mut cmd = std::process::Command::new("kill");
    if let Some(s) = sig {
        cmd.arg(s);
    }
    cmd.arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn wait_dead(pid: u32, timeout: Duration) -> bool {
    let start = Instant::now();
    while is_alive(pid) {
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

/// pid 存活探测：unix `kill -0` + `ps stat` 排僵尸（kill -0 对僵尸仍成功）；
/// windows `tasklist`；其他平台保守返回 false。
pub fn is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let signaled = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !signaled {
            return false;
        }
        // 僵尸进程 kill -0 仍成功：ps stat 以 Z 开头判死
        // （同进程内 kill 自己 spawn 的 child 未 wait 时常见）。
        match std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
        {
            Ok(o) => {
                let stat = String::from_utf8_lossy(&o.stdout);
                !stat.trim_start().starts_with('Z')
            }
            Err(_) => true,
        }
    }
    #[cfg(windows)]
    {
        // tasklist CSV 输出含 "<pid>" 即存活。
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// pidfile 创建时间 ≈ 组件启动时间（serve status 报 uptime 用）。
pub fn pidfile_started(component: &str) -> Option<SystemTime> {
    let md = fs::metadata(pidfile(component)).ok()?;
    md.created().ok().or_else(|| md.modified().ok())
}

/// `PATH` 查找可执行文件（momod 定位；`--app-bin` 优先于此）。
pub fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|p| p.is_file())
}
