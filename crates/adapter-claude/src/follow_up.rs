use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use workboard_native::{ScanLimits, discover_jsonl_files, stream_jsonl};

const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeFollowUpRequest {
    pub executable: PathBuf,
    pub native_id: String,
    pub working_directory: PathBuf,
    pub capability_bundle_root: PathBuf,
    pub workflow_token: String,
    pub text: String,
    pub client_message_id: String,
    pub active_turn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeFollowUpFailureKind {
    Deferred,
    Rejected,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeFollowUpFailure {
    pub kind: ClaudeFollowUpFailureKind,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ClaudeFollowUpClient {
    timeout: Duration,
}

impl Default for ClaudeFollowUpClient {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30 * 60),
        }
    }
}

impl ClaudeFollowUpClient {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub fn deliver(
        &self,
        request: &ClaudeFollowUpRequest,
    ) -> Result<String, ClaudeFollowUpFailure> {
        if request.active_turn {
            return Err(failure(
                ClaudeFollowUpFailureKind::Deferred,
                "the Claude turn is active; the follow-up remains queued",
            ));
        }
        let mut child = Command::new(&request.executable)
            .current_dir(&request.working_directory)
            .env("CLAUDE_CONFIG_DIR", &request.capability_bundle_root)
            .env(
                workboard_core::WORKBOARD_WORKFLOW_TOKEN_ENV,
                &request.workflow_token,
            )
            .args([
                "--print",
                "--resume",
                request.native_id.as_str(),
                "--output-format",
                "json",
                "--",
                request.text.as_str(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                failure(
                    ClaudeFollowUpFailureKind::Rejected,
                    format!("failed to start Claude follow-up: {error}"),
                )
            })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            failure(
                ClaudeFollowUpFailureKind::Rejected,
                "Claude follow-up stdout was unavailable",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            failure(
                ClaudeFollowUpFailureKind::Rejected,
                "Claude follow-up stderr was unavailable",
            )
        })?;
        let stdout_reader = thread::spawn(move || read_bounded(stdout));
        let stderr_reader = thread::spawn(move || read_bounded(stderr));
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(failure(
                        ClaudeFollowUpFailureKind::Uncertain,
                        "Claude follow-up timed out",
                    ));
                }
                Err(error) => {
                    return Err(failure(
                        ClaudeFollowUpFailureKind::Uncertain,
                        format!("failed while waiting for Claude follow-up: {error}"),
                    ));
                }
            }
        };
        let stdout = stdout_reader
            .join()
            .map_err(|_| {
                failure(
                    ClaudeFollowUpFailureKind::Uncertain,
                    "Claude stdout reader failed",
                )
            })?
            .map_err(|error| failure(ClaudeFollowUpFailureKind::Uncertain, error.to_string()))?;
        let stderr = stderr_reader
            .join()
            .map_err(|_| {
                failure(
                    ClaudeFollowUpFailureKind::Uncertain,
                    "Claude stderr reader failed",
                )
            })?
            .map_err(|error| failure(ClaudeFollowUpFailureKind::Uncertain, error.to_string()))?;
        if !status.success() {
            return Err(failure(
                ClaudeFollowUpFailureKind::Rejected,
                format!(
                    "Claude rejected the follow-up: {}",
                    clean_text(&String::from_utf8_lossy(&stderr), 512)
                ),
            ));
        }
        let response: Value = serde_json::from_slice(&stdout).map_err(|error| {
            failure(
                ClaudeFollowUpFailureKind::Uncertain,
                format!("Claude returned an unsupported receipt: {error}"),
            )
        })?;
        if response.get("is_error").and_then(Value::as_bool) == Some(true)
            || response.get("session_id").and_then(Value::as_str)
                != Some(request.native_id.as_str())
        {
            return Err(failure(
                ClaudeFollowUpFailureKind::Rejected,
                "Claude did not acknowledge the exact resumed session",
            ));
        }
        Ok(json!({
            "provider": "claude",
            "clientMessageId": request.client_message_id,
            "accepted": true
        })
        .to_string())
    }

    pub fn reconcile(
        &self,
        request: &ClaudeFollowUpRequest,
    ) -> Result<Option<String>, ClaudeFollowUpFailure> {
        let root = request.capability_bundle_root.join("projects");
        if !root.is_dir() {
            return Ok(None);
        }
        let limits = ScanLimits::default();
        let files = discover_jsonl_files(&root, limits.max_sources).map_err(|error| {
            failure(
                ClaudeFollowUpFailureKind::Uncertain,
                format!("failed to reconcile Claude follow-up: {}", error.message),
            )
        })?;
        for path in files {
            let stream = stream_jsonl(&path, None, limits).map_err(|error| {
                failure(
                    ClaudeFollowUpFailureKind::Uncertain,
                    format!("failed to reconcile Claude follow-up: {}", error.message),
                )
            })?;
            if stream.records.iter().any(|record| {
                record.value.get("sessionId").and_then(Value::as_str)
                    == Some(request.native_id.as_str())
                    && contains_exact_text(&record.value, &request.text)
            }) {
                return Ok(Some(
                    json!({
                        "provider": "claude",
                        "clientMessageId": request.client_message_id,
                        "accepted": true
                    })
                    .to_string(),
                ));
            }
        }
        Ok(None)
    }
}

