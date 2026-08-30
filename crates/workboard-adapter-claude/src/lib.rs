use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;
use time::OffsetDateTime;
use workboard_core::{LaunchProfile, LaunchProfileError, Tool};
use workboard_native::{
    AdapterFailure, AdapterFailureKind, AdapterScan, ConversationKind, NativeAdapter,
    NativeConversation, ScanLimits, SourceScan, SourceState, TranscriptBuilder, TranscriptChunk,
    discover_jsonl_files, stream_jsonl,
};

pub const ADAPTER_VERSION: &str = "claude-jsonl-v1";

pub fn launch_profile_arguments(
    profile: &LaunchProfile,
) -> Result<Vec<OsString>, LaunchProfileError> {
    profile.validate_for_launch(Tool::Claude, profile.role)?;
    Ok(vec![
        OsString::from("--model"),
        OsString::from(
            profile
                .model
                .as_deref()
                .ok_or(LaunchProfileError::UnknownModel)?,
        ),
        OsString::from("--effort"),
        OsString::from(
            profile
                .effort
                .ok_or(LaunchProfileError::UnknownEffort)?
                .as_str(),
        ),
    ])
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeAdapterV1 {
    limits: ScanLimits,
}

impl ClaudeAdapterV1 {
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
                message: "Claude resume source did not contain the expected top-level session"
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
            message: "no recognised Claude conversation identity was found".to_owned(),
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

impl NativeAdapter for ClaudeAdapterV1 {
    fn tool(&self) -> Tool {
        Tool::Claude
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
            tool: Tool::Claude,
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
        let value = &record.value;
        for field in ["message", "toolUseResult", "data", "content"] {
            if let Some(content) = value.get(field) {
                collect_text(content, &mut builder, path, record.offset)?;
            }
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
    let is_helper = record
        .get("isSidechain")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || record.get("agentId").is_some();
    let parent = text(record, &["sessionId", "parentSessionId"]);
    let native_id = if is_helper {
        text(record, &["agentId"]).or_else(|| parent.clone())
    } else {
        parent.clone()
    };
    let Some(native_id) = native_id else {
        if let Some(existing) = conversation {
            existing.unknown_record_count += 1;
        }
        return Ok(());
    };
    let kind = if is_helper {
        ConversationKind::Helper
    } else {
        ConversationKind::TopLevel
    };
    if let Some(existing) = conversation.as_ref() {
        if existing.native_id != native_id {
            return Err(AdapterFailure {
                path: path.to_path_buf(),
                byte_offset: offset,
                kind: AdapterFailureKind::ConflictingIdentity,
                message: "one Claude source contained multiple conversation identities".to_owned(),
            });
        }
    } else {
        let mut discovered = NativeConversation::new(native_id, kind);
        if is_helper {
            discovered.parent_native_id = parent;
        }
        *conversation = Some(discovered);
    }

    let discovered = conversation.as_mut().expect("conversation was initialised");
    let timestamp = timestamp(record.get("timestamp"));
    discovered.observe_activity(timestamp);
    if discovered.native_created_at.is_none() {
        discovered.native_created_at = timestamp;
    }
    if let Some(cwd) = text(record, &["cwd"]) {
        discovered.observe_cwd(cwd, timestamp);
    }
    if let Some(version) = text(record, &["version", "claudeVersion"]) {
        discovered.tool_version = Some(clean(&version, 128));
    }
    if discovered.forked_from_native_id.is_none() {
        discovered.forked_from_native_id =
            text(record, &["forkedFromSessionId", "forked_from_session_id"]);
    }

    match record.get("type").and_then(Value::as_str) {
        Some("user") => {
            if let Some(prompt) = message_text(record).map(|value| clean(&value, preview_chars))
                && !prompt.is_empty()
            {
                discovered.observe_prompt(prompt);
            }
        }
        Some("custom-title" | "title") => {
            if let Some(title) = text(record, &["customTitle", "title"]) {
                discovered.title = Some(clean(&title, 512));
            }
        }
        Some("summary" | "compact") => discovered.compacted = true,
        Some(
            "assistant"
            | "system"
            | "progress"
            | "file-history-snapshot"
            | "queue-operation"
            | "pr-link"
            | "attachment",
        ) => {}
        _ => discovered.unknown_record_count += 1,
    }
    Ok(())
}

fn message_text(record: &Value) -> Option<String> {
    let content = record.get("message")?.get("content")?;
    match content {
        Value::String(value) => Some(value.clone()),
        Value::Array(items) => {
            let parts: Vec<&str> = items
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect();
            (!parts.is_empty()).then(|| parts.join(" "))
        }
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

    use super::ClaudeAdapterV1;

    #[test]
    fn discovers_resume_clear_compact_fork_helper_and_unknown_shapes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sessions");
        let scan = ClaudeAdapterV1::default()
            .scan(&root, &HashMap::new())
            .expect("the synthetic fixture root should scan");

        assert!(scan.failures.is_empty());
        assert_eq!(scan.sources.len(), 6);
        let resumed = find(&scan, "claude-resume");
        assert_eq!(
            resumed.first_prompt_preview.as_deref(),
            Some("Start the synthetic task")
        );
        assert_eq!(
            resumed.last_prompt_preview.as_deref(),
            Some("Resume the synthetic task")
        );
        assert_eq!(resumed.title.as_deref(), Some("Synthetic resume"));
        assert_eq!(resumed.unknown_record_count, 1);
        assert!(find(&scan, "claude-compact").compacted);
        assert_eq!(
            find(&scan, "claude-fork").forked_from_native_id.as_deref(),
            Some("claude-resume")
        );
        let helper = find(&scan, "helper-alpha");
        assert_eq!(helper.kind, ConversationKind::Helper);
        assert_eq!(helper.parent_native_id.as_deref(), Some("claude-resume"));
    }

    #[test]
    fn leaves_an_incomplete_record_for_the_next_scan() {
        let directory = TempDir::new().expect("the fixture directory should exist");
        let source =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/truncated.jsonl");
        let mut bytes = fs::read(source).expect("the fixture should be readable");
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        fs::write(directory.path().join("truncated.jsonl"), bytes)
            .expect("the fixture should copy");
        let scan = ClaudeAdapterV1::default()
            .scan(directory.path(), &HashMap::new())
            .expect("the fixture root should scan");

        assert!(scan.failures.is_empty());
        assert!(scan.sources[0].incomplete_tail);
        assert_eq!(
            scan.sources[0].conversation.last_prompt_preview.as_deref(),
            Some("Complete record")
        );
    }

    #[test]
    fn extracts_bounded_full_transcript_text_read_only() {
        let directory = TempDir::new().expect("fixture directory");
        let path = directory.path().join("full.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"user\",\"sessionId\":\"full-claude\",\"message\":{\"content\":\"Visible prompt\"}}\n",
                "{\"type\":\"assistant\",\"sessionId\":\"full-claude\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Assistant-only nebula phrase\"},{\"type\":\"tool_result\",\"content\":\"Synthetic tool payload\"}]}}\n"
            ),
        )
        .expect("fixture transcript");

        let chunks = ClaudeAdapterV1::default()
            .full_transcript(&path)
            .expect("full transcript extraction");
        let text = chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(text.contains("Assistant-only nebula phrase"));
        assert!(text.contains("Synthetic tool payload"));
    }

    #[test]
    fn maps_a_validated_profile_without_shell_interpretation() {
        let profile = workboard_core::LaunchProfile::new(
            workboard_core::Tool::Claude,
            "sonnet",
            workboard_core::ReasoningEffort::Xhigh,
            workboard_core::ManagedSessionRole::Review,
            workboard_core::LaunchProfileSource::ExplicitOverride,
        )
        .expect("valid profile");

        assert_eq!(
            super::launch_profile_arguments(&profile).expect("profile arguments"),
            ["--model", "sonnet", "--effort", "xhigh"].map(std::ffi::OsString::from)
        );
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
