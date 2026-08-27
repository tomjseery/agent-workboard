mod app_server;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use time::OffsetDateTime;
use workboard_core::Tool;
use workboard_native::{
    AdapterFailure, AdapterFailureKind, AdapterScan, ConversationKind, NativeAdapter,
    NativeConversation, ScanLimits, SourceScan, SourceState, TranscriptBuilder, TranscriptChunk,
    discover_jsonl_files, stream_jsonl,
};

pub use app_server::{
    APP_SERVER_ADAPTER_VERSION, CodexAppServerClient, CodexAppServerFailure,
    CodexAppServerFailureKind, CodexAppServerLimits, CodexAppServerSnapshot,
    CodexAppServerStatusObservation, CodexAppServerThread,
};

pub const ADAPTER_VERSION: &str = "codex-rollout-v1";

#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAdapterV1 {
    limits: ScanLimits,
}

impl CodexAdapterV1 {
    pub fn new(limits: ScanLimits) -> Self {
        Self { limits }
    }

    pub fn preflight_resume(
        &self,
        path: &Path,
        expected_native_id: &str,
    ) -> Result<(), AdapterFailure> {
        let source = self.scan_source(path, None)?;
        if source.conversation.kind != ConversationKind::TopLevel
            || source.conversation.native_id != expected_native_id
        {
            return Err(AdapterFailure {
                path: path.to_owned(),
                byte_offset: 0,
                kind: AdapterFailureKind::ConflictingIdentity,
                message: "Codex resume source did not contain the expected top-level thread"
                    .to_owned(),
            });
        }
        Ok(())
    }

    fn scan_source(
        &self,
        path: &Path,
        prior: Option<&SourceState>,
    ) -> Result<SourceScan, AdapterFailure> {
        let stream = stream_jsonl(path, prior.map(|state| &state.cursor), self.limits)?;
        let mut conversation = if stream.start_offset == 0 {
            None
        } else {
            prior.map(|state| state.conversation.clone())
        };
        for record in &stream.records {
            apply_record(
                &mut conversation,
                &record.value,
                record.offset,
                path,
                self.limits.max_preview_chars,
            )?;
        }
        let conversation = conversation.ok_or_else(|| AdapterFailure {
            path: path.to_path_buf(),
            byte_offset: 0,
            kind: AdapterFailureKind::MissingIdentity,
            message: "no recognised Codex thread identity was found".to_owned(),
        })?;
        Ok(SourceScan {
            path: path.to_path_buf(),
            source_size: stream.source_size,
            modified_at_ns: stream.modified_at_ns,
            cursor: stream.cursor,
            conversation,
            incomplete_tail: stream.incomplete_tail,
            records_read: stream.records.len(),
        })
    }
}

impl NativeAdapter for CodexAdapterV1 {
    fn tool(&self) -> Tool {
        Tool::Codex
    }

    fn version(&self) -> &'static str {
        ADAPTER_VERSION
    }

    fn scan(
        &self,
        root: &Path,
        states: &HashMap<PathBuf, SourceState>,
    ) -> Result<AdapterScan, AdapterFailure> {
        let inventory = discover_jsonl_files(root, self.limits.max_sources)?;
        let mut sources = Vec::new();
        let mut failures = Vec::new();
        for path in &inventory {
            match self.scan_source(path, states.get(path)) {
                Ok(source) => sources.push(source),
                Err(error) => failures.push(error),
            }
        }
        Ok(AdapterScan {
            tool: Tool::Codex,
            adapter_version: ADAPTER_VERSION,
            inventory,
            sources,
            failures,
        })
    }

    fn full_transcript(&self, path: &Path) -> Result<Vec<TranscriptChunk>, AdapterFailure> {
        let stream = stream_jsonl(path, None, self.limits)?;
        transcript_chunks(
            path,
            &stream.records,
            self.limits.max_search_chunk_chars,
            self.limits.max_transcript_chars,
        )
    }
}

fn transcript_chunks(
    path: &Path,
    records: &[workboard_native::JsonlRecord],
    max_chunk_chars: usize,
    max_transcript_chars: usize,
) -> Result<Vec<TranscriptChunk>, AdapterFailure> {
    let mut builder = TranscriptBuilder::new(max_chunk_chars, max_transcript_chars);
    for record in records {
        let payload = record.value.get("payload").unwrap_or(&record.value);
        if matches!(
            record.value.get("type").and_then(Value::as_str),
            Some("event_msg" | "response_item")
        ) {
            collect_text(payload, &mut builder, path, record.offset)?;
        }
    }
    Ok(builder.finish())
}

