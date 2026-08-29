use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};
use time::OffsetDateTime;
use workboard_adapter_claude::ClaudeAdapterV1;
use workboard_adapter_codex::CodexAdapterV1;
use workboard_core::{
    ConversationLifecycle, ConversationRef, LiveEvidenceSource, LiveStatus, ManagedLaunchMode,
    ManagedLaunchRequest, ManagedLaunchSpec, ProcessIdentity, Resumability, ResumeLaunchSpec,
    TerminalKind, Tool,
};
use workboard_native::AdapterFailureKind;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeContext {
    pub working_directory: PathBuf,
    pub title: String,
    pub sources: Vec<ResumeSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeSource {
    pub path: PathBuf,
    pub missing: bool,
    pub snapshot_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveState {
    pub lifecycle: ConversationLifecycle,
    pub evidence: Vec<LiveEvidenceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveEvidenceSummary {
    pub source: LiveEvidenceSource,
    pub status: LiveStatus,
    pub observed_at: String,
    pub expires_at: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResumePreview {
    pub schema_version: u32,
    pub resumability: Resumability,
    pub working_directory: String,
    pub terminal_executable: String,
    pub terminal_arguments: Vec<String>,
    pub native_executable: String,
    pub native_arguments: Vec<String>,
    pub live: LiveState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResumeOutcome {
    pub schema_version: u32,
    pub status: &'static str,
    pub lease_id: workboard_core::LaunchLeaseId,
    pub terminal_pid: u32,
    pub working_directory: String,
    pub live: LiveState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagedLaunchPreview {
    pub schema_version: u32,
    pub working_directory: String,
    pub terminal_executable: String,
    pub terminal_arguments: Vec<String>,
    pub native_executable: String,
    pub native_arguments: Vec<String>,
}

pub struct PreparedResume {
    pub launch: ResumeLaunchSpec,
    pub preview: ResumePreview,
}

pub struct PreparedManagedLaunch {
    pub launch: ManagedLaunchSpec,
    pub preview: ManagedLaunchPreview,
}

pub struct PrepareManagedLaunch {
    pub tool: Tool,
    pub mode: ManagedLaunchMode,
    pub working_directory: PathBuf,
    pub title: String,
    pub terminal_window: Option<String>,
    pub terminal: PathBuf,
    pub native: PathBuf,
    pub launch_token: String,
    pub workflow_token: Option<String>,
    pub initial_prompt: Option<String>,
}

pub trait LaunchExecutor {
    fn launch(&self, specification: &ResumeLaunchSpec) -> Result<LaunchedProcess, AppError>;
}

pub trait ManagedLaunchExecutor {
    fn launch(&self, specification: &ManagedLaunchSpec) -> Result<LaunchedProcess, AppError>;
}

pub trait ProcessInspector {
    fn inspect(&self, pid: u32) -> Option<ProcessIdentity>;
}

pub trait ProcessTerminator {
    fn terminate(&self, expected: &ProcessIdentity) -> Result<(), AppError>;
}

pub struct SystemLaunchExecutor;

impl LaunchExecutor for SystemLaunchExecutor {
    fn launch(&self, specification: &ResumeLaunchSpec) -> Result<LaunchedProcess, AppError> {
        let launched_at = OffsetDateTime::now_utc();
        let child = Command::new(specification.terminal().executable())
            .args(specification.terminal().arguments())
            .current_dir(specification.working_directory())
            .spawn()
            .map_err(AppError::LaunchIo)?;
        let pid = child.id();
        let product_identity = ProcessIdentity::new(
            pid,
            launched_at,
            specification.terminal().executable(),
            Some(std::process::id()),
        )
        .map_err(|error| AppError::Domain(error.to_string()))?;
        let observed_identity = SystemProcessInspector.inspect(pid);
        Ok(LaunchedProcess {
            product_identity,
            observed_identity,
        })
    }
}

impl ManagedLaunchExecutor for SystemLaunchExecutor {
    fn launch(&self, specification: &ManagedLaunchSpec) -> Result<LaunchedProcess, AppError> {
        let launched_at = OffsetDateTime::now_utc();
        let child = specification
            .direct_child_command()
            .spawn()
            .map_err(AppError::LaunchIo)?;
        let pid = child.id();
        let product_identity = ProcessIdentity::new(
            pid,
            launched_at,
            specification.terminal().executable(),
            Some(std::process::id()),
        )
        .map_err(|error| AppError::Domain(error.to_string()))?;
        let observed_identity = SystemProcessInspector.inspect(pid);
        Ok(LaunchedProcess {
            product_identity,
            observed_identity,
        })
    }
}

pub struct SystemProcessInspector;

pub struct SystemProcessTerminator;

impl ProcessInspector for SystemProcessInspector {
    fn inspect(&self, pid: u32) -> Option<ProcessIdentity> {
        let pid = Pid::from_u32(pid);
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
        let process = system.process(pid)?;
        let created_at =
            OffsetDateTime::from_unix_timestamp(i64::try_from(process.start_time()).ok()?).ok()?;
        let executable = process.exe()?.to_path_buf();
        ProcessIdentity::new(
            pid.as_u32(),
            created_at,
            executable,
            process.parent().map(Pid::as_u32),
        )
        .ok()
    }
}

impl ProcessTerminator for SystemProcessTerminator {
    fn terminate(&self, expected: &ProcessIdentity) -> Result<(), AppError> {
        let observed = SystemProcessInspector
            .inspect(expected.pid())
            .ok_or(AppError::ManagedSessionProcessNotFound(expected.pid()))?;
        if !same_process(&observed, expected) {
            return Err(AppError::ManagedSessionProcessIdentityMismatch);
        }
        let pid = Pid::from_u32(expected.pid());
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
        let process = system
            .process(pid)
            .ok_or(AppError::ManagedSessionProcessNotFound(expected.pid()))?;
        if !process.kill() {
            return Err(AppError::ManagedSessionProcessTerminationFailed(
                expected.pid(),
            ));
        }
        Ok(())
    }
}

fn same_process(left: &ProcessIdentity, right: &ProcessIdentity) -> bool {
    left.pid() == right.pid()
        && left.created_at() == right.created_at()
        && left.executable() == right.executable()
        && left.parent_pid() == right.parent_pid()
}

pub struct LaunchedProcess {
    pub product_identity: ProcessIdentity,
    pub observed_identity: Option<ProcessIdentity>,
}

pub fn prepare_resume(
    conversation: &ConversationRef,
    context: &ResumeContext,
    terminal: &Path,
    native: &Path,
    title_override: Option<&str>,
    live: LiveState,
) -> Result<PreparedResume, AppError> {
    let (terminal_kind, terminal) = resolve_terminal(terminal)?;
    let native = resolve_executable(native)
        .ok_or_else(|| AppError::NativeExecutableUnavailable(native.to_owned()))?;
    validate_native_source(conversation, context)?;
    let launch = ResumeLaunchSpec::new(
        terminal_kind,
        &terminal,
        &native,
        conversation,
        &context.working_directory,
        title_override.unwrap_or(&context.title),
    )
    .map_err(|error| AppError::Domain(error.to_string()))?;
    let preview = ResumePreview {
        schema_version: 1,
        resumability: Resumability::PreflightPassed,
        working_directory: path_text(launch.working_directory()),
        terminal_executable: path_text(launch.terminal().executable()),
        terminal_arguments: display_arguments(launch.terminal().arguments()),
        native_executable: path_text(launch.native().executable()),
        native_arguments: display_arguments(launch.native().arguments()),
        live,
    };
    Ok(PreparedResume { launch, preview })
}

pub fn prepare_managed_launch(
    request: PrepareManagedLaunch,
) -> Result<PreparedManagedLaunch, AppError> {
    let (terminal_kind, terminal) = resolve_terminal(&request.terminal)?;
    let native = resolve_executable(&request.native)
        .ok_or_else(|| AppError::NativeExecutableUnavailable(request.native.clone()))?;
    let launch = ManagedLaunchSpec::new(ManagedLaunchRequest {
        terminal_kind,
        terminal_executable: terminal,
        native_executable: native,
        tool: request.tool,
        mode: request.mode,
        working_directory: request.working_directory,
        title: request.title,
        terminal_window: request.terminal_window,
        launch_token: request.launch_token,
        workflow_token: request.workflow_token,
        initial_prompt: request.initial_prompt,
    })
    .map_err(|error| AppError::Domain(error.to_string()))?;
    let preview = ManagedLaunchPreview {
        schema_version: 1,
        working_directory: path_text(launch.working_directory()),
        terminal_executable: path_text(launch.terminal().executable()),
        terminal_arguments: display_arguments(launch.terminal().arguments()),
        native_executable: path_text(launch.native().executable()),
        native_arguments: display_arguments(launch.native().arguments()),
    };
    Ok(PreparedManagedLaunch { launch, preview })
}

fn resolve_terminal(requested: &Path) -> Result<(TerminalKind, PathBuf), AppError> {
    #[cfg(windows)]
    {
        resolve_executable(requested)
            .or_else(|| resolve_windows_terminal_alias(requested))
            .map(|path| (TerminalKind::WindowsTerminal, path))
            .ok_or_else(|| AppError::TerminalExecutableUnavailable(requested.to_owned()))
    }
    #[cfg(target_os = "linux")]
    {
        let requested_kind =
            if requested.file_name() == Some(std::ffi::OsStr::new("x-terminal-emulator")) {
                TerminalKind::XTerminalEmulator
            } else {
                TerminalKind::XdgTerminalExec
            };
        if let Some(path) = resolve_executable(requested) {
            return Ok((requested_kind, path));
        }
        if requested == Path::new("xdg-terminal-exec")
            && let Some(path) = resolve_executable(Path::new("x-terminal-emulator"))
        {
            return Ok((TerminalKind::XTerminalEmulator, path));
        }
        Err(AppError::TerminalExecutableUnavailable(
            requested.to_owned(),
        ))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = requested;
        Err(AppError::ResumePlatformUnsupported)
    }
}

#[cfg(windows)]
fn resolve_windows_terminal_alias(requested: &Path) -> Option<PathBuf> {
    use std::os::windows::fs::MetadataExt;

    let file_name = requested.file_name()?.to_string_lossy();
    if !file_name.eq_ignore_ascii_case("wt") && !file_name.eq_ignore_ascii_case("wt.exe") {
        return None;
    }
    let candidates = if requested.is_absolute() || requested.components().count() > 1 {
        vec![requested.to_path_buf()]
    } else {
        env::split_paths(&env::var_os("PATH")?)
            .map(|directory| directory.join("wt.exe"))
            .collect()
    };
    candidates.into_iter().find(|candidate| {
        let in_windows_apps = candidate
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|parent| parent.eq_ignore_ascii_case("WindowsApps"));
        let metadata = fs::symlink_metadata(candidate).ok();
        in_windows_apps
            && metadata.as_ref().is_some_and(|metadata| {
                const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                metadata.len() == 0
                    && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            })
    })
}

pub(crate) fn validate_native_source(
    conversation: &ConversationRef,
    context: &ResumeContext,
) -> Result<(), AppError> {
    let mut missing = true;
    let mut last_error = None;
    for source in context.sources.iter().filter(|source| !source.missing) {
        missing = false;
        let snapshot: workboard_native::NativeConversation =
            match serde_json::from_str(&source.snapshot_json) {
                Ok(value) => value,
                Err(error) => {
                    last_error = Some(format!("last-good native snapshot is corrupt: {error}"));
                    continue;
                }
            };
        if snapshot.native_id != conversation.native_id()
            || snapshot.kind != workboard_native::ConversationKind::TopLevel
        {
            last_error = Some("last-good native snapshot identity does not match".to_owned());
            continue;
        }
        let result =
            match conversation.tool() {
                Tool::Claude => ClaudeAdapterV1::default()
                    .preflight_resume(&source.path, conversation.native_id()),
                Tool::Codex => CodexAdapterV1::default()
                    .preflight_resume(&source.path, conversation.native_id()),
            };
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(match error.kind {
                    AdapterFailureKind::Io => {
                        format!("native source is unavailable: {}", error.message)
                    }
                    _ => format!(
                        "native source failed read-only validation: {}",
                        error.message
                    ),
                });
            }
        }
    }
    Err(AppError::ConversationNotResumable(if missing {
        "no readable native source is present".to_owned()
    } else {
        last_error.unwrap_or_else(|| "native source validation failed".to_owned())
    }))
}

fn resolve_executable(requested: &Path) -> Option<PathBuf> {
    if requested.is_absolute() || requested.components().count() > 1 {
        return canonical_executable_file(requested);
    }
    let search_path = env::var_os("PATH")?;
    let directories = env::split_paths(&search_path).collect::<Vec<_>>();
    resolve_executable_from_directories(requested, &directories)
}

fn resolve_executable_from_directories(
    requested: &Path,
    directories: &[PathBuf],
) -> Option<PathBuf> {
    let extensions = executable_extensions(requested);
    for directory in directories {
        for extension in &extensions {
            let candidate = directory.join(format!(
                "{}{}",
                requested.to_string_lossy(),
                extension.to_string_lossy()
            ));
            if let Some(path) = canonical_executable_file(&candidate) {
                return Some(path);
            }
        }
    }
    None
}

pub fn native_executable_available(requested: &Path) -> bool {
    resolve_executable(requested).is_some()
}

fn executable_extensions(requested: &Path) -> Vec<OsString> {
    if requested.extension().is_some() {
        return vec![OsString::new()];
    }
    #[cfg(windows)]
    {
        vec![OsString::from(".exe"), OsString::from(".com")]
    }
    #[cfg(not(windows))]
    {
        vec![OsString::new()]
    }
}

fn canonical_executable_file(path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    if !path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("com")
    }) {
        return None;
    }
    canonical_file(path)
}

fn canonical_file(path: &Path) -> Option<PathBuf> {
    let metadata = fs::metadata(path).ok()?;
    metadata
        .is_file()
        .then(|| fs::canonicalize(path).ok())
        .flatten()
}

fn display_arguments(arguments: &[OsString]) -> Vec<String> {
    arguments
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_adapter_codex::CodexAdapterV1;
    use workboard_core::{ConversationLifecycle, ConversationRef, ProcessIdentity, Tool};
    use workboard_native::NativeAdapter;

    use super::{
        LiveState, ResumeContext, ResumeSource, prepare_resume, resolve_executable_from_directories,
    };

    #[derive(Clone)]
    struct FakeProcess {
        identity: ProcessIdentity,
    }

    fn find_process(
        processes: &[FakeProcess],
        pid: u32,
        created_at: OffsetDateTime,
        executable: &str,
    ) -> bool {
        processes
            .iter()
            .map(|process| &process.identity)
            .any(|identity| {
                identity.pid() == pid
                    && identity.created_at() == created_at
                    && identity.executable() == Path::new(executable)
            })
    }

    #[test]
    fn fake_process_tree_rejects_pid_reuse_and_wrong_executable() {
        let old_time = OffsetDateTime::from_unix_timestamp(1_700_000_000)
            .expect("the fixture timestamp should be valid");
        let new_time = OffsetDateTime::from_unix_timestamp(1_700_000_100)
            .expect("the fixture timestamp should be valid");
        let processes = vec![FakeProcess {
            identity: ProcessIdentity::new(81, new_time, "C:/fake/claude.exe", Some(20))
                .expect("the fixture process should be valid"),
        }];

        assert!(!find_process(
            &processes,
            81,
            old_time,
            "C:/fake/claude.exe"
        ));
        assert!(!find_process(&processes, 81, new_time, "C:/fake/codex.exe"));
        assert!(find_process(&processes, 81, new_time, "C:/fake/claude.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn executable_resolution_skips_nvm_shims_for_a_real_executable() {
        let directory = TempDir::new().expect("temporary directory");
        let nvm = directory.path().join("nvm");
        let native = directory.path().join("native");
        fs::create_dir_all(&nvm).expect("nvm directory");
        fs::create_dir_all(&native).expect("native directory");
        fs::write(nvm.join("claude"), []).expect("extensionless nvm shim");
        fs::write(nvm.join("claude.cmd"), []).expect("nvm command shim");
        fs::write(native.join("claude.exe"), []).expect("native executable");

        let resolved =
            resolve_executable_from_directories(Path::new("claude"), &[nvm, native.clone()])
                .expect("native executable should resolve");

        assert_eq!(
            resolved,
            fs::canonicalize(native.join("claude.exe")).expect("canonical native executable")
        );
    }

    #[test]
    fn prepares_a_shell_free_resume_from_a_verified_native_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../workboard-adapter-codex/tests/fixtures/sessions");
        let scan = CodexAdapterV1::default()
            .scan(&root, &HashMap::new())
            .expect("scan native fixtures");
        let source = scan
            .sources
            .iter()
            .find(|source| source.conversation.native_id == "codex-resume")
            .expect("resume source");
        let directory = TempDir::new().expect("temporary directory");
        let terminal = directory.path().join("wt.exe");
        let native = directory.path().join("codex.exe");
        fs::write(&terminal, []).expect("terminal fixture");
        fs::write(&native, []).expect("native fixture");
        let conversation =
            ConversationRef::new(Tool::Codex, "codex-resume").expect("conversation reference");
        let context = ResumeContext {
            working_directory: directory.path().to_path_buf(),
            title: "Work item".to_owned(),
            sources: vec![ResumeSource {
                path: source.path.clone(),
                missing: false,
                snapshot_json: serde_json::to_string(&source.conversation).expect("snapshot JSON"),
            }],
        };

        let prepared = prepare_resume(
            &conversation,
            &context,
            &terminal,
            &native,
            None,
            LiveState {
                lifecycle: ConversationLifecycle::NotLive,
                evidence: Vec::new(),
            },
        )
        .expect("resume preflight");

        assert_eq!(
            prepared.preview.native_arguments,
            vec![
                "--cd".to_owned(),
                directory.path().to_string_lossy().into_owned(),
                "--dangerously-bypass-hook-trust".to_owned(),
                "resume".to_owned(),
                "codex-resume".to_owned(),
            ]
        );
    }
}
