use std::io;
use std::net::SocketAddr;

use thiserror::Error;

use crate::protocol::RemoteError;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("daemon I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("the daemon endpoint is unavailable: {0}")]
    Unavailable(io::Error),
    #[error("daemon protocol encoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("daemon protocol version {0} is unsupported")]
    UnsupportedProtocol(u32),
    #[error("daemon request was rejected: {}: {}", .0.code, .0.message)]
    Remote(RemoteError),
    #[error("daemon response did not contain a result")]
    MissingResult,
    #[error("daemon writer stopped before completing the request")]
    WriterStopped,
    #[error("daemon server thread failed")]
    ServerThreadFailed,
    #[error("daemon watcher is already running or the writer has stopped")]
    WatcherUnavailable,
    #[error("daemon authentication token is empty or contains control characters")]
    InvalidToken,
    #[error("daemon must bind to a loopback address: {0}")]
    NonLoopbackAddress(SocketAddr),
    #[error("another workboardd instance already owns this Workboard store")]
    AlreadyRunning,
    #[error(
        "a legacy workboardd lock requires explicit recovery before this Workboard store can start"
    )]
    LegacyOwnership,
}

impl DaemonError {
    pub fn remote(error: RemoteError) -> Self {
        Self::Remote(error)
    }

    pub fn code(&self) -> &str {
        match self {
            Self::Remote(error) => &error.code,
            Self::Io(_) => "daemon_io",
            Self::Unavailable(_) => "daemon_unavailable",
            Self::Json(_) => "daemon_json",
            Self::UnsupportedProtocol(_) => "unsupported_protocol",
            Self::MissingResult => "missing_result",
            Self::WriterStopped => "writer_stopped",
            Self::ServerThreadFailed => "server_thread_failed",
            Self::WatcherUnavailable => "daemon_watcher_unavailable",
            Self::InvalidToken => "daemon_invalid_token",
            Self::NonLoopbackAddress(_) => "daemon_non_loopback_address",
            Self::AlreadyRunning => "daemon_already_running",
            Self::LegacyOwnership => "daemon_legacy_ownership",
        }
    }
}
