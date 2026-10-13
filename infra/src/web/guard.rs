//! `momoctl web` 启动守卫：`--listen` 绑出 loopback 时强制 `MOMO_WEB_TOKEN`
//! （exit 2 usage error，config.md §2.4 口径）。
//! 请求级认证（Bearer 校验 + CSRF）不在此模块——那是独立 issue 的中间件。

use std::net::ToSocketAddrs;

use crate::output::AppError;

/// 校验 `--listen` 地址与 token 配置：
/// - 地址不可解析 / 解析为空 → Config（exit 2）；
/// - 任一解析结果非 loopback 且 token 未设 → Config（exit 2）；
/// - 全 loopback，或已配 token → 放行。
///
/// `localhost` 经系统解析（IPv4+IPv6 须全部 loopback 才视为本机绑定），
/// 防止只绑 v6 ::1 之外地址这类半开放配置蒙混过关。
pub fn check_listen(listen: &str, token_configured: bool) -> Result<(), AppError> {
    let addrs: Vec<_> = listen
        .to_socket_addrs()
        .map_err(|e| AppError::Config(format!("invalid --listen '{listen}': {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(AppError::Config(format!(
            "invalid --listen '{listen}': resolves to no address"
        )));
    }
    if !addrs.iter().all(|a| a.ip().is_loopback()) && !token_configured {
        return Err(AppError::Config(
            "--listen binds beyond loopback but MOMO_WEB_TOKEN is not set; \
             refusing to start (set MOMO_WEB_TOKEN or bind a loopback address)"
                .into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// loopback 字面量无论有无 token 都放行。
    #[test]
    fn loopback_passes_without_token() {
        assert!(check_listen("127.0.0.1:7900", false).is_ok());
        assert!(check_listen("[::1]:7900", false).is_ok());
    }

    /// 非 loopback 无 token → exit 2；有 token → 放行。
    #[test]
    fn non_loopback_requires_token() {
        let err = check_listen("0.0.0.0:7900", false).unwrap_err();
        assert!(matches!(err, AppError::Config(_)), "{err}");
        assert!(check_listen("0.0.0.0:7900", true).is_ok());
    }

    /// 无法解析的地址 → exit 2 而非 panic。
    #[test]
    fn unresolvable_is_usage_error() {
        assert!(matches!(
            check_listen("not-an-addr", false),
            Err(AppError::Config(_))
        ));
    }

    /// `localhost` 解析结果全 loopback 时按本机绑定放行。
    /// （解析依赖系统 hosts；CI/开发机 localhost 均解析到回环。）
    #[test]
    fn localhost_counts_as_loopback() {
        assert!(check_listen("localhost:7900", false).is_ok());
    }
}
