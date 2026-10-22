use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub base_url: String,
    pub token: String,
}

#[derive(Deserialize)]
struct Ready {
    #[serde(rename = "type")]
    kind: String,
    nonce: String,
    pid: u32,
    port: u16,
}

fn validate_ready(line: &str, nonce: &str, pid: u32) -> Result<u16, String> {
    let ready: Ready =
        serde_json::from_str(line).map_err(|_| "Invalid backend readiness message")?;
    if ready.kind != "momo-ready" || ready.nonce != nonce || ready.pid != pid || ready.port == 0 {
        return Err("Backend ownership handshake failed".into());
    }
    Ok(ready.port)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn randomness_has_full_length_and_fails_no_fallback_contract() {
        let first = secret().unwrap();
        let second = secret().unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
        assert!(validate_ready("not json", "nonce", 1).is_err());
        assert!(validate_ready("{}", "nonce", 1).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_reaps_owned_process_and_leaves_unrelated_process_alive() {
        use std::os::unix::process::CommandExt;
        let mut unrelated = Command::new("sleep").arg("30").spawn().unwrap();
        let child = Command::new("sh")
            .args(["-c", "cat >/dev/null"])
            .process_group(0)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut backend = OwnedBackend { child };
        backend.shutdown();
        assert!(backend.child.try_wait().unwrap().is_some());
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert!(unrelated.try_wait().unwrap().is_none());
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn keeps_stdout_open_after_readiness_for_native_libraries() {
        let mut child = Command::new("sh")
            .args([
                "-c",
                "printf 'ready\\n'; dd if=/dev/zero bs=65536 count=4 2>/dev/null",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        read_ready_output(child.stdout.take().unwrap(), tx);
        assert_eq!(rx.recv().unwrap().unwrap(), "ready\n");
        assert!(
            child.wait().unwrap().success(),
            "Native stdout must not see a closed pipe"
        );
    }

    #[test]
    fn accepts_only_the_owned_child_nonce_and_nonzero_port() {
        let line = r#"{"type":"momo-ready","nonce":"secret","pid":42,"port":39101}"#;
        assert_eq!(validate_ready(line, "secret", 42).unwrap(), 39101);
        assert!(validate_ready(line, "wrong", 42).is_err());
        assert!(validate_ready(line, "secret", 43).is_err());
        assert!(validate_ready(&line.replace("39101", "0"), "secret", 42).is_err());
        assert!(validate_ready(&line.replace("momo-ready", "other"), "secret", 42).is_err());
    }
}

use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

pub struct OwnedBackend {
    child: Child,
    #[cfg(windows)]
    job: Job,
}

#[cfg(windows)]
struct Job(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

impl OwnedBackend {
    fn shutdown(&mut self) {
        // EOF requests FastAPI lifespan cleanup, including its MCP clients.
        self.child.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for OwnedBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct ShellState {
    pub runtime: Mutex<Result<Runtime, String>>,
    child: Mutex<Option<OwnedBackend>>,
}
impl ShellState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            runtime: Mutex::new(Err("Backend is starting".into())),
            child: Mutex::new(None),
        })
    }
    pub fn shutdown(&self) {
        self.child.lock().unwrap().take();
    }
}

fn secret() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "Operating system randomness unavailable")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn start(state: Arc<ShellState>, resource_dir: &Path, data_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|_| "Cannot create desktop data directory")?;
    let token = secret()?;
    let nonce = secret()?;
    let bin = resource_dir.join("backend").join(if cfg!(windows) {
        "momo-backend.exe"
    } else {
        "momo-backend"
    });
    let mut command = Command::new(bin);
    command
        .args(["--port", "0", "--data-dir"])
        .arg(data_dir)
        .arg("--resources-dir")
        .arg(resource_dir.join("models"))
        .env("MOMO_DESKTOP_TOKEN", &token)
        .env("MOMO_DESKTOP_NONCE", &nonce)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start packaged backend: {e}"))?;
    let pid = child.id();
    #[cfg(windows)]
    let job = unsafe {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            let _ = child.kill();
            return Err("Cannot create backend job".into());
        }
        let job = Job(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(handle, child.as_raw_handle() as _) == 0
        {
            let _ = child.kill();
            return Err("Cannot contain backend process".into());
        }
        job
    };
    let stdout = child.stdout.take().ok_or("Backend stdout unavailable")?;
    *state.child.lock().unwrap() = Some(OwnedBackend {
        child,
        #[cfg(windows)]
        job,
    });
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || read_ready_output(stdout, tx));
    let result = (|| {
        let line = rx
            .recv_timeout(Duration::from_secs(45))
            .map_err(|_| "Backend readiness timed out")?
            .map_err(|_| "Cannot read backend readiness")?;
        let port = validate_ready(&line, &nonce, pid)?;
        let base_url = format!("http://127.0.0.1:{port}");
        let response = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| "Cannot create health client")?
            .get(format!("{base_url}/api/health"))
            .bearer_auth(&token)
            .send()
            .map_err(|_| "Backend health check failed")?;
        if !response.status().is_success() {
            return Err("Backend health check rejected".into());
        }
        if state
            .child
            .lock()
            .unwrap()
            .as_mut()
            .ok_or("Backend stopped")?
            .child
            .try_wait()
            .map_err(|_| "Cannot inspect backend")?
            .is_some()
        {
            return Err("Backend exited during startup".into());
        }
        Ok(Runtime { base_url, token })
    })();
    *state.runtime.lock().unwrap() = result.clone();
    if result.is_err() {
        state.shutdown();
    }
    if result.is_ok() {
        let monitor = state.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let crashed = {
                    let mut owned = monitor.child.lock().unwrap();
                    match owned.as_mut() {
                        None => return,
                        Some(backend) => matches!(backend.child.try_wait(), Ok(Some(_))),
                    }
                };
                if crashed {
                    *monitor.runtime.lock().unwrap() =
                        Err("Backend exited; restart momo to reconnect".into());
                    monitor.shutdown();
                    return;
                }
            }
        });
    }
    result.map(|_| ())
}

fn read_ready_output(stdout: impl std::io::Read, tx: mpsc::SyncSender<std::io::Result<String>>) {
    use std::io::Read;
    let mut line = String::new();
    let mut reader = BufReader::new(stdout);
    let result = (&mut reader).take(8192).read_line(&mut line).map(|_| line);
    let _ = tx.send(result);
    // Native libraries may write stdout after startup. Drain without logging
    // to keep their pipe open and avoid disclosing a malformed handshake.
    let _ = std::io::copy(&mut reader, &mut std::io::sink());
}
