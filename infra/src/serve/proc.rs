//! spawn/kill/pidfile/日志落盘。
//! 状态目录：`${XDG_STATE_HOME:-~/.local/state}/momo/`（pid/、logs/、active_profile），
//! 不提供 `MOMO_*` 覆盖键（口径见 momo-infra.md「状态目录与进程管理」）。
//! 探活/终止走系统调用（unix `waitpid`+`kill(2)`；windows `OpenProcess`/`TerminateProcess`），
//! 不依赖 PATH 上的外部命令（kill/ps/tasklist 均已移除）。

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

/// 优雅停止：先发 SIGTERM（win: taskkill 温和请求），`grace` 内轮询存活；
/// 未退出再 SIGKILL（win: TerminateProcess）。返回进程最终已退出。
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

/// 单次终止信号：unix SIGTERM；windows `TerminateProcess`；其他平台 no-op。
pub fn kill_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        send_signal(pid, libc::SIGTERM)
    }
    #[cfg(windows)]
    {
        terminate_process(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// 停止前置信号：unix SIGTERM；windows `taskkill /PID`（不带 /F 的温和请求）。
/// windows 侧 taskkill 缺失仅失去温和请求，`stop_pid` 仍会升级 TerminateProcess。
fn term_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        send_signal(pid, libc::SIGTERM)
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

/// 升级强杀：unix SIGKILL；windows `TerminateProcess`（不依赖 taskkill）。
fn force_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        send_signal(pid, libc::SIGKILL)
    }
    #[cfg(windows)]
    {
        terminate_process(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// unix `kill(2)` 封装：返回信号是否送达（EPERM 即「存在但无权限」也算未送达，
/// 由 `is_alive` 的 kill(pid,0) 分支另行判定存活）。
/// `kill(2)` 的 pid<=0 是进程组语义（0=本组、负数=进程组），不是 pid 语义：拒绝。
#[cfg(unix)]
fn send_signal(pid: u32, sig: i32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    unsafe { libc::kill(pid as i32, sig) == 0 }
}

/// windows 强杀原语：`OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess`。
/// 打不开句柄（无此进程/无权限）→ false。
#[cfg(windows)]
fn terminate_process(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h == 0 {
            return false;
        }
        let ok = TerminateProcess(h, 1);
        CloseHandle(h);
        ok != 0
    }
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

/// pid 存活探测，不依赖外部命令。
///
/// unix：
/// 1. `waitpid(WNOHANG)` 先试收本进程 child 的尸——返回 pid 表示 child
///    已退出（收尸完成）→ 死；返回 0 表示 child 仍在运行 → 活；
///    -1（ECHILD 等，非本进程 child）→ 落到下一步。
///    这一步替代旧的 `ps stat` 排僵尸（kill(pid,0) 对僵尸仍成功）。
/// 2. `kill(pid, 0)`：送达 → 活；`EPERM`（存在但属其他用户）→ 活；
///    `ESRCH` 及其余 errno → 死。
///
/// windows：`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` 打不开时
/// `GetLastError()==ERROR_ACCESS_DENIED`（存在但无权限）→ 活，其余 → 死；
/// 打开后 `GetExitCodeProcess`：`STILL_ACTIVE` → 活，拿到退出码 → 死，
/// 查询失败保守判活（宁 down 不收敛，勿 up 重复拉起）。
///
/// 其他平台保守返回 false。
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        if pid > i32::MAX as u32 {
            return false;
        }
        unsafe {
            match libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) {
                r if r == pid as i32 => return false,
                0 => return true,
                _ => {}
            }
            if libc::kill(pid as i32, 0) == 0 {
                return true;
            }
            std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            CloseHandle, GetLastError, ERROR_ACCESS_DENIED, STILL_ACTIVE,
        };
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h == 0 {
                return GetLastError() == ERROR_ACCESS_DENIED;
            }
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(h, &mut code);
            CloseHandle(h);
            if ok == 0 {
                return true;
            }
            code == STILL_ACTIVE as u32
        }
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
/// windows 可执行文件带 `.exe` 后缀：`bin` 未以 `.exe` 结尾时追加 `bin.exe` 探测。
pub fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| candidates(dir, bin))
        .find(|p| p.is_file())
}

/// 各平台候选文件名：unix 仅 `bin`；windows 追加 `.exe`（已带 .exe 不重复探测）。
#[cfg(windows)]
fn candidates(dir: PathBuf, bin: &str) -> Vec<PathBuf> {
    let mut v = vec![dir.join(bin)];
    if !bin.to_ascii_lowercase().ends_with(".exe") {
        v.push(dir.join(format!("{bin}.exe")));
    }
    v
}

/// unix 无扩展名探测：可执行判定靠文件存在（权限位由 spawn 阶段兜底）。
#[cfg(not(windows))]
fn candidates(dir: PathBuf, bin: &str) -> Vec<PathBuf> {
    vec![dir.join(bin)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自身 pid 必活；`is_alive` 不能把它误判为死。
    /// （win: OpenProcess 自查；unix: kill(pid,0) 自查。）
    #[test]
    fn is_alive_reports_self() {
        assert!(is_alive(std::process::id()));
    }

    /// 不可能存活的 pid 必判死（4_000_000 超出常见 pid 空间）。
    #[test]
    fn is_alive_reports_dead_pid() {
        assert!(!is_alive(4_000_000));
        // pid 0 在 kill(2)/OpenProcess 都是特殊语义，显式判死。
        assert!(!is_alive(0));
    }

    /// `find_in_path` 能找到 PATH 上真实存在的工具（cargo 是构建依赖，必在）。
    /// windows 分支顺带验证 `.exe` 追加探测路径可达。
    #[test]
    fn find_in_path_locates_cargo() {
        assert!(find_in_path("cargo").is_some());
        assert!(find_in_path("definitely-not-a-real-bin-momo").is_none());
    }
}
