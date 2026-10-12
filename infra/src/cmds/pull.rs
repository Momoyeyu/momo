//! `momoctl pull [--manifest PATH] [--only NAME] [--check]`：模型下载 + sha256 校验。
//! `--check` 已实装：定位 MANIFEST.toml → 逐文件存在性 + sha256 复算，
//! 占位 sha256（"TODO"）与缺失文件按 fail 上报，退出码 5。
//! 下载路径（HF_ENDPOINT resolve、.part 续传、models.lock 登记）仍 stub。

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::cli::PullArgs;
use crate::env_cfg::EnvCfg;
use crate::manifest::{is_placeholder_sha, sha256_file, FileEntry, Manifest};
use crate::output::{not_implemented, AppError, Check, CheckStatus, ExitCode, Summary};

/// `momoctl pull --check` 报告：JSON 顶层恒带 `schema`。
#[derive(Debug, Serialize)]
struct PullReport {
    schema: u8,
    manifest: String,
    models_dir: String,
    checks: Vec<Check>,
    summary: Summary,
}

pub fn run(args: &PullArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    if !args.check {
        return Ok(not_implemented("pull", ctx.json));
    }
    check(args, ctx)
}

fn check(args: &PullArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    let manifest_path = locate_manifest(args)?;
    let manifest = Manifest::load(&manifest_path)?;
    let models_dir = EnvCfg.models_dir();

    let selected: Vec<_> = manifest
        .models
        .iter()
        .filter(|m| args.only.is_empty() || args.only.iter().any(|o| o == &m.name))
        .collect();
    if selected.is_empty() {
        return Err(AppError::Config(format!(
            "no manifest entry matches --only {:?}",
            args.only
        )));
    }

    let mut checks = Vec::new();
    for m in selected {
        if m.files.is_empty() {
            checks.push(Check::warn(
                format!("{}/", m.name),
                "entry declares no files",
                "MANIFEST.toml 条目口径见 momo-models.md",
            ));
        }
        for f in &m.files {
            checks.push(check_file(
                m.dest_dir(&models_dir).join(&f.path),
                &m.name,
                f,
            ));
        }
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
    let report = PullReport {
        schema: 1,
        manifest: manifest_path.display().to_string(),
        models_dir: models_dir.display().to_string(),
        checks,
        summary,
    };

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        render_human(&report, &mut std::io::stdout().lock())?;
    }

    Ok(if report.summary.fail > 0 {
        ExitCode::Verify as i32
    } else {
        ExitCode::Ok as i32
    })
}

/// 单个文件校验：缺失 / size 不符 / sha256 占位 / sha256 不匹配 → fail。
fn check_file(path: PathBuf, model: &str, f: &FileEntry) -> Check {
    let id = format!("{model}/{}", f.path);
    if !path.is_file() {
        return Check::fail(
            id,
            format!("missing: {}", path.display()),
            format!("momoctl pull --only {model} 下载后重跑 --check"),
        );
    }
    if let Some(expected) = f.size {
        match fs::metadata(&path) {
            Ok(md) if md.len() == expected => {}
            Ok(md) => {
                return Check::fail(
                    id,
                    format!("size {} != expected {}", md.len(), expected),
                    "文件可能不完整；重新 pull",
                )
            }
            Err(e) => return Check::fail(id, format!("stat failed: {e}"), "检查文件权限与路径"),
        }
    }
    match &f.sha256 {
        Some(h) if is_placeholder_sha(h) => Check::fail(
            id,
            "sha256 is placeholder (TODO); cannot verify",
            "MANIFEST sha256 待 pull 实装后按实际文件补齐",
        ),
        Some(expected) => match sha256_file(&path) {
            Ok(actual) if actual.eq_ignore_ascii_case(expected) => {
                Check::ok(id, format!("sha256 ok ({})", &actual[..12]))
            }
            Ok(actual) => Check::fail(
                id,
                format!(
                    "sha256 {} != expected {}",
                    &actual[..12],
                    &expected[..12.min(expected.len())]
                ),
                format!("删除 {} 后重新 pull", path.display()),
            ),
            Err(e) => Check::fail(id, format!("sha256 read failed: {e}"), "检查文件可读性"),
        },
        None => Check::ok(id, "present (no sha256 declared)"),
    }
}

/// MANIFEST.toml 定位：`--manifest` 优先；否则按常见仓布局探测。
/// 候选：momo-models 仓内 cwd、monorepo 根、infra/ 子目录、momo-models 兄弟仓。
fn locate_manifest(args: &PullArgs) -> Result<PathBuf, AppError> {
    if let Some(p) = &args.manifest {
        if !p.is_file() {
            return Err(AppError::Config(format!(
                "manifest not found: {}",
                p.display()
            )));
        }
        return Ok(p.clone());
    }
    const CANDIDATES: &[&str] = &[
        "models/MANIFEST.toml",
        "models/models/MANIFEST.toml",
        "../models/models/MANIFEST.toml",
        "../momo-models/models/MANIFEST.toml",
    ];
    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .ok_or_else(|| {
            AppError::Config(format!(
                "MANIFEST.toml not found (tried {}); use --manifest PATH",
                CANDIDATES.join(", ")
            ))
        })
}

fn render_human(report: &PullReport, w: &mut impl Write) -> std::io::Result<()> {
    writeln!(w, "== pull --check ==")?;
    writeln!(w, "manifest:   {}", report.manifest)?;
    writeln!(w, "models dir: {}", report.models_dir)?;
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
    )
}
