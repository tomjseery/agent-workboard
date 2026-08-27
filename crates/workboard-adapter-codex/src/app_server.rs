use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use workboard_core::LiveStatus;
use workboard_native::{ConversationKind, NativeConversation};

pub const APP_SERVER_ADAPTER_VERSION: &str = "codex-app-server-v2-2026-08";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodexAppServerLimits {
    pub max_message_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_messages: usize,
    pub max_pages: usize,
    pub max_threads: usize,
    pub max_preview_chars: usize,
    pub page_size: u32,
    pub request_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub restart_attempts: usize,
}

impl Default for CodexAppServerLimits {
    fn default() -> Self {
        Self {
            max_message_bytes: 4 * 1024 * 1024,
            max_stderr_bytes: 256 * 1024,
            max_messages: 50_000,
            max_pages: 1_000,
            max_threads: 10_000,
            max_preview_chars: 280,
            page_size: 100,
            request_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(2),
            restart_attempts: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexAppServerFailureKind {
    Io,
    Timeout,
    ProcessExited,
    Protocol,
    MessageTooLarge,
    StderrTooLarge,
    MessageLimitExceeded,
    PageLimitExceeded,
    ThreadLimitExceeded,
    UnsupportedSchema,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAppServerFailure {
    pub kind: CodexAppServerFailureKind,
    pub message: String,
}

impl std::fmt::Display for CodexAppServerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CodexAppServerFailure {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAppServerSnapshot {
    pub threads: Vec<CodexAppServerThread>,
    pub status_observations: Vec<CodexAppServerStatusObservation>,
    pub server_version: Option<String>,
    pub notifications_received: usize,
    pub restart_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAppServerThread {
    pub conversation: NativeConversation,
    pub status: LiveStatus,
    pub source: String,
    pub owned_by_connection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAppServerStatusObservation {
    pub native_id: String,
    pub status: LiveStatus,
    pub owned_by_connection: bool,
}

#[derive(Debug, Clone)]
pub struct CodexAppServerClient {
    executable: PathBuf,
    arguments: Vec<OsString>,
    limits: CodexAppServerLimits,
}

impl CodexAppServerClient {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            arguments: vec!["app-server".into(), "--stdio".into()],
            limits: CodexAppServerLimits::default(),
        }
    }

    pub fn with_arguments(
        executable: impl Into<PathBuf>,
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
        limits: CodexAppServerLimits,
    ) -> Self {
        Self {
            executable: executable.into(),
            arguments: arguments.into_iter().map(Into::into).collect(),
            limits,
        }
    }

    pub fn discover(&self) -> Result<CodexAppServerSnapshot, CodexAppServerFailure> {
        let attempts = self.limits.restart_attempts.saturating_add(1);
        let mut last_failure = None;
        for attempt in 0..attempts {
            match Connection::start(&self.executable, &self.arguments, self.limits)
                .and_then(Connection::discover)
            {
                Ok(mut snapshot) => {
                    snapshot.restart_count = attempt;
                    return Ok(snapshot);
                }
                Err(failure) => last_failure = Some(failure),
            }
        }
        Err(last_failure.unwrap_or_else(|| {
            failure(
                CodexAppServerFailureKind::Protocol,
                "Codex app-server discovery had no permitted attempts",
            )
        }))
    }
}

struct Connection {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Result<Vec<u8>, CodexAppServerFailure>>,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<std::io::Result<BoundedStderr>>>,
    limits: CodexAppServerLimits,
    next_id: u64,
    messages_received: usize,
    notifications_received: usize,
    status_observations: Vec<CodexAppServerStatusObservation>,
}

impl Connection {
    fn start(
        executable: &Path,
        arguments: &[OsString],
        limits: CodexAppServerLimits,
    ) -> Result<Self, CodexAppServerFailure> {
        let mut child = Command::new(executable)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| io_failure("starting Codex app-server", error))?;
        let stdin = child.stdin.take().ok_or_else(|| {
            failure(
                CodexAppServerFailureKind::Io,
                "Codex app-server stdin was unavailable",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            failure(
                CodexAppServerFailureKind::Io,
                "Codex app-server stdout was unavailable",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            failure(
                CodexAppServerFailureKind::Io,
                "Codex app-server stderr was unavailable",
            )
        })?;
        let (message_sender, messages) = mpsc::sync_channel(1);
        let stdout_reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let message = read_bounded_line(&mut reader, limits.max_message_bytes);
                let finished = matches!(&message, Ok(value) if value.is_empty());
                if message_sender.send(message).is_err() || finished {
                    break;
                }
            }
        });
        let stderr_reader =
            thread::spawn(move || read_bounded_stderr(stderr, limits.max_stderr_bytes));
        Ok(Self {
            child,
            stdin: Some(stdin),
            messages,
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
            limits,
            next_id: 1,
            messages_received: 0,
            notifications_received: 0,
            status_observations: Vec::new(),
        })
    }

    fn discover(mut self) -> Result<CodexAppServerSnapshot, CodexAppServerFailure> {
        let result = self.discover_inner();
        let stderr = self.shutdown();
        match (result, stderr) {
            (Ok(_), Ok(stderr)) if stderr.oversized => Err(failure(
                CodexAppServerFailureKind::StderrTooLarge,
                "Codex app-server stderr exceeded its byte limit",
            )),
            (Ok(snapshot), Ok(_)) => Ok(snapshot),
            (Ok(_), Err(error)) => Err(error),
            (Err(mut error), Ok(stderr)) => {
                let detail = clean_stderr(&stderr.bytes);
                if !detail.is_empty() {
                    error.message.push_str(": ");
                    error.message.push_str(&detail);
                }
                Err(error)
            }
            (Err(error), Err(_)) => Err(error),
        }
    }

    fn discover_inner(&mut self) -> Result<CodexAppServerSnapshot, CodexAppServerFailure> {
        let initialized: InitializeResponse = self.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "agent_workboard",
                    "title": "Agent Workboard",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {}
            }),
        )?;
        self.notify("initialized")?;
        let mut cursor = None;
        let mut cursors = HashSet::new();
        let mut identities = HashSet::new();
        let mut listed = Vec::new();
        for _ in 0..self.limits.max_pages {
            let response: ThreadListResponse = self.request(
                "thread/list",
                json!({
                    "cursor": cursor,
                    "limit": self.limits.page_size,
                    "sortKey": "updated_at",
                    "sortDirection": "desc",
                    "sourceKinds": [
                        "cli", "vscode", "exec", "appServer", "subAgent",
                        "subAgentReview", "subAgentCompact", "subAgentThreadSpawn",
                        "subAgentOther", "unknown"
                    ]
                }),
            )?;
            if listed.len().saturating_add(response.data.len()) > self.limits.max_threads {
                return Err(failure(
                    CodexAppServerFailureKind::ThreadLimitExceeded,
                    "Codex app-server thread snapshot exceeded its thread limit",
                ));
            }
            for thread in &response.data {
                if !identities.insert(thread.id.clone()) {
                    return Err(failure(
                        CodexAppServerFailureKind::Protocol,
                        "Codex app-server returned a duplicate thread identity",
                    ));
                }
            }
            listed.extend(response.data);
            match response.next_cursor {
                Some(next) if !next.is_empty() => {
                    if !cursors.insert(next.clone()) {
                        return Err(failure(
                            CodexAppServerFailureKind::Protocol,
                            "Codex app-server repeated a pagination cursor",
                        ));
                    }
                    cursor = Some(next);
                }
                _ => {
                    let mut threads = Vec::with_capacity(listed.len());
                    for listed_thread in listed {
                        let response: ThreadReadResponse = self.request(
                            "thread/read",
                            json!({
                                "threadId": listed_thread.id,
                                "includeTurns": false
                            }),
                        )?;
                        if response.thread.id != listed_thread.id {
                            return Err(failure(
                                CodexAppServerFailureKind::Protocol,
                                "Codex app-server thread/read returned a different identity",
                            ));
                        }
                        if !response.thread.ephemeral {
                            threads
                                .push(map_thread(response.thread, self.limits.max_preview_chars)?);
                        }
                    }
                    return Ok(CodexAppServerSnapshot {
                        threads,
                        status_observations: self.status_observations.clone(),
                        server_version: initialized
                            .server_info
                            .and_then(|server| server.version)
                            .or(initialized.user_agent),
                        notifications_received: self.notifications_received,
                        restart_count: 0,
                    });
                }
            }
        }
        Err(failure(
            CodexAppServerFailureKind::PageLimitExceeded,
            "Codex app-server thread pagination exceeded its page limit",
        ))
    }

