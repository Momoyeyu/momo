//!

use async_trait::async_trait;
use bytes::Bytes;
use url::Url;



}

#[async_trait]
pub trait AsrClient: Send + Sync {
}

pub struct MomoWsAsr {
}

impl MomoWsAsr {
    pub fn new(endpoint: Url) -> Self {
    }

    }
}

#[async_trait]
impl AsrClient for MomoWsAsr {
    }
}
