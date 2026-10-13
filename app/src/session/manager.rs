//! `SessionManager`：连接归属、单活跃 session 策略（P0-P2 后连顶掉前连）。

use std::sync::Mutex;

use tokio::sync::mpsc;

use crate::{
    proto::message::{ClientKind, new_id},
    session::session::{Session, SessionCmd, SessionDeps, SessionHandle},
    ws::conn::OutboundFrame,
};

/// 会话注册表。v1 按单编排器+单活跃 UI session 假设（protocol §4）。
pub struct SessionManager {
    active: Mutex<Option<SessionHandle>>,
    /// 注入每个新 session 的回合依赖（history 上限、句切参数、LLM client）。
    deps: SessionDeps,
}

impl SessionManager {
    pub fn new(deps: SessionDeps) -> Self {
        Self {
            active: Mutex::new(None),
            deps,
        }
    }

    /// 新连接归属新 session；若已有活跃 session，发 `Shutdown` 顶掉前连。
    pub fn attach(
        &self,
        conn_id: &str,
        client: ClientKind,
        out_tx: mpsc::Sender<OutboundFrame>,
    ) -> SessionHandle {
        let handle = Session::spawn(
            new_id("sess"),
            conn_id.to_owned(),
            client,
            out_tx,
            self.deps.clone(),
        );
        let mut guard = self.active.lock().expect("session mutex poisoned");
        if let Some(old) = guard.replace(handle.clone()) {
            let _ = old.cmd_tx.try_send(SessionCmd::Shutdown);
        }
        handle
    }

    /// 当前活跃 session（若有）。
    pub fn active(&self) -> Option<SessionHandle> {
        self.active.lock().expect("session mutex poisoned").clone()
    }
}