fn collect_text(
    value: &Value,
    builder: &mut TranscriptBuilder,
    path: &Path,
    offset: u64,
) -> Result<(), AdapterFailure> {
    match value {
        Value::String(text) => builder.push(text, path, offset),
        Value::Array(items) => {
            for item in items {
                collect_text(item, builder, path, offset)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_text(value, builder, path, offset)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn apply_record(
    conversation: &mut Option<NativeConversation>,
    record: &Value,
    offset: u64,
    path: &Path,
    preview_chars: usize,
) -> Result<(), AdapterFailure> {
    let record_type = record.get("type").and_then(Value::as_str);
    let payload = record.get("payload").unwrap_or(record);
    let native_id = if record_type == Some("session_meta") {
        text(payload, &["id", "thread_id", "threadId"])
    } else {
        text(record, &["thread_id", "threadId"])
    };
    if let Some(native_id) = native_id {
        if let Some(existing) = conversation.as_ref() {
            if existing.native_id != native_id {
                return Err(AdapterFailure {
                    path: path.to_path_buf(),
                    byte_offset: offset,
                    kind: AdapterFailureKind::ConflictingIdentity,
                    message: "one Codex rollout contained multiple thread identities".to_owned(),
                });
            }
        } else {
            let (kind, parent) = source_kind(payload.get("source"));
            let mut discovered = NativeConversation::new(native_id, kind);
            discovered.parent_native_id = parent;
            discovered.forked_from_native_id = text(
                payload,
                &["forked_from_id", "forkedFromId", "forked_from_thread_id"],
            );
            *conversation = Some(discovered);
        }
    }
    let Some(discovered) = conversation.as_mut() else {
        return Ok(());
    };
    let observed_at = timestamp(record.get("timestamp").or_else(|| payload.get("timestamp")));
    discovered.observe_activity(observed_at);
    if discovered.native_created_at.is_none() {
        discovered.native_created_at = observed_at;
    }
    if let Some(cwd) = text(payload, &["cwd"]) {
        discovered.observe_cwd(cwd, observed_at);
    }
    if let Some(version) = text(payload, &["cli_version", "cliVersion", "version"]) {
        discovered.tool_version = Some(clean(&version, 128));
    }
    if let Some(title) = text(payload, &["title"]) {
        discovered.title = Some(clean(&title, 512));
    }

    match record_type {
        Some("event_msg") => {
            if matches!(
                payload.get("type").and_then(Value::as_str),
                Some("user_message" | "user")
            ) && let Some(prompt) =
                text(payload, &["message", "text"]).map(|value| clean(&value, preview_chars))
                && !prompt.is_empty()
            {
                discovered.observe_prompt(prompt);
            }
        }
        Some("compacted") => discovered.compacted = true,
        Some("session_meta" | "response_item" | "world_state" | "turn_context") => {}
        _ => discovered.unknown_record_count += 1,
    }
    Ok(())
}

fn source_kind(source: Option<&Value>) -> (ConversationKind, Option<String>) {
    match source {
        Some(Value::String(value)) if value == "exec" => (ConversationKind::Helper, None),
        Some(Value::Object(object)) => {
            let parent = find_nested_text(
                &Value::Object(object.clone()),
                &["parent_thread_id", "parentThreadId"],
            );
            (ConversationKind::Helper, parent)
        }
        _ => (ConversationKind::TopLevel, None),
    }
}

fn find_nested_text(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => object.iter().find_map(|(key, value)| {
            if keys.contains(&key.as_str()) {
                value.as_str().map(str::to_owned)
            } else {
                find_nested_text(value, keys)
            }
        }),
        Value::Array(items) => items.iter().find_map(|item| find_nested_text(item, keys)),
        _ => None,
    }
}

fn text(record: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| record.get(*key).and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn timestamp(value: Option<&Value>) -> Option<OffsetDateTime> {
    value.and_then(Value::as_str).and_then(|value| {
        OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
    })
}

fn clean(value: &str, max_chars: usize) -> String {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use tempfile::TempDir;
    use workboard_native::{ConversationKind, NativeAdapter};

    use super::CodexAdapterV1;

    #[test]
    fn discovers_resume_clear_compact_fork_helper_and_unknown_shapes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sessions");
        let scan = CodexAdapterV1::default()
            .scan(&root, &HashMap::new())
            .expect("the fixtures should scan");

        assert!(scan.failures.is_empty());
        assert_eq!(scan.sources.len(), 5);
        let resumed = find(&scan, "codex-resume");
        assert_eq!(
            resumed.first_prompt_preview.as_deref(),
            Some("Start the Codex fixture")
        );
        assert_eq!(
            resumed.last_prompt_preview.as_deref(),
            Some("Resume the Codex fixture")
        );
        assert!(resumed.compacted);
        assert_eq!(resumed.unknown_record_count, 1);
        assert_eq!(
            find(&scan, "codex-fork").forked_from_native_id.as_deref(),
            Some("codex-resume")
        );
        let helper = find(&scan, "codex-helper");
        assert_eq!(helper.kind, ConversationKind::Helper);
        assert_eq!(helper.parent_native_id.as_deref(), Some("codex-resume"));
    }

    #[test]
    fn ignores_an_incomplete_final_record() {
        let directory = TempDir::new().expect("the fixture directory should exist");
        let source =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/truncated.jsonl");
        let mut bytes = fs::read(source).expect("the fixture should be readable");
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        fs::write(directory.path().join("truncated.jsonl"), bytes)
            .expect("the fixture should copy");
        let scan = CodexAdapterV1::default()
            .scan(directory.path(), &HashMap::new())
            .expect("the fixture should scan");

        assert!(scan.failures.is_empty());
        assert!(scan.sources[0].incomplete_tail);
        assert_eq!(
            scan.sources[0].conversation.last_prompt_preview.as_deref(),
            Some("Complete Codex record")
        );
    }

    #[test]
    fn extracts_bounded_full_transcript_text_read_only() {
        let directory = TempDir::new().expect("fixture directory");
        let path = directory.path().join("full.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"full-codex\",\"cwd\":\"C:/synthetic/repository\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"Visible prompt\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"output\":\"Assistant-only aurora phrase\"}}\n"
            ),
        )
        .expect("fixture transcript");

        let chunks = CodexAdapterV1::default()
            .full_transcript(&path)
            .expect("full transcript extraction");
        let text = chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(text.contains("Assistant-only aurora phrase"));
        assert!(!text.contains("C:/synthetic/repository"));
    }

    fn find<'a>(
        scan: &'a workboard_native::AdapterScan,
        id: &str,
    ) -> &'a workboard_native::NativeConversation {
        &scan
            .sources
            .iter()
            .find(|source| source.conversation.native_id == id)
            .expect("the conversation should exist")
            .conversation
    }
}
