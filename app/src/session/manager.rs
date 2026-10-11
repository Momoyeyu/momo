
use std::sync::Mutex;

use crate::{
    proto::message::{ClientKind, new_id},
};

pub struct SessionManager {
    active: Mutex<Option<SessionHandle>>,
}

impl SessionManager {
    }

    }

    /// 当前活跃 session（若有）。
    pub fn active(&self) -> Option<SessionHandle> {
        self.active.lock().expect("session mutex poisoned").clone()
    }
}
