use std::io;

use thiserror::Error;
use workboard_client_protocol::{ProtocolError, ResyncRequirement};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Workboard daemon I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("Workboard daemon protocol encoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Workboard daemon frame length {actual} exceeds the {limit}-byte limit")]
    FrameTooLarge { actual: usize, limit: usize },
    #[error("Workboard daemon sent an empty frame")]
    EmptyFrame,
    #[error("Workboard daemon endpoint is invalid: {0}")]
    InvalidEndpoint(String),
    #[error("Workboard daemon rejected the request: {}: {}", .0.code, .0.message)]
    Remote(Box<ProtocolError>),
    #[error("Workboard daemon returned an unexpected response")]
    UnexpectedResponse,
    #[error("no compatible Workboard read protocol is available")]
    IncompatibleProtocol,
    #[error("Workboard event stream requires authoritative resynchronization")]
    ResyncRequired(ResyncRequirement),
}

impl ClientError {
    pub fn code(&self) -> &str {
        match self {
            Self::Io(_) => "daemon_io",
            Self::Json(_) => "daemon_json",
            Self::FrameTooLarge { .. } => "frame_too_large",
            Self::EmptyFrame => "empty_frame",
            Self::InvalidEndpoint(_) => "invalid_endpoint",
            Self::Remote(error) => &error.code,
            Self::UnexpectedResponse => "unexpected_response",
            Self::IncompatibleProtocol => "incompatible_protocol",
            Self::ResyncRequired(_) => "resync_required",
        }
    }
}