    fn request<T: for<'de> Deserialize<'de>>(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<T, CodexAppServerFailure> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
            failure(
                CodexAppServerFailureKind::MessageLimitExceeded,
                "Codex app-server request identity overflowed",
            )
        })?;
        self.send(&json!({ "id": id, "method": method, "params": params }))?;
        let deadline = Instant::now() + self.limits.request_timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(failure(
                    CodexAppServerFailureKind::Timeout,
                    format!("Codex app-server {method} request timed out"),
                ));
            }
            let line = match self.messages.recv_timeout(remaining) {
                Ok(line) => line?,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(failure(
                        CodexAppServerFailureKind::Timeout,
                        format!("Codex app-server {method} request timed out"),
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(failure(
                        CodexAppServerFailureKind::ProcessExited,
                        "Codex app-server output closed unexpectedly",
                    ));
                }
            };
            if line.is_empty() {
                return Err(failure(
                    CodexAppServerFailureKind::ProcessExited,
                    "Codex app-server exited before completing discovery",
                ));
            }
            self.messages_received += 1;
            if self.messages_received > self.limits.max_messages {
                return Err(failure(
                    CodexAppServerFailureKind::MessageLimitExceeded,
                    "Codex app-server exceeded its message count limit",
                ));
            }
            let envelope: RpcEnvelope = serde_json::from_slice(&line).map_err(|error| {
                failure(
                    CodexAppServerFailureKind::Protocol,
                    format!("Codex app-server returned malformed JSON: {error}"),
                )
            })?;
            if let Some(response_id) = envelope.id {
                if envelope.method.is_some() {
                    return Err(failure(
                        CodexAppServerFailureKind::Protocol,
                        "Codex app-server sent an unsupported server request",
                    ));
                }
                if response_id != id {
                    return Err(failure(
                        CodexAppServerFailureKind::Protocol,
                        "Codex app-server returned an unexpected response identity",
                    ));
                }
                if let Some(error) = envelope.error {
                    return Err(failure(
                        CodexAppServerFailureKind::UnsupportedSchema,
                        format!(
                            "Codex app-server rejected {method} with {}: {}",
                            error.code,
                            clean_text(&error.message, 512)
                        ),
                    ));
                }
                let result = envelope.result.ok_or_else(|| {
                    failure(
                        CodexAppServerFailureKind::Protocol,
                        "Codex app-server response contained neither result nor error",
                    )
                })?;
                return serde_json::from_value(result).map_err(|error| {
                    failure(
                        CodexAppServerFailureKind::UnsupportedSchema,
                        format!("Codex app-server {method} response is unsupported: {error}"),
                    )
                });
            }
            let Some(notification_method) = envelope.method else {
                return Err(failure(
                    CodexAppServerFailureKind::Protocol,
                    "Codex app-server message was neither a response nor a notification",
                ));
            };
            if notification_method == "thread/status/changed" {
                let notification: ThreadStatusChanged =
                    serde_json::from_value(envelope.params.ok_or_else(|| {
                        failure(
                            CodexAppServerFailureKind::UnsupportedSchema,
                            "Codex app-server status notification had no parameters",
                        )
                    })?)
                    .map_err(|error| {
                        failure(
                            CodexAppServerFailureKind::UnsupportedSchema,
                            format!("Codex app-server status notification is unsupported: {error}"),
                        )
                    })?;
                self.status_observations
                    .push(CodexAppServerStatusObservation {
                        native_id: bounded_required(
                            "status notification thread identity",
                            notification.thread_id,
                            256,
                        )?,
                        status: live_status(notification.status),
                        owned_by_connection: false,
                    });
            }
            self.notifications_received += 1;
        }
    }

    fn notify(&mut self, method: &str) -> Result<(), CodexAppServerFailure> {
        self.send(&json!({ "method": method }))
    }

    fn send(&mut self, value: &Value) -> Result<(), CodexAppServerFailure> {
        let mut bytes = serde_json::to_vec(value).map_err(|error| {
            failure(
                CodexAppServerFailureKind::Protocol,
                format!("failed to encode Codex app-server request: {error}"),
            )
        })?;
        if bytes.len() > self.limits.max_message_bytes {
            return Err(failure(
                CodexAppServerFailureKind::MessageTooLarge,
                "Codex app-server request exceeded its message byte limit",
            ));
        }
        bytes.push(b'\n');
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            failure(
                CodexAppServerFailureKind::ProcessExited,
                "Codex app-server stdin was already closed",
            )
        })?;
        stdin
            .write_all(&bytes)
            .and_then(|()| stdin.flush())
            .map_err(|error| io_failure("writing to Codex app-server", error))
    }

    fn shutdown(&mut self) -> Result<BoundedStderr, CodexAppServerFailure> {
        drop(self.stdin.take());
        let (_, replacement) = mpsc::channel();
        drop(std::mem::replace(&mut self.messages, replacement));
        let deadline = Instant::now() + self.limits.shutdown_timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Ok(None) => {
                    self.child
                        .kill()
                        .map_err(|error| io_failure("stopping Codex app-server", error))?;
                    self.child
                        .wait()
                        .map_err(|error| io_failure("waiting for Codex app-server", error))?;
                    break;
                }
                Err(error) => return Err(io_failure("waiting for Codex app-server", error)),
            }
        }
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
        self.stderr_reader
            .take()
            .ok_or_else(|| {
                failure(
                    CodexAppServerFailureKind::Io,
                    "Codex app-server stderr reader was unavailable",
                )
            })?
            .join()
            .map_err(|_| {
                failure(
                    CodexAppServerFailureKind::Io,
                    "Codex app-server stderr reader failed",
                )
            })?
            .map_err(|error| io_failure("reading Codex app-server stderr", error))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResponse {
    server_info: Option<ServerInfo>,
    user_agent: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServerInfo {
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadListResponse {
    data: Vec<WireThread>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ThreadReadResponse {
    thread: WireThread,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireThread {
    id: String,
    preview: String,
    name: Option<String>,
    cwd: String,
    created_at: i64,
    updated_at: i64,
    cli_version: String,
    parent_thread_id: Option<String>,
    forked_from_id: Option<String>,
    source: Value,
    status: WireStatus,
    ephemeral: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WireStatus {
    NotLoaded,
    Idle,
    Active,
    SystemError,
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope {
    id: Option<u64>,
    method: Option<String>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStatusChanged {
    thread_id: String,
    status: WireStatus,
}

struct BoundedStderr {
    bytes: Vec<u8>,
    oversized: bool,
}

fn read_bounded_stderr(mut input: impl Read, limit: usize) -> std::io::Result<BoundedStderr> {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    let mut oversized = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_add(1).saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..count.min(remaining)]);
        oversized |= count > remaining || bytes.len() > limit;
    }
    bytes.truncate(limit);
    Ok(BoundedStderr { bytes, oversized })
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    limit: usize,
) -> Result<Vec<u8>, CodexAppServerFailure> {
    let mut line = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|error| io_failure("reading Codex app-server stdout", error))?;
        if buffer.is_empty() {
            return Ok(line);
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(buffer.len(), |index| index + 1);
        if line.len().saturating_add(count) > limit.saturating_add(1) {
            return Err(failure(
                CodexAppServerFailureKind::MessageTooLarge,
                "Codex app-server response exceeded its message byte limit",
            ));
        }
        line.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if newline.is_some() {
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            if line.len() > limit {
                return Err(failure(
                    CodexAppServerFailureKind::MessageTooLarge,
                    "Codex app-server response exceeded its message byte limit",
                ));
            }
            return Ok(line);
        }
    }
}

fn map_thread(
    wire: WireThread,
    max_preview_chars: usize,
) -> Result<CodexAppServerThread, CodexAppServerFailure> {
    let native_id = bounded_required("thread identity", wire.id, 256)?;
    let source = source_name(&wire.source)?;
    let parent = wire
        .parent_thread_id
        .map(|value| bounded_required("parent thread identity", value, 256))
        .transpose()?;
    let kind = if parent.is_some() || source == "exec" || source == "subAgent" {
        ConversationKind::Helper
    } else {
        ConversationKind::TopLevel
    };
    let created_at = OffsetDateTime::from_unix_timestamp(wire.created_at).map_err(|error| {
        failure(
            CodexAppServerFailureKind::UnsupportedSchema,
            format!("Codex app-server thread creation time is invalid: {error}"),
        )
    })?;
    let updated_at = OffsetDateTime::from_unix_timestamp(wire.updated_at).map_err(|error| {
        failure(
            CodexAppServerFailureKind::UnsupportedSchema,
            format!("Codex app-server thread update time is invalid: {error}"),
        )
    })?;
    let preview = clean_text(&wire.preview, max_preview_chars);
    let mut conversation = NativeConversation::new(native_id, kind);
    conversation.parent_native_id = parent;
    conversation.forked_from_native_id = wire
        .forked_from_id
        .map(|value| bounded_required("fork source identity", value, 256))
        .transpose()?;
    conversation.native_created_at = Some(created_at);
    conversation.observe_activity(Some(created_at));
    conversation.observe_activity(Some(updated_at));
    conversation.title = wire
        .name
        .map(|value| clean_text(&value, 512))
        .filter(|value| !value.is_empty());
    if !preview.is_empty() {
        conversation.observe_prompt(preview);
    }
    conversation.tool_version = Some(clean_text(&wire.cli_version, 128));
    let cwd = bounded_required("thread cwd", wire.cwd, 32 * 1024)?;
    if !Path::new(&cwd).is_absolute() {
        return Err(failure(
            CodexAppServerFailureKind::UnsupportedSchema,
            "Codex app-server thread cwd was not absolute",
        ));
    }
    conversation.observe_cwd(cwd, Some(updated_at));
    let status = live_status(wire.status);
    Ok(CodexAppServerThread {
        conversation,
        status,
        source,
        owned_by_connection: false,
    })
}

fn live_status(status: WireStatus) -> LiveStatus {
    match status {
        WireStatus::NotLoaded => LiveStatus::NotLoaded,
        WireStatus::Idle => LiveStatus::Idle,
        WireStatus::Active => LiveStatus::Active,
        WireStatus::SystemError => LiveStatus::SystemError,
    }
}

fn source_name(source: &Value) -> Result<String, CodexAppServerFailure> {
    if let Some(source) = source.as_str() {
        return bounded_required("thread source", source.to_owned(), 128);
    }
    if source.get("subAgent").is_some() {
        return Ok("subAgent".to_owned());
    }
    if source.get("custom").and_then(Value::as_str).is_some() {
        return Ok("custom".to_owned());
    }
    Err(failure(
        CodexAppServerFailureKind::UnsupportedSchema,
        "Codex app-server thread source shape is unsupported",
    ))
}

fn bounded_required(
    label: &str,
    value: String,
    max_chars: usize,
) -> Result<String, CodexAppServerFailure> {
    let value = clean_text(&value, max_chars.saturating_add(1));
    if value.is_empty() {
        return Err(failure(
            CodexAppServerFailureKind::UnsupportedSchema,
            format!("Codex app-server {label} was empty"),
        ));
    }
    if value.chars().count() > max_chars {
        return Err(failure(
            CodexAppServerFailureKind::UnsupportedSchema,
            format!("Codex app-server {label} exceeded its character limit"),
        ));
    }
    Ok(value)
}

fn clean_text(value: &str, max_chars: usize) -> String {
    let mut result = String::new();
    for part in value.split_whitespace() {
        let separator = usize::from(!result.is_empty());
        let remaining = max_chars.saturating_sub(result.chars().count() + separator);
        if remaining == 0 {
            break;
        }
        if separator == 1 {
            result.push(' ');
        }
        result.extend(
            part.chars()
                .filter(|character| !character.is_control())
                .take(remaining),
        );
    }
    result
}

fn clean_stderr(bytes: &[u8]) -> String {
    clean_text(&String::from_utf8_lossy(bytes), 512)
}

fn io_failure(operation: &str, error: std::io::Error) -> CodexAppServerFailure {
    failure(
        CodexAppServerFailureKind::Io,
        format!("I/O failed while {operation}: {error}"),
    )
}

fn failure(kind: CodexAppServerFailureKind, message: impl Into<String>) -> CodexAppServerFailure {
    CodexAppServerFailure {
        kind,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use workboard_core::LiveStatus;

    use super::{
        CodexAppServerFailureKind, ConversationKind, WireThread, map_thread, read_bounded_line,
    };

    #[test]
    fn maps_the_supported_thread_subset_without_storage_paths() {
        let thread = serde_json::from_value::<WireThread>(json!({
            "id": "thread-one",
            "preview": "  Inspect   the workboard  ",
            "name": "Workboard task",
            "cwd": if cfg!(windows) { "C:/synthetic/repository" } else { "/synthetic/repository" },
            "createdAt": 1_777_000_000,
            "updatedAt": 1_777_000_100,
            "cliVersion": "0.146.0",
            "parentThreadId": null,
            "forkedFromId": null,
            "source": "cli",
            "status": { "type": "notLoaded" },
            "ephemeral": false,
            "path": "C:/must-not-be-consumed/session.jsonl",
            "unknownStableField": true
        }))
        .expect("supported thread fixture");

        let mapped = map_thread(thread, 280).expect("thread mapping");

        assert_eq!(mapped.conversation.native_id, "thread-one");
        assert_eq!(mapped.conversation.kind, ConversationKind::TopLevel);
        assert_eq!(
            mapped.conversation.first_prompt_preview.as_deref(),
            Some("Inspect the workboard")
        );
        assert_eq!(mapped.status, LiveStatus::NotLoaded);
        assert!(!mapped.owned_by_connection);
    }

    #[test]
    fn maps_parented_threads_as_helpers() {
        let thread = serde_json::from_value::<WireThread>(json!({
            "id": "thread-child",
            "preview": "Child work",
            "name": null,
            "cwd": if cfg!(windows) { "C:/synthetic/repository" } else { "/synthetic/repository" },
            "createdAt": 1_777_000_000,
            "updatedAt": 1_777_000_100,
            "cliVersion": "0.146.0",
            "parentThreadId": "thread-parent",
            "forkedFromId": null,
            "source": { "subAgent": { "thread_spawn": { "depth": 1, "parent_thread_id": "thread-parent" } } },
            "status": { "type": "idle" },
            "ephemeral": false
        }))
        .expect("supported child fixture");

        let mapped = map_thread(thread, 280).expect("thread mapping");

        assert_eq!(mapped.conversation.kind, ConversationKind::Helper);
        assert_eq!(
            mapped.conversation.parent_native_id.as_deref(),
            Some("thread-parent")
        );
    }

    #[test]
    fn rejects_unsupported_status_schema() {
        let result = serde_json::from_value::<WireThread>(json!({
            "id": "thread-one",
            "preview": "Work",
            "name": null,
            "cwd": if cfg!(windows) { "C:/synthetic/repository" } else { "/synthetic/repository" },
            "createdAt": 1_777_000_000,
            "updatedAt": 1_777_000_100,
            "cliVersion": "0.146.0",
            "parentThreadId": null,
            "forkedFromId": null,
            "source": "cli",
            "status": { "type": "futureStatus" },
            "ephemeral": false
        }));

        assert!(result.is_err());
    }

    #[test]
    fn rejects_an_oversized_line_before_unbounded_allocation() {
        let mut input = std::io::Cursor::new(b"123456789\n".as_slice());
        let failure = read_bounded_line(&mut input, 8).expect_err("oversized line");

        assert_eq!(failure.kind, CodexAppServerFailureKind::MessageTooLarge);
    }
}
