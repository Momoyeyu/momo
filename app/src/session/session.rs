//! `Session` actor：命令循环、持有当前 [`Turn`]。
//!
//! Session actor 是唯一可变状态持有者：所有事件（客户端消息、管线回调、
//! 定时器）都汇进它的 `mpsc`，内部不共享锁（momo-app.md §session）。

use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::{
    proto::message::{ClientKind, ClientMsg, TurnState},
    session::{history::History, turn::Turn},
};

/// 汇入 Session actor 的命令。
#[derive(Debug)]
pub enum SessionCmd {
    /// conn_rx 解码出的客户端消息。
    ClientMsg(ClientMsg),
    /// WS 断开：取消活动回合、保留会话历史（protocol §7）。
    ConnClosed,
    /// 被新连接顶掉 / 进程关停。
    Shutdown,
}

/// 外界持有的会话句柄（momo-app.md §session 签名）。
#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub id: String,
    /// 该 session 所属连接的 client 类型（hello.client）。
    pub client: ClientKind,
    pub cmd_tx: mpsc::Sender<SessionCmd>,
    pub state_rx: watch::Receiver<TurnState>,
}

/// Session actor 本体；由 [`Session::spawn`] 在 tokio task 中运行。
#[derive(Debug)]
pub struct Session {
    id: String,
    #[allow(dead_code)] // TODO(P0): 回合日志/镜像需关联 conn
    conn_id: String,
    client: ClientKind,
    state_tx: watch::Sender<TurnState>,
    /// 当前回合；barge-in 时新回合用新 CancellationToken（momo-app.md §拓扑）。
    #[allow(dead_code)] // TODO(P0): input.text / input.audio.segment 开回合
    turn: Option<Turn>,
    /// 对话历史与已播文本记账。
    #[allow(dead_code)] // TODO(P0): 回合结束/partial 记账
    history: History,
}

impl Session {
    /// 创建 actor task 并返回句柄。
    pub fn spawn(id: String, conn_id: String, client: ClientKind) -> SessionHandle {
        // 会话命令队列；容量无协议硬约束，取与 conn_tx 同量级。
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (state_tx, state_rx) = watch::channel(TurnState::Idle);
        let session = Session {
            id: id.clone(),
            conn_id,
            client,
            state_tx,
            turn: None,
            history: History::default(),
        };
        tokio::spawn(session.run(cmd_rx));
        SessionHandle {
            id,
            client,
            cmd_tx,
            state_rx,
        }
    }

    /// 命令循环：唯一可变状态持有点。
    async fn run(mut self, mut cmd_rx: mpsc::Receiver<SessionCmd>) {
        debug!(session = %self.id, client = ?self.client, "session actor started");
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                SessionCmd::ClientMsg(msg) => self.on_client_msg(msg).await,
                SessionCmd::ConnClosed => {
                    // TODO(P0): 取消活动回合（turn.cancel），保留 history。
                    debug!(session = %self.id, "conn closed");
                    break;
                }
                SessionCmd::Shutdown => {
                    debug!(session = %self.id, "session shutdown");
                    break;
                }
            }
        }
        let _ = self.state_tx.send(TurnState::Idle);
        debug!(session = %self.id, "session actor exit");
    }

    /// 客户端消息分派；回合/管线逻辑为 P0 占位。
    async fn on_client_msg(&mut self, msg: ClientMsg) {
        match msg {
            ClientMsg::InputText(input) => {
                // TODO(P0): 开新回合 → llm stream → splitter → tts → audio sink。
                debug!(session = %self.id, id = %input.id, "input.text received (turn pipeline stub)");
            }
            ClientMsg::ControlBargeIn(barge_in) => {
                // TODO(P0): turn.rs barge-in 三动作（停播/取消上游/已播记账）。
                debug!(session = %self.id, turn = %barge_in.turn_id, "control.barge_in received (stub)");
            }
            ClientMsg::InputVad(vad) => {
                // TODO(P2): VAD 触发 barge-in（MOMO_BARGEIN_MIN_MS 防抖）。
                debug!(session = %self.id, event = ?vad.event, "input.vad received (stub)");
            }
            ClientMsg::InputAudioSegment(seg) => {
                // TODO(P2): final 段 → 开语音回合；镜像给 UI。
                debug!(session = %self.id, segment = %seg.segment_id, "input.audio.segment received (stub)");
            }
            other => warn!(session = %self.id, ?other, "unhandled client msg (stub)"),
        }
    }
}
