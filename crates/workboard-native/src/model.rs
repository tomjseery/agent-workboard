use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_core::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    TopLevel,
    Helper,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CwdObservation {
    pub path: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub observed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeConversation {
    pub native_id: String,
    pub kind: ConversationKind,
    pub parent_native_id: Option<String>,
    pub forked_from_native_id: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub native_created_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub first_activity_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_activity_at: Option<OffsetDateTime>,
    pub title: Option<String>,
    pub first_prompt_preview: Option<String>,
    pub last_prompt_preview: Option<String>,
    pub tool_version: Option<String>,
    pub cwd_observations: Vec<CwdObservation>,
    pub compacted: bool,
    pub unknown_record_count: u64,
}

impl NativeConversation {
    pub fn new(native_id: impl Into<String>, kind: ConversationKind) -> Self {
        Self {
            native_id: native_id.into(),
            kind,
            parent_native_id: None,
            forked_from_native_id: None,
            native_created_at: None,
            first_activity_at: None,
            last_activity_at: None,
            title: None,
            first_prompt_preview: None,
            last_prompt_preview: None,
            tool_version: None,
            cwd_observations: Vec::new(),
            compacted: false,
            unknown_record_count: 0,
        }
    }

    pub fn observe_activity(&mut self, timestamp: Option<OffsetDateTime>) {
        let Some(timestamp) = timestamp else {
            return;
        };
        self.first_activity_at = Some(
            self.first_activity_at
                .map_or(timestamp, |current| current.min(timestamp)),
        );
        self.last_activity_at = Some(
            self.last_activity_at
                .map_or(timestamp, |current| current.max(timestamp)),
        );
    }

    pub fn observe_cwd(&mut self, path: String, observed_at: Option<OffsetDateTime>) {
        if self
            .cwd_observations
            .iter()
            .any(|item| item.path == path && item.observed_at == observed_at)
        {
            return;
        }
        if self.cwd_observations.len() == 1_024 {
            self.cwd_observations.remove(0);
        }
        self.cwd_observations
            .push(CwdObservation { path, observed_at });
    }

    pub fn observe_prompt(&mut self, preview: String) {
        if self.first_prompt_preview.is_none() {
            self.first_prompt_preview = Some(preview.clone());
        }
        self.last_prompt_preview = Some(preview);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFingerprint {
    pub head_len: u64,
    pub head_hash: String,
    pub tail_len: u64,
    pub tail_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanCursor {
    pub byte_offset: u64,
    pub source_size: u64,
    pub fingerprint: FileFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceState {
    pub cursor: ScanCursor,
    pub conversation: NativeConversation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanLimits {
    pub max_record_bytes: usize,
    pub max_records: usize,
    pub max_sources: usize,
    pub max_preview_chars: usize,
    pub max_search_chunk_chars: usize,
    pub max_transcript_chars: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_record_bytes: 1024 * 1024,
            max_records: 100_000,
            max_sources: 10_000,
            max_preview_chars: 280,
            max_search_chunk_chars: 8 * 1024,
            max_transcript_chars: 20 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterFailureKind {
    Io,
    InvalidJson,
    RecordTooLarge,
    RecordLimitExceeded,
    SourceLimitExceeded,
    MissingIdentity,
    ConflictingIdentity,
    TranscriptLimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterFailure {
    pub path: PathBuf,
    pub byte_offset: u64,
    pub kind: AdapterFailureKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceScan {
    pub path: PathBuf,
    pub source_size: u64,
    pub modified_at_ns: Option<i128>,
    pub cursor: ScanCursor,
    pub conversation: NativeConversation,
    pub incomplete_tail: bool,
    pub records_read: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterScan {
    pub tool: Tool,
    pub adapter_version: &'static str,
    pub inventory: Vec<PathBuf>,
    pub sources: Vec<SourceScan>,
    pub failures: Vec<AdapterFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptChunk {
    pub ordinal: usize,
    pub text: String,
}

pub struct TranscriptBuilder {
    chunks: Vec<TranscriptChunk>,
    current: String,
    max_chunk_chars: usize,
    max_transcript_chars: usize,
    total_chars: usize,
}

impl TranscriptBuilder {
    pub fn new(max_chunk_chars: usize, max_transcript_chars: usize) -> Self {
        Self {
            chunks: Vec::new(),
            current: String::new(),
            max_chunk_chars,
            max_transcript_chars,
            total_chars: 0,
        }
    }

    pub fn push(&mut self, text: &str, path: &Path, offset: u64) -> Result<(), AdapterFailure> {
        for word in text.split_whitespace() {
            let word = word
                .chars()
                .filter(|character| !character.is_control())
                .collect::<String>();
            if word.is_empty() {
                continue;
            }
            let added = word.chars().count() + usize::from(!self.current.is_empty());
            if self.total_chars.saturating_add(added) > self.max_transcript_chars {
                return Err(AdapterFailure {
                    path: path.to_owned(),
                    byte_offset: offset,
                    kind: AdapterFailureKind::TranscriptLimitExceeded,
                    message: format!(
                        "transcript search text exceeds {} characters",
                        self.max_transcript_chars
                    ),
                });
            }
            if !self.current.is_empty()
                && self.current.chars().count().saturating_add(added) > self.max_chunk_chars
            {
                self.flush();
            }
            if !self.current.is_empty() {
                self.current.push(' ');
            }
            self.current.push_str(&word);
            self.total_chars += added;
        }
        Ok(())
    }

    fn flush(&mut self) {
        if self.current.is_empty() {
            return;
        }
        self.chunks.push(TranscriptChunk {
            ordinal: self.chunks.len(),
            text: std::mem::take(&mut self.current),
        });
    }

    pub fn finish(mut self) -> Vec<TranscriptChunk> {
        self.flush();
        self.chunks
    }
}

pub trait NativeAdapter {
    fn tool(&self) -> Tool;
    fn version(&self) -> &'static str;
    fn scan(
        &self,
        root: &Path,
        states: &HashMap<PathBuf, SourceState>,
    ) -> Result<AdapterScan, AdapterFailure>;
    fn full_transcript(&self, path: &Path) -> Result<Vec<TranscriptChunk>, AdapterFailure>;
}
