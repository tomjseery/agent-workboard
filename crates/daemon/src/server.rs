use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

use crate::client::DaemonClient;
use crate::error::DaemonError;
use crate::protocol::{
    MAX_MESSAGE_BYTES, PROTOCOL_VERSION, RemoteError, RequestEnvelope, ResponseEnvelope,
    WriteCommand,
};
use crate::watcher::{self, WatchConfig};

pub trait CommandHandler: Send + 'static {
    fn handle(&mut self, command: WriteCommand) -> Result<Value, RemoteError>;
}

impl<F> CommandHandler for F
where
    F: FnMut(WriteCommand) -> Result<Value, RemoteError> + Send + 'static,
{
    fn handle(&mut self, command: WriteCommand) -> Result<Value, RemoteError> {
        self(command)
    }
}

pub(crate) struct WriterRequest {
    command: WriteCommand,
    response: Sender<ResponseEnvelope>,
}

pub struct DaemonServer {
    address: SocketAddr,
    token: String,
    stopping: Arc<AtomicBool>,
    listener_thread: Option<JoinHandle<()>>,
    writer_sender: Option<Sender<WriterRequest>>,
    writer_thread: Option<JoinHandle<()>>,
    watcher_thread: Option<JoinHandle<()>>,
}

impl DaemonServer {
    pub fn start<H>(
        handler: H,
        address: SocketAddr,
        token: impl Into<String>,
    ) -> Result<Self, DaemonError>
    where
        H: CommandHandler,
    {
        if !address.ip().is_loopback() {
            return Err(DaemonError::NonLoopbackAddress(address));
        }
        let token = token.into();
        if token.is_empty() || token.chars().any(char::is_control) {
            return Err(DaemonError::InvalidToken);
        }
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stopping = Arc::new(AtomicBool::new(false));
        let (writer_sender, writer_receiver) = mpsc::channel();
        let writer_thread = thread::spawn(move || writer_loop(handler, writer_receiver));
        let listener_stopping = Arc::clone(&stopping);
        let listener_token = token.clone();
        let listener_writer = writer_sender.clone();
        let listener_thread = thread::spawn(move || {
            listener_loop(listener, listener_token, listener_writer, listener_stopping);
        });
        Ok(Self {
            address,
            token,
            stopping,
            listener_thread: Some(listener_thread),
            writer_sender: Some(writer_sender),
            writer_thread: Some(writer_thread),
            watcher_thread: None,
        })
    }

    pub fn enable_watcher(&mut self, watch: WatchConfig) -> Result<(), DaemonError> {
        if self.watcher_thread.is_some() {
            return Err(DaemonError::WatcherUnavailable);
        }
        let writer = self
            .writer_sender
            .as_ref()
            .ok_or(DaemonError::WatcherUnavailable)?
            .clone();
        let stopping = Arc::clone(&self.stopping);
        self.watcher_thread = Some(thread::spawn(move || {
            watcher::watch_loop(watch, writer, stopping);
        }));
        Ok(())
    }

    pub fn client(&self) -> DaemonClient {
        DaemonClient::new(self.address, &self.token)
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn descriptor(&self) -> crate::endpoint::EndpointDescriptor {
        crate::endpoint::EndpointDescriptor {
            protocol_version: PROTOCOL_VERSION,
            address: self.address,
            token: self.token.clone(),
        }
    }

    pub fn wait(mut self) -> Result<(), DaemonError> {
        self.join()
    }

    fn join(&mut self) -> Result<(), DaemonError> {
        if let Some(thread) = self.listener_thread.take() {
            thread.join().map_err(|_| DaemonError::ServerThreadFailed)?;
        }
        if let Some(thread) = self.watcher_thread.take() {
            thread.join().map_err(|_| DaemonError::ServerThreadFailed)?;
        }
        self.writer_sender.take();
        if let Some(thread) = self.writer_thread.take() {
            thread.join().map_err(|_| DaemonError::ServerThreadFailed)?;
        }
        Ok(())
    }
}

impl Drop for DaemonServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
        let _ = self.join();
    }
}

fn listener_loop(
    listener: TcpListener,
    token: String,
    writer: Sender<WriterRequest>,
    stopping: Arc<AtomicBool>,
) {
    while !stopping.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => handle_connection(stream, &token, &writer),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) if transient_accept_error(&error) => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn transient_accept_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    )
}

fn handle_connection(mut stream: TcpStream, token: &str, writer: &Sender<WriterRequest>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let response = read_request(&mut stream)
        .and_then(|request| validate_request(request, token))
        .and_then(|command| send_to_writer(command, writer))
        .unwrap_or_else(|error| error);
    if let Ok(body) = serde_json::to_vec(&response) {
        let _ = stream.write_all(&body);
        let _ = stream.shutdown(Shutdown::Write);
    }
}

fn read_request(stream: &mut TcpStream) -> Result<RequestEnvelope, ResponseEnvelope> {
    let mut body = Vec::new();
    stream
        .take(MAX_MESSAGE_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| ResponseEnvelope::failure("protocol_io", error.to_string()))?;
    if body.len() as u64 > MAX_MESSAGE_BYTES {
        return Err(ResponseEnvelope::failure(
            "message_too_large",
            "daemon request exceeds the message bound",
        ));
    }
    serde_json::from_slice(&body)
        .map_err(|error| ResponseEnvelope::failure("invalid_request", error.to_string()))
}

fn validate_request(
    request: RequestEnvelope,
    token: &str,
) -> Result<WriteCommand, ResponseEnvelope> {
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(ResponseEnvelope::failure(
            "unsupported_protocol",
            format!(
                "protocol version {} is unsupported",
                request.protocol_version
            ),
        ));
    }
    if request.token != token {
        return Err(ResponseEnvelope::failure(
            "authentication_failed",
            "daemon authentication failed",
        ));
    }
    Ok(request.command)
}

pub(crate) fn send_to_writer(
    command: WriteCommand,
    writer: &Sender<WriterRequest>,
) -> Result<ResponseEnvelope, ResponseEnvelope> {
    let (response_sender, response_receiver) = mpsc::channel();
    writer
        .send(WriterRequest {
            command,
            response: response_sender,
        })
        .map_err(|_| ResponseEnvelope::failure("writer_stopped", "daemon writer stopped"))?;
    response_receiver
        .recv()
        .map_err(|_| ResponseEnvelope::failure("writer_stopped", "daemon writer stopped"))
}

fn writer_loop<H>(mut handler: H, requests: Receiver<WriterRequest>)
where
    H: CommandHandler,
{
    for request in requests {
        let response = match request.command {
            WriteCommand::Ping => ResponseEnvelope::success(json!({ "status": "ready" })),
            command => handler
                .handle(command)
                .map(ResponseEnvelope::success)
                .unwrap_or_else(|error| ResponseEnvelope::failure(error.code, error.message)),
        };
        let _ = request.response.send(response);
    }
}
