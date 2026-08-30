use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = workboard_client_protocol::CURRENT_PROTOCOL_VERSION;
pub const LEGACY_PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    pub protocol_version: u32,
    pub token: String,
    pub command: WriteCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WriteCommand {
    Ping,
    RefreshNativeSessions {
        claude_root: Option<PathBuf>,
        codex_root: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub protocol_version: u32,
    pub result: Option<Value>,
    pub error: Option<RemoteError>,
}

impl ResponseEnvelope {
    pub fn success(result: Value) -> Self {
        Self {
            protocol_version: LEGACY_PROTOCOL_VERSION,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            protocol_version: LEGACY_PROTOCOL_VERSION,
            result: None,
            error: Some(RemoteError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteError {
    pub code: String,
    pub message: String,
}
