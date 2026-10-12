//!

use tracing::{debug, warn};

use crate::{
    ws::conn::OutboundFrame,
};

/// 汇入 Session actor 的命令。
#[derive(Debug)]
pub enum SessionCmd {
    /// conn_rx 解码出的客户端消息。
    Msg(Msg),
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub id: String,
    /// 该 session 所属连接的 client 类型（hello.client）。
    pub client: ClientKind,
    pub cmd_tx: mpsc::Sender<SessionCmd>,
    /// conn_tx 出站队列（有界 mpsc 背压）；Session/管线经它向连接推帧。
    pub out_tx: mpsc::Sender<OutboundFrame>,
    pub state_rx: watch::Receiver<TurnState>,
}

/// Session actor 本体；由 [`Session::spawn`] 在 tokio task 中运行。
pub struct Session {
    id: String,
    conn_id: String,
    client: ClientKind,
    state_tx: watch::Sender<TurnState>,
    /// conn_tx 出站队列；回合/管线 domain event 经它按序推帧（有界背压）。
    out_tx: mpsc::Sender<OutboundFrame>,
    turn: Option<Turn>,
    history: History,
}

impl Session {
    /// 创建 actor task 并返回句柄；`out_tx` 是该连接的 conn_tx 出站队列。
    pub fn spawn(
        id: String,
        conn_id: String,
        client: ClientKind,
        out_tx: mpsc::Sender<OutboundFrame>,
    ) -> SessionHandle {
        // 会话命令队列；容量无协议硬约束，取与 conn_tx 同量级。
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (state_tx, state_rx) = watch::channel(TurnState::Idle);
        let session = Session {
            id: id.clone(),
            conn_id,
            client,
            state_tx,
            out_tx: out_tx.clone(),
            turn: None,
        };
        tokio::spawn(session.run(cmd_rx));
        SessionHandle {
            id,
            client,
            cmd_tx,
            out_tx,
            state_rx,
        }
    }

    /// 命令循环：唯一可变状态持有点。
    async fn run(mut self, mut cmd_rx: mpsc::Receiver<SessionCmd>) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                SessionCmd::Msg(msg) => self.on_client_msg(msg).await,
                    break;
                }
            }
        }
        let _ = self.state_tx.send(TurnState::Idle);
        debug!(session = %self.id, "session actor exit");
    }

    async fn on_client_msg(&mut self, msg: Msg) {
        match msg {
            }
            }
            }
            }
        }
    }
}
