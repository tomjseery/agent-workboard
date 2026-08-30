use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::Value;

use crate::error::DaemonError;
use crate::protocol::{
    LEGACY_PROTOCOL_VERSION, MAX_MESSAGE_BYTES, RequestEnvelope, ResponseEnvelope, WriteCommand,
};

#[derive(Debug, Clone)]
pub struct DaemonClient {
    address: SocketAddr,
    token: String,
    timeout: Duration,
}

impl DaemonClient {
    pub fn new(address: SocketAddr, token: impl Into<String>) -> Self {
        Self {
            address,
            token: token.into(),
            timeout: Duration::from_secs(5),
        }
    }

    pub fn request(&self, command: WriteCommand) -> Result<Value, DaemonError> {
        let request = RequestEnvelope {
            protocol_version: LEGACY_PROTOCOL_VERSION,
            token: self.token.clone(),
            command,
        };
        let body = serde_json::to_vec(&request)?;
        if body.len() as u64 > MAX_MESSAGE_BYTES {
            return Err(DaemonError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "daemon request exceeds the message bound",
            )));
        }
        let mut stream = TcpStream::connect_timeout(&self.address, self.timeout)
            .map_err(DaemonError::Unavailable)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        stream.write_all(&body)?;
        stream.shutdown(Shutdown::Write)?;
        let mut response = Vec::new();
        stream
            .take(MAX_MESSAGE_BYTES + 1)
            .read_to_end(&mut response)?;
        if response.len() as u64 > MAX_MESSAGE_BYTES {
            return Err(DaemonError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "daemon response exceeds the message bound",
            )));
        }
        let response: ResponseEnvelope = serde_json::from_slice(&response)?;
        if response.protocol_version != LEGACY_PROTOCOL_VERSION {
            return Err(DaemonError::UnsupportedProtocol(response.protocol_version));
        }
        if let Some(error) = response.error {
            return Err(DaemonError::remote(error));
        }
        response.result.ok_or(DaemonError::MissingResult)
    }
}
