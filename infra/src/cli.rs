//! clap 命令定义：全局 `--json` / `--env-file` + 七个子命令。
//! 签名对齐 momo-docs `02-repos/momo-infra.md`「子命令清单」。

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "momoctl",
    version,
    about = "momo ops CLI: doctor / pull / serve / switch / bench / gc / config"
)]
pub struct Cli {
    /// Emit a single JSON object (versioned schema, for CI/scripts)
    #[arg(long, global = true)]
    pub json: bool,

    /// .env file path (auto-discovers ./.env by default)
    #[arg(long, global = true, value_name = "PATH")]
    pub env_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Environment check matrix (system/toolchain/python/network/env/models/endpoints)
    Doctor(DoctorArgs),
    /// Model download + sha256 verification (manifest from momo-models MANIFEST.toml)
    Pull(PullArgs),
    /// Local service orchestration: up/down/status/logs/restart
    Serve(ServeArgs),
    /// Switch between normal and degraded profiles
    Switch(SwitchArgs),
    /// End-to-end latency measurement (p50/p95, target <1.5s to first audio)
    Bench(BenchArgs),
    /// Model GC: keep N-1 generations, reconcile models.lock
    Gc(GcArgs),
    /// env schema drift check (config.schema.toml vs .env.example / config.md)
    Config(ConfigArgs),
}

/// 部署档位（与 MOMO_PROFILE 取值一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Profile {
    Normal,
    Degraded,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Normal => "normal",
            Profile::Degraded => "degraded",
        }
    }
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Validate required env and endpoints for a profile (default: MOMO_PROFILE, else normal)
    #[arg(long, value_enum)]
    pub profile: Option<Profile>,
}

#[derive(Debug, Args)]
pub struct PullArgs {
    /// MANIFEST.toml path (default: momo-models repo models/MANIFEST.toml)
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,

    /// Process only the named entries (repeatable)
    #[arg(long, value_name = "NAME")]
    pub only: Vec<String>,

    /// Verify only, no download (recompute sha256 per models.lock / MANIFEST)
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[command(subcommand)]
    pub action: ServeAction,
}

#[derive(Debug, Subcommand)]
pub enum ServeAction {
    /// Spawn components in profile start order and wait for readiness
    Up(ServeCommon),
    /// SIGTERM→SIGKILL in reverse pidfile order
    Down(ServeCommon),
    /// pid liveness + /healthz probes
    Status(ServeCommon),
    /// Tail component logs (-n lines, -f follow)
    Logs(LogsArgs),
    /// down + up
    Restart(ServeCommon),
}

#[derive(Debug, Args)]
pub struct ServeCommon {
    /// Component profile (default: MOMO_PROFILE, else normal)
    #[arg(long, value_enum)]
    pub profile: Option<Profile>,

    /// Apply only to the named components (repeatable)
    #[arg(long, value_name = "COMPONENT", visible_alias = "component")]
    pub only: Vec<String>,

    /// momod binary path (default: PATH lookup)
    #[arg(long, value_name = "PATH")]
    pub app_bin: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[command(flatten)]
    pub common: ServeCommon,

    /// Print the last N lines per component
    #[arg(short = 'n', long = "lines", default_value_t = 50, value_name = "N")]
    pub lines: usize,

    /// Keep following new log output (Ctrl-C to exit)
    #[arg(short = 'f', long)]
    pub follow: bool,
}

#[derive(Debug, Args)]
pub struct SwitchArgs {
    /// Target profile: repoint .env symlink to .env.<to>
    #[arg(long, value_enum)]
    pub to: Profile,

    /// Automatically serve down && serve up after switching
    #[arg(long)]
    pub restart: bool,
}

#[derive(Debug, Args)]
pub struct BenchArgs {
    /// Number of turns to measure (p50/p95)
    #[arg(long, default_value_t = 5, value_name = "N")]
    pub turns: u32,

    /// Exit with code 3 when over budget (>=1.5s to first audio)
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Check the three .env.example files and config.md against config.schema.toml
    Check(ConfigCheckArgs),
}

#[derive(Debug, Args)]
pub struct ConfigCheckArgs {
    /// Repo root (default: locate config.schema.toml upward from CWD)
    #[arg(long, value_name = "PATH")]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct GcArgs {
    /// Report only, do not delete (default; deletion requires --yes)
    #[arg(long)]
    pub dry_run: bool,

    /// Fine-tune artifact generations to keep (default 2, i.e. newest two of N-1)
    #[arg(long, default_value_t = 2, value_name = "N")]
    pub keep: u32,

    /// Confirm deletion (without --yes this is equivalent to --dry-run)
    #[arg(long)]
    pub yes: bool,
}
