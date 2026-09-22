use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveEvidenceSource {
    ProductLaunch,
    ClaudeHook,
    CodexHook,
    CodexAppServer,
    WindowsProcess,
}

impl LiveEvidenceSource {
    pub const fn is_exact(self) -> bool {
        matches!(
            self,
            Self::ClaudeHook | Self::CodexHook | Self::CodexAppServer
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveStatus {
    Active,
    Idle,
    Stopped,
    Unknown,
    SystemError,
    NotLoaded,
}

impl LiveStatus {
    pub const fn indicates_live(self) -> bool {
        matches!(self, Self::Active | Self::Idle)
    }

    pub const fn indicates_stopped(self) -> bool {
        matches!(self, Self::Stopped | Self::NotLoaded)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationLifecycle {
    ConfirmedLive,
    Uncertain,
    NotLive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resumability {
    Validated,
    PreflightPassed,
    Unknown,
    Missing,
    Corrupt,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pid: u32,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    executable: PathBuf,
    parent_pid: Option<u32>,
}

impl ProcessIdentity {
    pub fn new(
        pid: u32,
        created_at: OffsetDateTime,
        executable: impl Into<PathBuf>,
        parent_pid: Option<u32>,
    ) -> Result<Self, ProcessIdentityError> {
        let executable = executable.into();
        if pid == 0 {
            return Err(ProcessIdentityError::ZeroPid);
        }
        if executable.as_os_str().is_empty() {
            return Err(ProcessIdentityError::EmptyExecutable);
        }
        Ok(Self {
            pid,
            created_at,
            executable,
            parent_pid,
        })
    }

    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub const fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub const fn parent_pid(&self) -> Option<u32> {
        self.parent_pid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessIdentityError {
    ZeroPid,
    EmptyExecutable,
}

impl Display for ProcessIdentityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroPid => formatter.write_str("process ID must be greater than zero"),
            Self::EmptyExecutable => formatter.write_str("process executable cannot be empty"),
        }
    }
}

impl std::error::Error for ProcessIdentityError {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use time::OffsetDateTime;

    use super::{
        ConversationLifecycle, LiveEvidenceSource, LiveStatus, ProcessIdentity,
        ProcessIdentityError, Resumability,
    };

    #[test]
    fn exact_sources_and_live_statuses_are_explicit() {
        assert!(LiveEvidenceSource::ClaudeHook.is_exact());
        assert!(LiveEvidenceSource::CodexHook.is_exact());
        assert!(LiveEvidenceSource::CodexAppServer.is_exact());
        assert!(!LiveEvidenceSource::WindowsProcess.is_exact());
        assert!(LiveStatus::Active.indicates_live());
        assert!(LiveStatus::Idle.indicates_live());
        assert!(LiveStatus::NotLoaded.indicates_stopped());
        assert!(!LiveStatus::Unknown.indicates_live());
    }

    #[test]
    fn process_identity_requires_pid_creation_time_and_executable() {
        let created_at = OffsetDateTime::from_unix_timestamp(1_776_945_600)
            .expect("the fixture timestamp should be valid");
        let identity = ProcessIdentity::new(42, created_at, "C:/fake/wt.exe", Some(7))
            .expect("the complete process identity should be valid");

        assert_eq!(identity.pid(), 42);
        assert_eq!(identity.created_at(), created_at);
        assert_eq!(identity.executable(), PathBuf::from("C:/fake/wt.exe"));
        assert_eq!(identity.parent_pid(), Some(7));
        assert_eq!(
            ProcessIdentity::new(0, created_at, "C:/fake/wt.exe", None),
            Err(ProcessIdentityError::ZeroPid)
        );
    }

    #[test]
    fn lifecycle_and_resumability_have_stable_wire_names() {
        assert_eq!(
            serde_json::to_string(&ConversationLifecycle::ConfirmedLive)
                .expect("lifecycle should serialise"),
            "\"confirmed_live\""
        );
        assert_eq!(
            serde_json::to_string(&Resumability::PreflightPassed)
                .expect("resumability should serialise"),
            "\"preflight_passed\""
        );
    }
}
