use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{ManagedSessionRole, Tool};

pub const LAUNCH_PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningEffort {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchProfileSource {
    Suggested,
    Preference,
    ExplicitOverride,
    ResumePreserved,
    LegacyUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfile {
    pub schema_version: u32,
    pub tool: Tool,
    pub model: Option<String>,
    pub effort: Option<ReasoningEffort>,
    pub role: ManagedSessionRole,
    pub source: LaunchProfileSource,
}

impl LaunchProfile {
    pub fn new(
        tool: Tool,
        model: impl Into<String>,
        effort: ReasoningEffort,
        role: ManagedSessionRole,
        source: LaunchProfileSource,
    ) -> Result<Self, LaunchProfileError> {
        let profile = Self {
            schema_version: LAUNCH_PROFILE_SCHEMA_VERSION,
            tool,
            model: Some(model.into()),
            effort: Some(effort),
            role,
            source,
        };
        profile.validate_for_launch(tool, role)?;
        Ok(profile)
    }

    pub const fn legacy_unknown(tool: Tool, role: ManagedSessionRole) -> Self {
        Self {
            schema_version: LAUNCH_PROFILE_SCHEMA_VERSION,
            tool,
            model: None,
            effort: None,
            role,
            source: LaunchProfileSource::LegacyUnknown,
        }
    }

    pub fn suggested(tool: Tool, role: ManagedSessionRole) -> Self {
        let (model, effort) = match tool {
            Tool::Claude => ("sonnet", ReasoningEffort::High),
            Tool::Codex => ("gpt-5.6", ReasoningEffort::High),
        };
        Self::new(tool, model, effort, role, LaunchProfileSource::Suggested)
            .expect("built-in launch profile is valid")
    }

    pub fn validate_for_launch(
        &self,
        tool: Tool,
        role: ManagedSessionRole,
    ) -> Result<(), LaunchProfileError> {
        if self.schema_version != LAUNCH_PROFILE_SCHEMA_VERSION {
            return Err(LaunchProfileError::UnsupportedSchema(self.schema_version));
        }
        if self.tool != tool {
            return Err(LaunchProfileError::ProviderMismatch);
        }
        if self.role != role {
            return Err(LaunchProfileError::RoleMismatch);
        }
        let model = self
            .model
            .as_deref()
            .ok_or(LaunchProfileError::UnknownModel)?;
        if model.is_empty()
            || model.len() > 128
            || model.chars().any(char::is_whitespace)
            || model.chars().any(char::is_control)
            || !model.chars().all(|value| {
                value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.' | ':')
            })
        {
            return Err(LaunchProfileError::UnsafeModel);
        }
        let effort = self.effort.ok_or(LaunchProfileError::UnknownEffort)?;
        if tool == Tool::Codex && effort == ReasoningEffort::Max {
            return Err(LaunchProfileError::UnsupportedEffort { tool, effort });
        }
        if self.source == LaunchProfileSource::LegacyUnknown {
            return Err(LaunchProfileError::LegacyProfileCannotLaunch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchProfileError {
    UnsupportedSchema(u32),
    ProviderMismatch,
    RoleMismatch,
    UnknownModel,
    UnknownEffort,
    UnsafeModel,
    UnsupportedEffort { tool: Tool, effort: ReasoningEffort },
    LegacyProfileCannotLaunch,
}

impl Display for LaunchProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema(version) => {
                write!(formatter, "launch profile schema {version} is unsupported")
            }
            Self::ProviderMismatch => formatter.write_str("launch profile provider does not match"),
            Self::RoleMismatch => formatter.write_str("launch profile role does not match"),
            Self::UnknownModel => formatter.write_str("launch profile model is unknown"),
            Self::UnknownEffort => formatter.write_str("launch profile effort is unknown"),
            Self::UnsafeModel => formatter.write_str("launch profile model is unsafe"),
            Self::UnsupportedEffort { tool, effort } => write!(
                formatter,
                "{} effort is unsupported for {tool:?}",
                effort.as_str()
            ),
            Self::LegacyProfileCannotLaunch => {
                formatter.write_str("legacy unknown profile cannot launch")
            }
        }
    }
}

impl std::error::Error for LaunchProfileError {}

#[cfg(test)]
mod tests {
    use super::{LaunchProfile, LaunchProfileError, LaunchProfileSource, ReasoningEffort};
    use crate::{ManagedSessionRole, Tool};

    #[test]
    fn profiles_validate_provider_role_model_and_capability() {
        let profile = LaunchProfile::new(
            Tool::Claude,
            "sonnet",
            ReasoningEffort::Xhigh,
            ManagedSessionRole::WorkItemExecution,
            LaunchProfileSource::ExplicitOverride,
        )
        .expect("supported profile");
        assert!(
            profile
                .validate_for_launch(Tool::Claude, ManagedSessionRole::WorkItemExecution)
                .is_ok()
        );
        assert_eq!(
            profile.validate_for_launch(Tool::Codex, ManagedSessionRole::WorkItemExecution),
            Err(LaunchProfileError::ProviderMismatch)
        );
        assert!(
            LaunchProfile::new(
                Tool::Codex,
                "gpt-5.6",
                ReasoningEffort::Max,
                ManagedSessionRole::WorkItemExecution,
                LaunchProfileSource::ExplicitOverride,
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_profiles_remain_unknown_and_cannot_be_launched() {
        let profile = LaunchProfile::legacy_unknown(Tool::Codex, ManagedSessionRole::Review);
        assert_eq!(profile.model, None);
        assert_eq!(profile.effort, None);
        assert_eq!(
            profile.validate_for_launch(Tool::Codex, ManagedSessionRole::Review),
            Err(LaunchProfileError::UnknownModel)
        );
    }
}
