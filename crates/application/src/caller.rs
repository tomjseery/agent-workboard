use workboard_core::{ConversationRef, Tool};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerIdentity {
    pub conversation: ConversationRef,
    pub source_locator: String,
}

pub trait CallerIdentityProvider {
    fn identify(&self, requested_tool: Option<Tool>) -> Result<CallerIdentity, AppError>;
}

pub struct EnvironmentCallerIdentity;

impl CallerIdentityProvider for EnvironmentCallerIdentity {
    fn identify(&self, requested_tool: Option<Tool>) -> Result<CallerIdentity, AppError> {
        let claude = environment_value("CLAUDE_CODE_SESSION_ID");
        let codex = environment_value("CODEX_THREAD_ID");

        match requested_tool {
            Some(Tool::Claude) => {
                identity(Tool::Claude, claude, "CLAUDE_CODE_SESSION_ID", "Claude")
            }
            Some(Tool::Codex) => identity(Tool::Codex, codex, "CODEX_THREAD_ID", "Codex"),
            None => match (claude, codex) {
                (Some(_), Some(_)) => Err(AppError::CallerIdentityAmbiguous),
                (Some(native_id), None) => identity(
                    Tool::Claude,
                    Some(native_id),
                    "CLAUDE_CODE_SESSION_ID",
                    "Claude",
                ),
                (None, Some(native_id)) => {
                    identity(Tool::Codex, Some(native_id), "CODEX_THREAD_ID", "Codex")
                }
                (None, None) => Err(AppError::CallerIdentityMissing),
            },
        }
    }
}

fn environment_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn identity(
    tool: Tool,
    native_id: Option<String>,
    source_locator: &str,
    tool_name: &'static str,
) -> Result<CallerIdentity, AppError> {
    let native_id =
        native_id.ok_or(AppError::RequestedCallerIdentityMissing { tool: tool_name })?;
    let conversation = ConversationRef::new(tool, native_id)
        .map_err(|error| AppError::Domain(error.to_string()))?;

    Ok(CallerIdentity {
        conversation,
        source_locator: source_locator.to_owned(),
    })
}
