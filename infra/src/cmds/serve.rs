//! `momoctl serve <up|down|status|logs|restart> [--profile X] [--only C] [--app-bin P]`。
//! 执行体未实现；up/restart 已能加载 profile 组件表验证参数面。

use crate::cli::{ServeAction, ServeArgs, ServeCommon};
use crate::env_cfg::EnvCfg;
use crate::output::{not_implemented, AppError};
use crate::serve::profile::ServeProfile;

pub fn run(args: &ServeArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    let (action, common) = split(&args.action);
    match action {
        "up" | "restart" => {
            // 预热真实路径：档位解析 + profiles/*.toml 组件表加载（证明参数面可用）。
            let profile = EnvCfg.profile(common.profile);
            let doc = ServeProfile::load_builtin(profile.as_str())?;
            if ctx.verbose > 0 && !ctx.json {
                eprintln!(
                    "profile '{}': {} component(s) in start order: {}",
                    profile.as_str(),
                    doc.components.len(),
                    doc.start_order()
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" -> ")
                );
            }
        }
        _ => {}
    }
    Ok(not_implemented(&format!("serve {action}"), ctx.json))
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
