//! `momoctl doctor [--profile X] [--json]`：环境检查矩阵。
//! 组：system / toolchain / python / network / env / models / endpoints。
//! 退出码：全 ok → 0；有 fail → 3。检查项口径见 momo-infra.md「子命令清单」。

use std::path::PathBuf;
use std::process::Command;
use std::thread;

use crate::cli::{DoctorArgs, Profile};
use crate::env_cfg::{self, EnvCfg};
use crate::manifest::Manifest;
use crate::net;
use crate::output::{AppError, Check, CheckGroup, DoctorReport, ExitCode};

pub fn run(args: &DoctorArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    let env = EnvCfg;
    let profile = env.profile(args.profile);

    let groups = vec![
        check_system(),
        check_toolchain(),
        check_python_deps(),
        check_network(&env),
        check_env(&env, profile),
        check_models(&env),
        check_endpoints(&env),
    ];
    let report = DoctorReport::new(profile.as_str(), groups);

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        let mut out = std::io::stdout().lock();
        crate::output::render_doctor_human(&report, &mut out)?;
    }

    Ok(if report.has_fail() {
        ExitCode::CheckFailed as i32
    } else {
        ExitCode::Ok as i32
    })
}

// ---------- system ----------

fn check_system() -> CheckGroup {
    let mut checks = vec![];
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let detail = match os_version() {
        Some(v) => format!("{os} {v} ({arch})"),
        None => format!("{os} ({arch})"),
    };
    checks.push(Check::ok("os", detail));

    match os {
        "macos" => {
            // Metal 框架存在即视为 GPU 加速可用（mlx/llama.cpp 走 Metal）。
            if PathBuf::from("/System/Library/Frameworks/Metal.framework").exists() {
                checks.push(Check::ok("gpu.metal", "Metal framework present"));
            } else {
                checks.push(Check::warn(
                    "gpu.metal",
                    "Metal framework not found",
                    "MLX/llama.cpp 将退化 CPU；确认 macOS 版本与 Xcode CLT",
                ));
            }
        }
        "linux" => match tool_version("nvidia-smi", &["--version"]) {
            Some(v) => checks.push(Check::ok("gpu.cuda", v)),
            None => checks.push(Check::warn(
                "gpu.cuda",
                "nvidia-smi not found",
                "TTS CUDA 档需要 NVIDIA 驱动；纯 CPU 部署可忽略",
            )),
        },
        _ => checks.push(Check::ok("gpu", "no gpu probe for this OS")),
    }
    CheckGroup::new("system", checks)
}

/// `sw_vers -productVersion`（macOS）/ `uname -r` 兜底。
fn os_version() -> Option<String> {
    if cfg!(target_os = "macos") {
        tool_version("sw_vers", &["-productVersion"])
    } else {
        tool_version("uname", &["-r"])
    }
}

// ---------- toolchain ----------

fn check_toolchain() -> CheckGroup {
    // (bin, required)。必需项缺失 → fail；可选项缺失 → warn。
    const TOOLS: &[(&str, bool)] = &[
        ("rustc", true),
        ("cargo", true),
        ("python3", true),
        ("uv", true),
        ("node", false),
        ("pnpm", false),
        ("ffmpeg", false),
    ];
    let checks = TOOLS
        .iter()
        .map(|(bin, required)| match tool_version(bin, &["--version"]) {
            Some(v) => Check::ok(format!("tool.{bin}"), v),
            None if *required => Check::fail(
                format!("tool.{bin}"),
                format!("{bin} not found in PATH"),
                "安装后重试；rust 走 rustup，uv 见 astral.sh",
            ),
            None => Check::warn(
                format!("tool.{bin}"),
                format!("{bin} not found in PATH"),
                "前端构建/音频转码时需要",
            ),
        })
        .collect();
    CheckGroup::new("toolchain", checks)
}

