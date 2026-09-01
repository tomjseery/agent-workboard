use std::ffi::{OsStr, OsString};
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{ConversationRef, Tool};

pub const WORKBOARD_LAUNCH_TOKEN_ENV: &str = "WORKBOARD_LAUNCH_TOKEN";
pub const WORKBOARD_WORKFLOW_TOKEN_ENV: &str = "WORKBOARD_WORKFLOW_TOKEN";
pub const WORKBOARD_SESSION_ROLE_ENV: &str = "WORKBOARD_SESSION_ROLE";
pub const WORKBOARD_OWNER_ENV: &str = "WORKBOARD_OWNER";
pub const WORKBOARD_REPOSITORY_ENV: &str = "WORKBOARD_REPOSITORY";
pub const WORKBOARD_CHECKOUT_ENV: &str = "WORKBOARD_CHECKOUT";
pub const WORKBOARD_BUNDLE_ENV: &str = "WORKBOARD_CAPABILITY_BUNDLE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    executable: PathBuf,
    arguments: Vec<OsString>,
}

impl CommandSpec {
    pub fn new(executable: impl Into<PathBuf>, arguments: Vec<OsString>) -> Self {
        Self {
            executable: executable.into(),
            arguments,
        }
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeLaunchSpec {
    terminal: CommandSpec,
    native: CommandSpec,
    working_directory: PathBuf,
    title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalKind {
    WindowsTerminal,
    XdgTerminalExec,
    XTerminalEmulator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedLaunchMode {
    New,
    Resume(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedLaunchRequest {
    pub terminal_kind: TerminalKind,
    pub terminal_executable: PathBuf,
    pub native_executable: PathBuf,
    pub tool: Tool,
    pub mode: ManagedLaunchMode,
    pub working_directory: PathBuf,
    pub title: String,
    pub terminal_window: Option<String>,
    pub launch_token: String,
    pub workflow_token: Option<String>,
    pub capability_environment: Vec<(String, String)>,
    pub initial_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedLaunchSpec {
    terminal: CommandSpec,
    native: CommandSpec,
    working_directory: PathBuf,
    title: String,
    launch_token: String,
    workflow_token: Option<String>,
    capability_environment: Vec<(String, String)>,
}

impl ManagedLaunchSpec {
    pub fn new(request: ManagedLaunchRequest) -> Result<Self, LaunchSpecError> {
        let ManagedLaunchRequest {
            terminal_kind,
            terminal_executable,
            native_executable,
            tool,
            mode,
            working_directory,
            title,
            terminal_window,
            launch_token,
            workflow_token,
            capability_environment,
            initial_prompt,
        } = request;
        validate_path(&working_directory)?;
        if launch_token.is_empty() || launch_token.chars().any(char::is_control) {
            return Err(LaunchSpecError::UnsafeLaunchToken);
        }
        if workflow_token
            .as_deref()
            .is_some_and(|token| token.is_empty() || token.chars().any(char::is_control))
        {
            return Err(LaunchSpecError::UnsafeLaunchToken);
        }
        for (name, value) in &capability_environment {
            if name.is_empty()
                || !name
                    .chars()
                    .all(|character| character.is_ascii_uppercase() || character == '_')
            {
                return Err(LaunchSpecError::UnsafeCapabilityEnvironment);
            }
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err(LaunchSpecError::UnsafeCapabilityEnvironment);
            }
        }
        if terminal_window.as_deref().is_some_and(|window| {
            window.is_empty() || window.len() > 120 || window.chars().any(char::is_control)
        }) {
            return Err(LaunchSpecError::UnsafeTerminalWindow);
        }
        if native_executable.as_os_str().is_empty() {
            return Err(LaunchSpecError::EmptyExecutable);
        }
        if initial_prompt.as_deref().is_some_and(|prompt| {
            prompt.is_empty() || prompt.len() > 16 * 1024 || prompt.contains('\0')
        }) {
            return Err(LaunchSpecError::UnsafeInitialPrompt);
        }
        let native_arguments = match (tool, mode, initial_prompt) {
            (Tool::Claude, ManagedLaunchMode::New, prompt) => {
                prompt.map(OsString::from).into_iter().collect()
            }
            (Tool::Claude, ManagedLaunchMode::Resume(native_id), None) => {
                validate_native_id(&native_id)?;
                vec![OsString::from("--resume"), OsString::from(native_id)]
            }
            (Tool::Codex, ManagedLaunchMode::New, prompt) => {
                let mut arguments = vec![
                    OsString::from("--cd"),
                    working_directory.as_os_str().to_owned(),
                    OsString::from("--dangerously-bypass-hook-trust"),
                ];
                arguments.extend(prompt.map(OsString::from));
                arguments
            }
            (Tool::Codex, ManagedLaunchMode::Resume(native_id), None) => {
                validate_native_id(&native_id)?;
                vec![
                    OsString::from("--cd"),
                    working_directory.as_os_str().to_owned(),
                    OsString::from("--dangerously-bypass-hook-trust"),
                    OsString::from("resume"),
                    OsString::from(native_id),
                ]
            }
            (_, ManagedLaunchMode::Resume(_), Some(_)) => {
                return Err(LaunchSpecError::UnsafeInitialPrompt);
            }
        };
        let native = CommandSpec::new(native_executable, native_arguments);
        if terminal_executable.as_os_str().is_empty() {
            return Err(LaunchSpecError::EmptyExecutable);
        }
        let title = sanitise_terminal_title(&title);
        let mut arguments = terminal_arguments(
            terminal_kind,
            &working_directory,
            &title,
            native.executable(),
            terminal_window.as_deref(),
        );
        arguments.extend(native.arguments().iter().cloned());
        Ok(Self {
            terminal: CommandSpec::new(terminal_executable, arguments),
            native,
            working_directory,
            title,
            launch_token,
            workflow_token,
            capability_environment,
        })
    }

    pub fn terminal(&self) -> &CommandSpec {
        &self.terminal
    }

    pub fn native(&self) -> &CommandSpec {
        &self.native
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn launch_token(&self) -> &str {
        &self.launch_token
    }

    pub fn workflow_token(&self) -> Option<&str> {
        self.workflow_token.as_deref()
    }

    pub fn capability_environment(&self) -> &[(String, String)] {
        &self.capability_environment
    }

    pub fn direct_child_command(&self) -> Command {
        let mut command = Command::new(self.terminal.executable());
        command
            .args(self.terminal.arguments())
            .env_remove("NO_COLOR")
            .env_remove("TERM")
            .current_dir(&self.working_directory)
            .env(WORKBOARD_LAUNCH_TOKEN_ENV, &self.launch_token);
        if let Some(token) = &self.workflow_token {
            command.env(WORKBOARD_WORKFLOW_TOKEN_ENV, token);
        }
        for (name, value) in &self.capability_environment {
            command.env(name, value);
        }
        command
    }
}

impl ResumeLaunchSpec {
    pub fn new(
        terminal_kind: TerminalKind,
        terminal_executable: impl Into<PathBuf>,
        native_executable: impl Into<PathBuf>,
        conversation: &ConversationRef,
        working_directory: impl Into<PathBuf>,
        title: &str,
    ) -> Result<Self, LaunchSpecError> {
        let working_directory = working_directory.into();
        validate_path(&working_directory)?;
        validate_native_id(conversation.native_id())?;

        let title = sanitise_terminal_title(title);
        let native_executable = native_executable.into();
        if native_executable.as_os_str().is_empty() {
            return Err(LaunchSpecError::EmptyExecutable);
        }

        let native_arguments = match conversation.tool() {
            Tool::Claude => vec![
                OsString::from("--resume"),
                OsString::from(conversation.native_id()),
            ],
            Tool::Codex => vec![
                OsString::from("--cd"),
                working_directory.as_os_str().to_owned(),
                OsString::from("--dangerously-bypass-hook-trust"),
                OsString::from("resume"),
                OsString::from(conversation.native_id()),
            ],
        };
        let native = CommandSpec::new(native_executable, native_arguments);

        let terminal_executable = terminal_executable.into();
        if terminal_executable.as_os_str().is_empty() {
            return Err(LaunchSpecError::EmptyExecutable);
        }
        let mut terminal_arguments = terminal_arguments(
            terminal_kind,
            &working_directory,
            &title,
            native.executable(),
            None,
        );
        terminal_arguments.extend(native.arguments().iter().cloned());
        let terminal = CommandSpec::new(terminal_executable, terminal_arguments);

        Ok(Self {
            terminal,
            native,
            working_directory,
            title,
        })
    }

    pub fn terminal(&self) -> &CommandSpec {
        &self.terminal
    }

    pub fn native(&self) -> &CommandSpec {
        &self.native
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

fn prefixed_argument(prefix: &str, value: &OsStr) -> OsString {
    let mut argument = OsString::from(prefix);
    argument.push(value);
    argument
}

fn terminal_arguments(
    terminal_kind: TerminalKind,
    working_directory: &Path,
    title: &str,
    native_executable: &Path,
    terminal_window: Option<&str>,
) -> Vec<OsString> {
    match terminal_kind {
        TerminalKind::WindowsTerminal => vec![
            OsString::from("--window"),
            OsString::from(terminal_window.unwrap_or("new")),
            OsString::from("new-tab"),
            OsString::from("--startingDirectory"),
            working_directory.as_os_str().to_owned(),
            OsString::from("--title"),
            OsString::from(title),
            OsString::from("--suppressApplicationTitle"),
            native_executable.as_os_str().to_owned(),
        ],
        TerminalKind::XdgTerminalExec => vec![
            prefixed_argument("--title=", OsStr::new(title)),
            prefixed_argument("--dir=", working_directory.as_os_str()),
            OsString::from("--"),
            native_executable.as_os_str().to_owned(),
        ],
        TerminalKind::XTerminalEmulator => vec![
            OsString::from("-e"),
            native_executable.as_os_str().to_owned(),
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchSpecError {
    EmptyExecutable,
    WorkingDirectoryNotAbsolute(PathBuf),
    UnsafeWorkingDirectory(PathBuf),
    UnsafeNativeId,
    UnsafeLaunchToken,
    UnsafeInitialPrompt,
    UnsafeTerminalWindow,
    UnsafeCapabilityEnvironment,
}

impl Display for LaunchSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExecutable => formatter.write_str("launch executable cannot be empty"),
            Self::WorkingDirectoryNotAbsolute(path) => write!(
                formatter,
                "launch working directory must be absolute: {}",
                path.display()
            ),
            Self::UnsafeWorkingDirectory(path) => write!(
                formatter,
                "launch working directory contains a control character: {}",
                path.display()
            ),
            Self::UnsafeNativeId => {
                formatter.write_str("native conversation ID contains a control character")
            }
            Self::UnsafeLaunchToken => formatter.write_str("launch token is empty or unsafe"),
            Self::UnsafeInitialPrompt => formatter.write_str("initial prompt is empty or unsafe"),
            Self::UnsafeTerminalWindow => {
                formatter.write_str("terminal window identity is empty or unsafe")
            }
            Self::UnsafeCapabilityEnvironment => {
                formatter.write_str("capability environment name or value is empty or unsafe")
            }
        }
    }
}

impl std::error::Error for LaunchSpecError {}

pub fn sanitise_terminal_title(value: &str) -> String {
    let mut title = value
        .chars()
        .filter(|character| !character.is_control())
        .take(120)
        .collect::<String>();
    if title.trim().is_empty() {
        title = crate::PRODUCT_NAME.to_owned();
    }
    title
}

fn validate_path(path: &Path) -> Result<(), LaunchSpecError> {
    if !path.is_absolute() {
        return Err(LaunchSpecError::WorkingDirectoryNotAbsolute(
            path.to_owned(),
        ));
    }
    if contains_control(path.as_os_str()) {
        return Err(LaunchSpecError::UnsafeWorkingDirectory(path.to_owned()));
    }
    Ok(())
}

fn validate_native_id(native_id: &str) -> Result<(), LaunchSpecError> {
    if native_id.chars().any(char::is_control) {
        return Err(LaunchSpecError::UnsafeNativeId);
    }
    Ok(())
}

fn contains_control(value: &OsStr) -> bool {
    value.to_string_lossy().chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use crate::{ConversationRef, Tool};

    use super::{
        LaunchSpecError, ManagedLaunchMode, ManagedLaunchRequest, ManagedLaunchSpec,
        ResumeLaunchSpec, TerminalKind, WORKBOARD_LAUNCH_TOKEN_ENV, WORKBOARD_WORKFLOW_TOKEN_ENV,
        sanitise_terminal_title,
    };

    #[test]
    fn builds_new_and_resume_managed_launches_with_an_opaque_token() {
        let directory = absolute_fixture_path("managed-worktree");
        let new_launch = ManagedLaunchSpec::new(ManagedLaunchRequest {
            terminal_kind: TerminalKind::WindowsTerminal,
            terminal_executable: "wt.exe".into(),
            native_executable: "codex.exe".into(),
            tool: Tool::Codex,
            mode: ManagedLaunchMode::New,
            working_directory: directory.clone(),
            title: "Planning".to_owned(),
            terminal_window: Some("workboard-feature-feature-123".to_owned()),
            launch_token: "opaque-token".to_owned(),
            workflow_token: Some("workflow-token".to_owned()),
            capability_environment: vec![(
                "CLAUDE_CONFIG_DIR".to_owned(),
                "C:/bundles/session".to_owned(),
            )],
            initial_prompt: Some("Use the managed Feature planning workflow.".to_owned()),
        })
        .expect("new managed launch");
        assert_eq!(
            new_launch.native().arguments(),
            [
                OsString::from("--cd"),
                directory.as_os_str().to_owned(),
                OsString::from("--dangerously-bypass-hook-trust"),
                OsString::from("Use the managed Feature planning workflow.")
            ]
        );
        assert_eq!(new_launch.launch_token(), "opaque-token");
        assert_eq!(new_launch.workflow_token(), Some("workflow-token"));
        assert_eq!(
            &new_launch.terminal().arguments()[..2],
            [
                OsString::from("--window"),
                OsString::from("workboard-feature-feature-123")
            ]
        );
        let command = new_launch.direct_child_command();
        assert_eq!(command.get_current_dir(), Some(directory.as_path()));
        assert_eq!(
            command
                .get_envs()
                .find(|(name, _)| *name == std::ffi::OsStr::new(WORKBOARD_LAUNCH_TOKEN_ENV))
                .and_then(|(_, value)| value),
            Some(std::ffi::OsStr::new("opaque-token"))
        );
        assert_eq!(
            command
                .get_envs()
                .find(|(name, _)| *name == std::ffi::OsStr::new(WORKBOARD_WORKFLOW_TOKEN_ENV))
                .and_then(|(_, value)| value),
            Some(std::ffi::OsStr::new("workflow-token"))
        );
        assert!(
            command.get_envs().any(|(name, value)| {
                name == std::ffi::OsStr::new("NO_COLOR") && value.is_none()
            })
        );
        assert!(
            command
                .get_envs()
                .any(|(name, value)| name == std::ffi::OsStr::new("TERM") && value.is_none())
        );
        assert_eq!(
            command
                .get_envs()
                .find(|(name, _)| *name == std::ffi::OsStr::new("CLAUDE_CONFIG_DIR"))
                .and_then(|(_, value)| value),
            Some(std::ffi::OsStr::new("C:/bundles/session"))
        );

        let resumed = ManagedLaunchSpec::new(ManagedLaunchRequest {
            terminal_kind: TerminalKind::WindowsTerminal,
            terminal_executable: "wt.exe".into(),
            native_executable: "claude.exe".into(),
            tool: Tool::Claude,
            mode: ManagedLaunchMode::Resume("session-123".to_owned()),
            working_directory: directory.clone(),
            title: "Delivery".to_owned(),
            terminal_window: Some("workboard-feature-feature-123".to_owned()),
            launch_token: "opaque-token".to_owned(),
            workflow_token: None,
            capability_environment: Vec::new(),
            initial_prompt: None,
        })
        .expect("managed resume");
        assert_eq!(
            resumed.native().arguments(),
            [OsString::from("--resume"), OsString::from("session-123")]
        );
    }

    #[test]
    fn builds_a_claude_resume_as_one_argument_vector() {
        let directory = absolute_fixture_path("workspace with spaces; echo never");
        let conversation = ConversationRef::new(Tool::Claude, "session-123")
            .expect("the fixture identity should be valid");

        let launch = ResumeLaunchSpec::new(
            TerminalKind::WindowsTerminal,
            "wt.exe",
            "claude.exe",
            &conversation,
            &directory,
            "Feature; $(never)\u{1b}[31m",
        )
        .expect("the launch should be valid");

        assert_eq!(launch.title(), "Feature; $(never)[31m");
        assert_eq!(
            launch.native().arguments(),
            [OsString::from("--resume"), OsString::from("session-123")]
        );
        assert_eq!(
            launch.terminal().arguments(),
            [
                OsString::from("--window"),
                OsString::from("new"),
                OsString::from("new-tab"),
                OsString::from("--startingDirectory"),
                directory.as_os_str().to_owned(),
                OsString::from("--title"),
                OsString::from("Feature; $(never)[31m"),
                OsString::from("--suppressApplicationTitle"),
                OsString::from("claude.exe"),
                OsString::from("--resume"),
                OsString::from("session-123"),
            ]
        );
    }

    #[test]
    fn builds_a_codex_resume_with_an_explicit_working_directory() {
        let directory = absolute_fixture_path("codex-worktree");
        let conversation = ConversationRef::new(Tool::Codex, "thread-456")
            .expect("the fixture identity should be valid");

        let launch = ResumeLaunchSpec::new(
            TerminalKind::WindowsTerminal,
            "wt.exe",
            "codex.exe",
            &conversation,
            &directory,
            "Codex fixture",
        )
        .expect("the launch should be valid");

        assert_eq!(
            launch.native().arguments(),
            [
                OsString::from("--cd"),
                directory.as_os_str().to_owned(),
                OsString::from("--dangerously-bypass-hook-trust"),
                OsString::from("resume"),
                OsString::from("thread-456"),
            ]
        );
    }

    #[test]
    fn rejects_a_relative_working_directory_and_control_native_id() {
        let conversation = ConversationRef::new(Tool::Claude, "session-123")
            .expect("the fixture identity should be valid");
        assert_eq!(
            ResumeLaunchSpec::new(
                TerminalKind::WindowsTerminal,
                "wt.exe",
                "claude.exe",
                &conversation,
                "relative",
                "title",
            ),
            Err(LaunchSpecError::WorkingDirectoryNotAbsolute(PathBuf::from(
                "relative"
            )))
        );

        let conversation = ConversationRef::new(Tool::Claude, "session\n123")
            .expect("the base identity model accepts native text");
        assert_eq!(
            ResumeLaunchSpec::new(
                TerminalKind::WindowsTerminal,
                "wt.exe",
                "claude.exe",
                &conversation,
                absolute_fixture_path("workspace"),
                "title",
            ),
            Err(LaunchSpecError::UnsafeNativeId)
        );
    }

    #[test]
    fn rejects_an_unsafe_capability_environment() {
        let directory = absolute_fixture_path("managed-worktree");
        let request = |name: &str, value: &str| ManagedLaunchRequest {
            terminal_kind: TerminalKind::WindowsTerminal,
            terminal_executable: "wt.exe".into(),
            native_executable: "claude.exe".into(),
            tool: Tool::Claude,
            mode: ManagedLaunchMode::New,
            working_directory: directory.clone(),
            title: "Planning".to_owned(),
            terminal_window: None,
            launch_token: "opaque-token".to_owned(),
            workflow_token: None,
            capability_environment: vec![(name.to_owned(), value.to_owned())],
            initial_prompt: None,
        };

        for (name, value) in [
            ("claude_config_dir", "C:/bundles/session"),
            ("CLAUDE-CONFIG-DIR", "C:/bundles/session"),
            ("", "C:/bundles/session"),
            ("CLAUDE_CONFIG_DIR", ""),
            ("CLAUDE_CONFIG_DIR", "C:/bundles\u{1b}[31m"),
        ] {
            assert_eq!(
                ManagedLaunchSpec::new(request(name, value)),
                Err(LaunchSpecError::UnsafeCapabilityEnvironment),
                "{name}={value} should be rejected"
            );
        }
    }

    #[test]
    fn supplies_a_safe_fallback_for_an_empty_title() {
        assert_eq!(sanitise_terminal_title("\n\r\u{1b}"), "Agent Workboard");
    }

    #[test]
    fn builds_an_xdg_terminal_resume_as_one_argument_vector() {
        let directory = absolute_fixture_path("workspace with spaces; echo never");
        let conversation = ConversationRef::new(Tool::Codex, "thread-456")
            .expect("the fixture identity should be valid");

        let launch = ResumeLaunchSpec::new(
            TerminalKind::XdgTerminalExec,
            "xdg-terminal-exec",
            "codex",
            &conversation,
            &directory,
            "Feature; $(never)\u{1b}[31m",
        )
        .expect("the launch should be valid");

        let mut expected_directory = OsString::from("--dir=");
        expected_directory.push(directory.as_os_str());
        assert_eq!(
            launch.terminal().arguments(),
            [
                OsString::from("--title=Feature; $(never)[31m"),
                expected_directory,
                OsString::from("--"),
                OsString::from("codex"),
                OsString::from("--cd"),
                directory.as_os_str().to_owned(),
                OsString::from("--dangerously-bypass-hook-trust"),
                OsString::from("resume"),
                OsString::from("thread-456"),
            ]
        );
    }

    #[test]
    fn builds_an_x_terminal_emulator_fallback_without_a_shell() {
        let directory = absolute_fixture_path("fallback-workspace");
        let conversation = ConversationRef::new(Tool::Claude, "session-123")
            .expect("the fixture identity should be valid");

        let launch = ResumeLaunchSpec::new(
            TerminalKind::XTerminalEmulator,
            "x-terminal-emulator",
            "claude",
            &conversation,
            &directory,
            "Claude fixture",
        )
        .expect("the launch should be valid");

        assert_eq!(
            launch.terminal().arguments(),
            [
                OsString::from("-e"),
                OsString::from("claude"),
                OsString::from("--resume"),
                OsString::from("session-123"),
            ]
        );
    }

    fn absolute_fixture_path(name: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(r"C:\fixtures\{name}"))
        } else {
            PathBuf::from(format!("/fixtures/{name}"))
        }
    }
}
