
use std::sync::Mutex;

use tokio::sync::mpsc;

use crate::{
    proto::message::{ClientKind, new_id},
    ws::conn::OutboundFrame,
};

/// 会话注册表。v1 按单编排器+单活跃 UI session 假设（protocol §4）。
pub struct SessionManager {
    active: Mutex<Option<SessionHandle>>,
}

impl SessionManager {
    }

    pub fn attach(
        &self,
        conn_id: &str,
        client: ClientKind,
        out_tx: mpsc::Sender<OutboundFrame>,
    }

    /// 当前活跃 session（若有）。
    pub fn active(&self) -> Option<SessionHandle> {
        self.active.lock().expect("session mutex poisoned").clone()
    }
}
