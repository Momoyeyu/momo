//! clap 命令定义：全局 `--json` / `--env-file` / `-v` + 六个子命令。
//! 签名对齐 momo-docs `02-repos/momo-infra.md`「子命令清单」。

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "momoctl",
    version,
    about = "momo 运维 CLI：doctor / pull / serve / switch / bench / gc / config"
)]
pub struct Cli {
    /// 以 JSON 输出单一对象（schema 版本化，供 CI/脚本消费）
    #[arg(long, global = true)]
    pub json: bool,

    /// .env 文件路径（默认自动发现 ./.env）
    #[arg(long, global = true, value_name = "PATH")]
    pub env_file: Option<PathBuf>,

    /// 提高日志详细度：-v=info，-vv=debug（默认 warn）
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 环境检查矩阵（系统/工具链/Python/网络/env/模型/端点）
    Doctor(DoctorArgs),
    /// 模型下载 + sha256 校验（清单来自 momo-models 的 MANIFEST.toml）
    Pull(PullArgs),
    /// 本地服务编排：up/down/status/logs/restart
    Serve(ServeArgs),
    /// normal ↔ degraded 档位切换
    Switch(SwitchArgs),
    /// 端到端延迟测量（p50/p95，目标 <1.5s 出声）
    Bench(BenchArgs),
    /// 模型 GC：N-1 代清理、models.lock 联动
    Gc(GcArgs),
    /// env schema 漂移检查（config.schema.toml ↔ .env.example / config.md）
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
    /// 按指定档位校验必需 env 与端点（默认读 MOMO_PROFILE，缺省 normal）
    #[arg(long, value_enum)]
    pub profile: Option<Profile>,
}

#[derive(Debug, Args)]
pub struct PullArgs {
    /// MANIFEST.toml 路径（默认 momo-models 仓 models/MANIFEST.toml）
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,

    /// 只处理指定条目（可重复）
    #[arg(long, value_name = "NAME")]
    pub only: Vec<String>,

    /// 只校验不下载（按 models.lock / MANIFEST 复算 sha256）
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
    /// 按 profile 组件表依序拉起并等待就绪
    Up(ServeCommon),
    /// 按 pidfile 逆序 SIGTERM→SIGKILL
    Down(ServeCommon),
    /// pid 存活 + /healthz 探测
    Status(ServeCommon),
    /// tail 组件日志（-n 行数，-f 持续跟随）
    Logs(LogsArgs),
    /// down + up
    Restart(ServeCommon),
}

#[derive(Debug, Args)]
pub struct ServeCommon {
    /// 组件清单档位（默认读 MOMO_PROFILE，缺省 normal）
    #[arg(long, value_enum)]
    pub profile: Option<Profile>,

    /// 只作用于指定组件（可重复）
    #[arg(long, value_name = "COMPONENT", visible_alias = "component")]
    pub only: Vec<String>,

    /// momod 二进制路径（默认 PATH 查找）
    #[arg(long, value_name = "PATH")]
    pub app_bin: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[command(flatten)]
    pub common: ServeCommon,

    /// 每个组件输出最后 N 行
    #[arg(short = 'n', long = "lines", default_value_t = 50, value_name = "N")]
    pub lines: usize,

    /// 持续跟随新增日志（Ctrl-C 退出）
    #[arg(short = 'f', long)]
    pub follow: bool,
}

#[derive(Debug, Args)]
pub struct SwitchArgs {
    /// 目标档位：.env 符号链接重指 .env.<to>
    #[arg(long, value_enum)]
    pub to: Profile,

    /// 切换后自动 serve down && serve up
    #[arg(long)]
    pub restart: bool,
}

#[derive(Debug, Args)]
pub struct BenchArgs {
    /// 测量回合数（取 p50/p95）
    #[arg(long, default_value_t = 5, value_name = "N")]
    pub turns: u32,

    /// 超标（>=1.5s 出声）时以退出码 3 结束
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
    /// 校验三侧 .env.example 与 config.md 是否和 config.schema.toml 一致
    Check(ConfigCheckArgs),
}

#[derive(Debug, Args)]
pub struct ConfigCheckArgs {
    /// 仓库根目录（默认从 CWD 向上定位 config.schema.toml）
    #[arg(long, value_name = "PATH")]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct GcArgs {
    /// 只报告不删除（默认行为；删除需显式 --yes）
    #[arg(long)]
    pub dry_run: bool,

    /// 微调产物保留代数（默认 2，即 N-1 中保留最新两代）
    #[arg(long, default_value_t = 2, value_name = "N")]
    pub keep: u32,

    /// 确认执行删除（不带 --yes 时等价 --dry-run）
    #[arg(long)]
    pub yes: bool,
}