fn read_bounded(input: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    input.take(MAX_OUTPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        return Err(std::io::Error::other("provider output exceeded 4 MiB"));
    }
    Ok(bytes)
}

fn contains_exact_text(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(value) => value == expected,
        Value::Array(values) => values
            .iter()
            .any(|value| contains_exact_text(value, expected)),
        Value::Object(values) => values
            .values()
            .any(|value| contains_exact_text(value, expected)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn clean_text(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn failure(kind: ClaudeFollowUpFailureKind, message: impl Into<String>) -> ClaudeFollowUpFailure {
    ClaudeFollowUpFailure {
        kind,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tempfile::TempDir;

    use super::{ClaudeFollowUpClient, ClaudeFollowUpRequest};

    #[test]
    fn reconciles_an_exact_follow_up_without_native_identity_in_the_receipt() {
        let directory = TempDir::new().expect("temporary directory");
        let projects = directory.path().join("projects/project");
        fs::create_dir_all(&projects).expect("projects directory");
        fs::write(
            projects.join("session.jsonl"),
            "{\"type\":\"user\",\"sessionId\":\"native-secret\",\"message\":{\"content\":\"continue safely\"}}\n",
        )
        .expect("transcript fixture");
        let request = ClaudeFollowUpRequest {
            executable: "claude".into(),
            native_id: "native-secret".to_owned(),
            working_directory: directory.path().to_path_buf(),
            capability_bundle_root: directory.path().to_path_buf(),
            workflow_token: "workflow-token".to_owned(),
            text: "continue safely".to_owned(),
            client_message_id: "follow-up-key".to_owned(),
            active_turn: false,
        };

        let receipt = ClaudeFollowUpClient::with_timeout(Duration::from_secs(1))
            .reconcile(&request)
            .expect("reconcile")
            .expect("receipt");

        assert!(!receipt.contains("native-secret"));
        assert!(receipt.contains("follow-up-key"));
    }

    #[cfg(windows)]
    #[test]
    fn delivers_with_one_argument_vector_and_returns_an_opaque_receipt() {
        let directory = TempDir::new().expect("temporary directory");
        let executable = directory.path().join("claude.cmd");
        fs::write(
            &executable,
            "@echo off\r\necho {\"session_id\":\"%3\",\"is_error\":false}\r\n",
        )
        .expect("fake Claude executable");
        let request = ClaudeFollowUpRequest {
            executable,
            native_id: "native-secret".to_owned(),
            working_directory: std::env::current_dir().expect("current directory"),
            capability_bundle_root: directory.path().to_path_buf(),
            workflow_token: "workflow-token".to_owned(),
            text: "--hostile prompt & still one argument".to_owned(),
            client_message_id: "follow-up-key".to_owned(),
            active_turn: false,
        };

        let receipt = ClaudeFollowUpClient::with_timeout(Duration::from_secs(5))
            .deliver(&request)
            .expect("deliver follow-up");

        assert!(!receipt.contains("native-secret"));
        assert!(receipt.contains("follow-up-key"));
    }
}
