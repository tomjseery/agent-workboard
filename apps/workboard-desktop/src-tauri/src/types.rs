use serde::{Deserialize, Serialize};
use ts_rs::TS;
use workboard_client_protocol::{
    BoardSnapshot, CommandOperation, EventCursor, EventEnvelope, ReadQuery, ResyncRequirement,
    WorkspaceId,
};

pub const MAX_IPC_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapState {
    Connecting,
    Disconnected,
    Incompatible,
    ReadOnly,
    Resyncing,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionTarget {
    pub workspace_id: WorkspaceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapHandshake {
    pub state: BootstrapState,
    pub subscriptions: Vec<SubscriptionTarget>,
    pub refusal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub workspace_id: WorkspaceId,
    pub query: ReadQuery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequest {
    pub workspace_id: WorkspaceId,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub command: CommandOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SubscribeRequest {
    Start {
        workspace_id: WorkspaceId,
        cursor: Option<EventCursor>,
    },
    Cancel {
        subscription_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionReceipt {
    pub subscription_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SubscriptionMessage {
    Connected {
        state: BootstrapState,
    },
    Event(EventEnvelope),
    Resyncing(ResyncRequirement),
    Resynced {
        requirement: ResyncRequirement,
        #[ts(type = "unknown")]
        snapshot: BoardSnapshot,
    },
    Disconnected {
        code: String,
    },
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

impl BridgeError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }

    pub fn invalid_request() -> Self {
        Self::new("invalid_request", "The desktop request is invalid.")
    }

    pub fn request_too_large() -> Self {
        Self::new(
            "request_too_large",
            "The desktop request exceeds the allowed size.",
        )
    }

    pub fn disconnected() -> Self {
        Self::new("daemon_unavailable", "Workboard is unavailable.")
    }

    pub fn incompatible() -> Self {
        Self::new(
            "incompatible_protocol",
            "This desktop client is incompatible with Workboard.",
        )
    }

    pub fn forbidden_window() -> Self {
        Self::new(
            "ipc_window_forbidden",
            "This window cannot access the Workboard bridge.",
        )
    }

    pub fn unsafe_payload() -> Self {
        Self::new(
            "unsafe_daemon_payload",
            "Workboard returned an unsafe payload.",
        )
    }
}

pub fn validate_request<T: Serialize>(request: &T) -> Result<(), BridgeError> {
    let bytes = serde_json::to_vec(request).map_err(|_| BridgeError::invalid_request())?;
    if bytes.len() > MAX_IPC_REQUEST_BYTES {
        return Err(BridgeError::request_too_large());
    }
    Ok(())
}
