//! spawn/kill/pidfile/日志落盘。
//! 状态目录固定 XDG 默认：`${XDG_STATE_HOME:-~/.local/state}/momo/`
//! （pid/、logs/、active_profile），不提供 env 覆盖（momo-infra.md）。

use std::fs;
use std::path::PathBuf;

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

/// pid 存活探测：unix 走 `kill -0`；其他平台保守返回 false。
pub fn is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// `PATH` 查找可执行文件（momod 定位；`--app-bin` 优先于此）。
pub fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|p| p.is_file())
}