/// 运行 `<bin> <args>` 取首行版本输出；二进制缺失或失败返回 None。
fn tool_version(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stdout
        .lines()
        .chain(stderr.lines())
        .next()
        .unwrap_or("")
        .trim();
    if line.is_empty() {
        Some(format!("{bin} (version output empty)"))
    } else {
        Some(line.to_string())
    }
}

// ---------- python deps ----------

fn check_python_deps() -> CheckGroup {
    let mut checks = vec![];
    // torch/torchaudio 存在性与 CUDA build；laptop 的 TTS 依赖它。
    let probe = Command::new("python3")
        .args([
            "-c",
            "import torch, torchaudio; \
             print(torch.__version__, torchaudio.__version__, \
             torch.cuda.is_available())",
        ])
        .output();
    match probe {
        Ok(out) if out.status.success() => {
            let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parts: Vec<&str> = line.split_whitespace().collect();
            let cuda = parts.get(2).is_some_and(|v| *v == "True");
            checks.push(Check::ok("py.torch", &line));
            if cuda {
                checks.push(Check::ok("py.cuda", "torch.cuda.is_available()=True"));
            } else if cfg!(target_os = "linux") {
                checks.push(Check::warn(
                    "py.cuda",
                    "torch.cuda.is_available()=False",
                    "TTS CUDA 档需要 CUDA build 的 torch；cpu/mps 档可忽略",
                ));
            } else {
                checks.push(Check::ok("py.cuda", "cuda=False (non-linux; mps/cpu path)"));
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("import failed")
                .trim()
                .to_string();
            checks.push(Check::fail(
                "py.torch",
                format!("torch/torchaudio import failed: {err}"),
                "在 momo-models 仓 `uv sync` 建 venv；doctor 探的是系统 python3",
            ));
        }
        Err(_) => checks.push(Check::fail(
            "py.torch",
            "python3 not found",
            "安装 python3 与 uv",
        )),
    }
    CheckGroup::new("python", checks)
}

// ---------- network ----------

fn check_network(env: &EnvCfg) -> CheckGroup {
    // 远端三个 URL 并行探，各 3s 超时。
    let targets: [(&str, &str); 3] = [
        ("net.hf-mirror", "https://hf-mirror.com"),
        ("net.huggingface", "https://huggingface.co"),
        ("net.deepseek", "https://api.deepseek.com"),
    ];
    let results = thread::scope(|s| {
        targets
            .iter()
            .map(|(id, url)| s.spawn(move || (*id, *url, net::probe_head(url))))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });

    let mut checks = vec![];
    for (id, url, p) in &results {
        let check = match *id {
            // hf-mirror 是 laptop 主源（05-ops/model-fetch.md），不可达为 fail。
            "net.hf-mirror" if p.reachable => Check::ok(*id, p.describe()),
            "net.hf-mirror" => Check::fail(
                *id,
                format!("{url} unreachable: {}", p.describe()),
                "laptop 主源必须可通；排查代理/DNS，备选 ModelScope",
            ),
            // huggingface.co 直连不可达是已知事实（laptop），warn + hint HF_ENDPOINT。
            "net.huggingface" if p.reachable => Check::ok(*id, p.describe()),
            "net.huggingface" => Check::warn(
                *id,
                format!("{url} unreachable: {}", p.describe()),
                "export HF_ENDPOINT=https://hf-mirror.com",
            ),
            // DeepSeek 仅降级档/P4 视觉需要，不可达 warn。
            _ if p.reachable => Check::ok(*id, p.describe()),
            _ => Check::warn(
                *id,
                format!("{url} unreachable: {}", p.describe()),
                "仅 degraded 档 DeepSeek 大脑 / P4 视觉需要",
            ),
        };
        checks.push(check);
    }

    // PyPI 镜像：只查 env 是否配置（安装期生效，不探活）。
    if env.get("UV_INDEX_URL").is_some() || env.get("PIP_INDEX_URL").is_some() {
        checks.push(Check::ok("net.pypi-mirror", "index mirror configured"));
    } else {
        checks.push(Check::warn(
            "net.pypi-mirror",
            "UV_INDEX_URL / PIP_INDEX_URL unset",
            "laptop 建议配置 PyPI 镜像（05-ops/model-fetch.md）",
        ));
    }
    CheckGroup::new("network", checks)
}

// ---------- env ----------

fn check_env(env: &EnvCfg, profile: Profile) -> CheckGroup {
    let mut checks = vec![];

    checks.push(Check::ok("env.MOMO_PROFILE", profile.as_str()));

    // 必需键：缺失 fail；敏感键打码。
    for key in env_cfg::REQUIRED_KEYS {
        checks.push(match env.get(key) {
            Some(v) => Check::ok(format!("env.{key}"), v),
            None => Check::fail(
                format!("env.{key}"),
                "unset",
                format!("在 .env 中设置 {key}（模板见 .env.example）"),
            ),
        });
    }

    // 有默认值的键：报告生效值或 default。
    for key in env_cfg::DEFAULTED_KEYS {
        checks.push(match env.get(key) {
            Some(v) => Check::ok(format!("env.{key}"), v),
            None => Check::ok(format!("env.{key}"), "unset → built-in default"),
        });
    }

    // 敏感键：只报 set/unset（config.md §4 不回显）。
    for key in env_cfg::SENSITIVE_KEYS {
        let detail = if env.get(key).is_some() {
            "set"
        } else {
            "unset"
        };
        checks.push(Check::ok(format!("env.{key}"), detail));
    }

    // 条件必需：LLM 端点指 DeepSeek 时必须有 key。
    if let Some(ep) = env.llm_endpoint() {
        if ep.contains("deepseek.com")
            && env.get("MOMO_LLM_API_KEY").is_none()
            && env.get("MOMO_DEEPSEEK_API_KEY").is_none()
        {
            checks.push(Check::fail(
                "env.MOMO_LLM_API_KEY",
                "unset but MOMO_LLM_ENDPOINT is api.deepseek.com",
                "degraded DeepSeek 档必须设置 MOMO_LLM_API_KEY",
            ));
        }
    }
    CheckGroup::new("env", checks)
}

// ---------- models ----------

/// MANIFEST.toml 定位：当前目录 ./models/，其次兄弟仓 ../momo-models/models/。
fn find_manifest() -> Option<PathBuf> {
    [
        "models/MANIFEST.toml",
        "../momo-models/models/MANIFEST.toml",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

fn check_models(env: &EnvCfg) -> CheckGroup {
    let mut checks = vec![];
    let models_dir = env.models_dir();
    if models_dir.is_dir() {
        checks.push(Check::ok(
            "models.dir",
            format!("{} exists", models_dir.display()),
        ));
    } else {
        checks.push(Check::warn(
            "models.dir",
            format!("{} missing", models_dir.display()),
            "首次 `momoctl pull` 会创建",
        ));
    }

    let manifest_path = match find_manifest() {
        Some(p) => p,
        None => {
            checks.push(Check::fail(
                "models.manifest",
                "MANIFEST.toml not found",
                "预期 ../momo-models/models/MANIFEST.toml（克隆 momo-models 仓）",
            ));
            return CheckGroup::new("models", checks);
        }
    };
    checks.push(Check::ok(
        "models.manifest",
        format!("{}", manifest_path.display()),
    ));

    let manifest = match Manifest::load(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            checks.push(Check::fail(
                "models.manifest",
                format!("parse failed: {e}"),
                "条目口径见 momo-models.md：name/repo_id/revision/files/sha256/dest",
            ));
            return CheckGroup::new("models", checks);
        }
    };

    if manifest.models.is_empty() {
        checks.push(Check::ok("models.entries", "0 entries (empty manifest)"));
    }
    for m in &manifest.models {
        let dir = m.dest_dir(&models_dir);
        if !dir.is_dir() {
            checks.push(Check::fail(
                format!("model.{}", m.name),
                format!("{} missing", dir.display()),
                format!("momoctl pull --only {}", m.name),
            ));
            continue;
        }
        let missing: Vec<&str> = m
            .files
            .iter()
            .filter(|f| !dir.join(&f.path).exists())
            .map(|f| f.path.as_str())
            .collect();
        if !missing.is_empty() {
            checks.push(Check::fail(
                format!("model.{}", m.name),
                format!("{}/{} files missing", missing.len(), m.files.len()),
                format!("momoctl pull --only {}", m.name),
            ));
            continue;
        }
        // sha256 抽查：第一条带 hash 的文件。
        if let Some(f) = m.files.iter().find(|f| f.sha256.is_some()) {
            let path = dir.join(&f.path);
            match crate::manifest::sha256_file(&path) {
                Ok(h) if h.eq_ignore_ascii_case(f.sha256.as_ref().unwrap()) => {
                    checks.push(Check::ok(
                        format!("model.{}", m.name),
                        format!("{} files ok, sha256 spot-check passed", m.files.len()),
                    ));
                }
                Ok(_) => checks.push(Check::fail(
                    format!("model.{}", m.name),
                    format!("sha256 mismatch on {}", f.path),
                    format!("删除后 `momoctl pull --only {}` 重下", m.name),
                )),
                Err(e) => checks.push(Check::warn(
                    format!("model.{}", m.name),
                    format!("sha256 spot-check failed: {e}"),
                    "检查文件可读性",
                )),
            }
        } else {
            checks.push(Check::ok(
                format!("model.{}", m.name),
                format!("{} files present (no sha256 to spot-check)", m.files.len()),
            ));
        }
    }
    CheckGroup::new("models", checks)
}

// ---------- endpoints ----------

/// `0.0.0.0`/`::` 监听地址探活时换成 loopback。
fn probeable(listen: &str) -> String {
    listen
        .replacen("0.0.0.0", "127.0.0.1", 1)
        .replacen("[::]", "127.0.0.1", 1)
}

fn check_endpoints(env: &EnvCfg) -> CheckGroup {
    let mut checks = vec![];

    let local: [(&str, String, &str); 3] = [
        ("ep.momod", probeable(&env.orch_listen()), "/healthz"),
        ("ep.svc-asr", probeable(&env.asr_listen()), "/healthz"),
        ("ep.svc-tts", probeable(&env.tts_listen()), "/healthz"),
    ];
    for (id, addr, path) in &local {
        let url = format!("http://{addr}{path}");
        let p = net::probe_get(&url);
        if p.ready() {
            checks.push(Check::ok(*id, format!("{url} → {}", p.describe())));
        } else {
            checks.push(Check::fail(
                *id,
                format!("{url} → {}", p.describe()),
                "未启动：`momoctl serve up`；启动后应返回 200",
            ));
        }
    }

    // 大脑：MOMO_LLM_ENDPOINT（含 /v1）+ /models，OpenAI 端点直连探活。
    match env.llm_endpoint() {
        Some(ep) => {
            let url = format!("{}/models", ep.trim_end_matches('/'));
            let p = net::probe_get(&url);
            if p.ready() {
                checks.push(Check::ok("ep.brain", format!("{url} → {}", p.describe())));
            } else {
                checks.push(Check::fail(
                    "ep.brain",
                    format!("{url} → {}", p.describe()),
                    "正常档查 mac-studio MLX；不可用则 `momoctl switch --to degraded`",
                ));
            }
        }
        None => checks.push(Check::fail(
            "ep.brain",
            "MOMO_LLM_ENDPOINT unset",
            "在 .env 中设置 MOMO_LLM_ENDPOINT",
        )),
    }
    CheckGroup::new("endpoints", checks)
}
