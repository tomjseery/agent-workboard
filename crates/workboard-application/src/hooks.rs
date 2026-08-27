use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_core::{ConversationRef, LiveStatus, ProcessIdentity, Tool};

use crate::error::AppError;

pub const MAX_HOOK_INPUT_BYTES: usize = 64 * 1024;
pub const HOOK_OBSERVATION_TTL_SECONDS: i64 = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookIngestionMutation {
    pub tool: Tool,
    pub payload_json: String,
    pub observed_at: String,
    #[serde(default)]
    pub launch_token: Option<String>,
    #[serde(default)]
    pub process: Option<ProcessIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeHookEventKind {
    SessionStart,
    Activity,
    CwdChanged,
    Compact,
    Idle,
    SessionEnd,
}

impl NativeHookEventKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::Activity => "activity",
            Self::CwdChanged => "cwd_changed",
            Self::Compact => "compact",
            Self::Idle => "idle",
            Self::SessionEnd => "session_end",
        }
    }

    pub fn status(self) -> LiveStatus {
        match self {
            Self::Idle => LiveStatus::Idle,
            Self::SessionEnd => LiveStatus::Stopped,
            Self::SessionStart | Self::Activity | Self::CwdChanged | Self::Compact => {
                LiveStatus::Active
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHookObservation {
    pub conversation: ConversationRef,
    pub event: NativeHookEventKind,
    pub native_event_name: String,
    pub lifecycle_source: Option<String>,
    pub cwd: PathBuf,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
struct NativeHookPayload {
    session_id: String,
    cwd: PathBuf,
    hook_event_name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
}

pub fn parse_hook(mutation: &HookIngestionMutation) -> Result<NativeHookObservation, AppError> {
    if mutation.payload_json.len() > MAX_HOOK_INPUT_BYTES {
        return Err(AppError::HookInputTooLarge {
            limit: MAX_HOOK_INPUT_BYTES,
        });
    }
    let payload: NativeHookPayload = serde_json::from_str(&mutation.payload_json)
        .map_err(|error| AppError::InvalidHookInput(error.to_string()))?;
    if payload.session_id.len() > 512 {
        return Err(AppError::InvalidHookInput(
            "hook session ID exceeds 512 bytes".to_owned(),
        ));
    }
    if payload.hook_event_name.len() > 64 {
        return Err(AppError::InvalidHookInput(
            "hook event name exceeds 64 bytes".to_owned(),
        ));
    }
    if payload.agent_id.is_some() {
        return Err(AppError::HelperHookIdentity);
    }
    if !payload.cwd.is_absolute() {
        return Err(AppError::InvalidHookInput(
            "hook cwd must be an absolute path".to_owned(),
        ));
    }
    let cwd = payload
        .cwd
        .to_str()
        .ok_or_else(|| AppError::InvalidHookInput("hook cwd must be valid Unicode".to_owned()))?;
    if cwd.chars().any(char::is_control) {
        return Err(AppError::InvalidHookInput(
            "hook cwd contains control characters".to_owned(),
        ));
    }
    let conversation = ConversationRef::new(mutation.tool, payload.session_id)
        .map_err(|error| AppError::InvalidHookInput(error.to_string()))?;
    let event = match mutation.tool {
        Tool::Claude => claude_event(&payload.hook_event_name),
        Tool::Codex => codex_event(&payload.hook_event_name),
    }?;
    let lifecycle_source = if event == NativeHookEventKind::SessionStart {
        let source = payload.source.ok_or_else(|| {
            AppError::InvalidHookInput("SessionStart hook input requires source".to_owned())
        })?;
        if !matches!(source.as_str(), "startup" | "resume" | "clear" | "compact") {
            return Err(AppError::InvalidHookInput(format!(
                "unsupported SessionStart source: {source}"
            )));
        }
        Some(source)
    } else {
        payload.source
    };
    let observed_at = OffsetDateTime::parse(
        &mutation.observed_at,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|error| AppError::InvalidHookInput(error.to_string()))?;

    Ok(NativeHookObservation {
        conversation,
        event,
        native_event_name: payload.hook_event_name,
        lifecycle_source,
        cwd: payload.cwd,
        observed_at,
    })
}

fn claude_event(value: &str) -> Result<NativeHookEventKind, AppError> {
    match value {
        "SessionStart" => Ok(NativeHookEventKind::SessionStart),
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
            Ok(NativeHookEventKind::Activity)
        }
        "CwdChanged" => Ok(NativeHookEventKind::CwdChanged),
        "PreCompact" | "PostCompact" => Ok(NativeHookEventKind::Compact),
        "Stop" => Ok(NativeHookEventKind::Idle),
        "SessionEnd" => Ok(NativeHookEventKind::SessionEnd),
        "SubagentStart" | "SubagentStop" => Err(AppError::HelperHookIdentity),
        _ => Err(AppError::UnsupportedHookEvent {
            tool: "Claude",
            event: value.to_owned(),
        }),
    }
}

fn codex_event(value: &str) -> Result<NativeHookEventKind, AppError> {
    match value {
        "SessionStart" => Ok(NativeHookEventKind::SessionStart),
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => Ok(NativeHookEventKind::Activity),
        "PreCompact" | "PostCompact" => Ok(NativeHookEventKind::Compact),
        "Stop" => Ok(NativeHookEventKind::Idle),
        "SessionEnd" => Ok(NativeHookEventKind::SessionEnd),
        "SubagentStart" | "SubagentStop" => Err(AppError::HelperHookIdentity),
        _ => Err(AppError::UnsupportedHookEvent {
            tool: "Codex",
            event: value.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use workboard_core::{LiveStatus, Tool};

    use super::{HookIngestionMutation, MAX_HOOK_INPUT_BYTES, NativeHookEventKind, parse_hook};
    use crate::error::AppError;

    #[test]
    fn parses_claude_session_start_without_retaining_unknown_fields() {
        let payload = json!({
            "session_id": "session-one",
            "cwd": fixture_cwd(),
            "hook_event_name": "SessionStart",
            "source": "resume",
            "model": "fixture-model",
            "prompt": "must not be retained"
        })
        .to_string();
        let observation =
            parse_hook(&mutation(Tool::Claude, &payload)).expect("the hook should parse");

        assert_eq!(observation.event, NativeHookEventKind::SessionStart);
        assert_eq!(observation.event.status(), LiveStatus::Active);
        assert_eq!(observation.lifecycle_source.as_deref(), Some("resume"));
        assert_eq!(observation.conversation.native_id(), "session-one");
    }

    #[test]
    fn maps_stop_to_idle_and_session_end_to_stopped() {
        let idle = parse_hook(&mutation(Tool::Codex, &payload("thread-one", "Stop")))
            .expect("the stop hook should parse");
        let stopped = parse_hook(&mutation(Tool::Codex, &payload("thread-one", "SessionEnd")))
            .expect("the session end hook should parse");

        assert_eq!(idle.event.status(), LiveStatus::Idle);
        assert_eq!(stopped.event.status(), LiveStatus::Stopped);
    }

    #[test]
    fn accepts_supported_claude_lifecycle_and_activity_events() {
        for source in ["startup", "resume", "clear", "compact"] {
            let mut event = event("session-one", "SessionStart");
            event["source"] = source.into();
            event["agent_type"] = "reviewer".into();
            let observation = parse_hook(&mutation(Tool::Claude, &event.to_string()))
                .expect("the SessionStart source should parse");
            assert_eq!(observation.event, NativeHookEventKind::SessionStart);
            assert_eq!(observation.lifecycle_source.as_deref(), Some(source));
        }

        for (native_event, event) in [
            ("UserPromptSubmit", NativeHookEventKind::Activity),
            ("PreToolUse", NativeHookEventKind::Activity),
            ("PostToolUse", NativeHookEventKind::Activity),
            ("PostToolUseFailure", NativeHookEventKind::Activity),
            ("CwdChanged", NativeHookEventKind::CwdChanged),
            ("PreCompact", NativeHookEventKind::Compact),
            ("PostCompact", NativeHookEventKind::Compact),
            ("SessionEnd", NativeHookEventKind::SessionEnd),
        ] {
            let observation = parse_hook(&mutation(
                Tool::Claude,
                &payload("session-one", native_event),
            ))
            .expect("the supported Claude event should parse");
            assert_eq!(observation.event, event);
        }
    }

    #[test]
    fn rejects_helper_and_tool_specific_unsupported_events() {
        let helper = parse_hook(&mutation(
            Tool::Claude,
            &payload("session-one", "SubagentStart"),
        ));
        let cwd = parse_hook(&mutation(Tool::Codex, &payload("thread-one", "CwdChanged")));

        assert!(matches!(helper, Err(AppError::HelperHookIdentity)));
        assert!(matches!(cwd, Err(AppError::UnsupportedHookEvent { .. })));
    }

    #[test]
    fn rejects_malformed_and_oversized_hook_input() {
        let malformed = parse_hook(&mutation(Tool::Claude, "{"));
        let oversized = parse_hook(&mutation(
            Tool::Claude,
            &"x".repeat(MAX_HOOK_INPUT_BYTES + 1),
        ));

        assert!(matches!(malformed, Err(AppError::InvalidHookInput(_))));
        assert!(matches!(oversized, Err(AppError::HookInputTooLarge { .. })));
    }

    fn mutation(tool: Tool, payload_json: &str) -> HookIngestionMutation {
        HookIngestionMutation {
            tool,
            payload_json: payload_json.to_owned(),
            observed_at: "2026-08-21T12:00:00Z".to_owned(),
            launch_token: None,
            process: None,
        }
    }

    fn payload(native_id: &str, hook_event_name: &str) -> String {
        event(native_id, hook_event_name).to_string()
    }

    fn event(native_id: &str, hook_event_name: &str) -> serde_json::Value {
        json!({
            "session_id": native_id,
            "cwd": fixture_cwd(),
            "hook_event_name": hook_event_name
        })
    }

    fn fixture_cwd() -> String {
        std::env::current_dir()
            .expect("current directory")
            .to_string_lossy()
            .into_owned()
    }
}
