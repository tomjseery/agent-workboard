use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("no Claude or Codex conversation identity was found in the caller environment")]
    CallerIdentityMissing,
    #[error("both Claude and Codex caller identities are present; pass --tool to select one")]
    CallerIdentityAmbiguous,
    #[error("the {tool} caller identity is not present in the environment")]
    RequestedCallerIdentityMissing { tool: &'static str },
    #[error("--worktree must be an absolute path: {0}")]
    WorktreePathNotAbsolute(PathBuf),
    #[error("the worktree path does not exist or is not a directory: {0}")]
    WorktreePathInvalid(PathBuf),
    #[error("the path is inside a worktree but is not its root: {0}")]
    WorktreePathNotRoot(PathBuf),
    #[error("Git did not report the path as a registered worktree: {0}")]
    WorktreeNotRegistered(PathBuf),
    #[error("failed to run Git: {0}")]
    GitIo(#[source] std::io::Error),
    #[error("Git could not resolve the target worktree: {message}")]
    GitCommand { message: String },
    #[error("Git returned non-UTF-8 output while resolving the worktree")]
    GitOutputEncoding,
    #[error("the resolved Git path cannot be represented as UTF-8: {0}")]
    GitPathEncoding(PathBuf),
    #[error("the association reason cannot be blank")]
    EmptyReason,
    #[error("the idempotency key cannot be blank")]
    EmptyIdempotencyKey,
    #[error("the idempotency key already belongs to a different association request")]
    IdempotencyConflict,
    #[error("the requested Work item belongs to a different repository")]
    WorkItemRepositoryMismatch,
    #[error("the requested Work item does not exist or has no checkout history")]
    WorkItemNotFound,
    #[error("the requested conversation is not present in Workboard")]
    ConversationNotFound,
    #[error("the conversation already has a primary Work item; use correct")]
    ConversationAlreadyAssigned,
    #[error("the conversation has no primary Work item to correct or confirm")]
    ConversationNotAssigned,
    #[error("manual assignment and correction require a Work item")]
    WorkItemRequired,
    #[error("confirmation uses the current resolved Work item and accepts no target")]
    ConfirmationTargetProvided,
    #[error("the requested action is not a manual association action")]
    InvalidManualAction,
    #[error("bulk assignment requires at least one conversation")]
    EmptyBulkAssignment,
    #[error("the effective time is not a valid RFC 3339 timestamp: {0}")]
    InvalidEffectiveTime(String),
    #[error("the Work item title cannot be blank")]
    EmptyWorkItemTitle,
    #[error("the user metadata request ID cannot be blank")]
    EmptyMetadataRequestId,
    #[error("the user metadata request ID already belongs to a different edit")]
    MetadataIdempotencyConflict,
    #[error("the conversation title must be at most 200 characters")]
    ConversationTitleTooLong,
    #[error("conversation notes must be at most 10000 characters")]
    ConversationNotesTooLong,
    #[error("the requested repository does not exist in Workboard")]
    SearchRepositoryNotFound,
    #[error("GitHub pull-request discovery is disabled")]
    ProviderDisabled,
    #[error("failed to run the GitHub CLI: {0}")]
    ProviderIo(#[source] std::io::Error),
    #[error("GitHub discovery failed: {message}")]
    ProviderCommand { message: String },
    #[error("GitHub returned invalid structured data: {message}")]
    ProviderOutput { message: String },
    #[error("the Work item checkout was not present in the completed Git scan")]
    WorkItemRepositoryNotScanned,
    #[error("the selected checkout belongs to a different repository")]
    ResumeRepositoryMismatch,
    #[error("the selected checkout has not been included in a completed Git scan")]
    ResumeCheckoutNotScanned,
    #[error("the checkout recreation path must not already exist: {0}")]
    RecreateCheckoutPathExists(PathBuf),
    #[error("the checkout recreation path has no existing parent directory: {0}")]
    RecreateCheckoutParentMissing(PathBuf),
    #[error("no current checkout is available; pass --worktree or recreate the checkout")]
    ResumeCheckoutRequired,
    #[error("the conversation is not resumable: {0}")]
    ConversationNotResumable(String),
    #[error("the native executable is unavailable: {0}")]
    NativeExecutableUnavailable(PathBuf),
    #[error("the terminal launcher executable is unavailable: {0}")]
    TerminalExecutableUnavailable(PathBuf),
    #[error("conversation resume is not supported on this platform")]
    ResumePlatformUnsupported,
    #[error("a confirmed live instance already owns this conversation")]
    DuplicateConfirmed,
    #[error("live state is uncertain; inspect the evidence or pass --allow-uncertain")]
    DuplicateUncertain,
    #[error("the launch lease was lost before launch completed")]
    LaunchLeaseLost,
    #[error("the managed launch token is missing, unknown, or no longer usable")]
    LaunchTokenInvalid,
    #[error("the workflow operation is not authorized by a current managed-session binding")]
    WorkflowOperationUnauthorized,
    #[error("a repository workflow document changed before the checkpoint committed")]
    WorkflowDocumentChanged,
    #[error("the live-state observation is invalid: {0}")]
    InvalidLiveObservation(String),
    #[error("failed to read native hook input: {0}")]
    HookInputIo(#[source] std::io::Error),
    #[error("native hook input exceeds the {limit}-byte limit")]
    HookInputTooLarge { limit: usize },
    #[error("native hook input is invalid: {0}")]
    InvalidHookInput(String),
    #[error("the {tool} hook event is unsupported: {event}")]
    UnsupportedHookEvent { tool: &'static str, event: String },
    #[error("helper or subagent hook identity cannot identify a top-level conversation")]
    HelperHookIdentity,
    #[error("the caller identity has no exact hook observation")]
    CallerIdentityUncorrelated,
    #[error("the caller identity's exact hook observation has expired")]
    CallerIdentityExpired,
    #[error("the caller identity does not match the currently observed native conversation")]
    CallerIdentityMismatch,
    #[error("the caller conversation is no longer active")]
    CallerIdentityNotActive,
    #[error("the {label} must be an absolute path: {path}")]
    IntegrationPathNotAbsolute { label: &'static str, path: PathBuf },
    #[error("the {label} is unavailable or is not a file: {path}")]
    IntegrationPathInvalid { label: &'static str, path: PathBuf },
    #[error("native integration configuration is too large at {path}; limit is {limit} bytes")]
    IntegrationConfigurationTooLarge { path: PathBuf, limit: u64 },
    #[error("native integration configuration is malformed at {path}: {message}")]
    IntegrationConfigurationMalformed { path: PathBuf, message: String },
    #[error("native integration configuration changed while it was being updated: {0}")]
    IntegrationConfigurationChanged(PathBuf),
    #[error("native integration confirmation is missing, expired, or already used: {0}")]
    IntegrationConfirmationInvalid(PathBuf),
    #[error("{tool} native integration is unavailable: {reason}")]
    IntegrationUnavailable { tool: &'static str, reason: String },
    #[error("native integration I/O failed while {operation} at {path}: {source}")]
    IntegrationIo {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to launch the terminal: {0}")]
    LaunchIo(#[source] std::io::Error),
    #[error("the Workboard data directory is unavailable")]
    DataDirectoryUnavailable,
    #[error("failed to create the Workboard data directory: {0}")]
    CreateDataDirectory(#[source] std::io::Error),
    #[error("recovery I/O failed while {operation}: {source}")]
    RecoveryIo {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("Workboard storage failed: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("failed to encode the association event: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("native {tool} discovery failed: {message}")]
    Adapter { tool: &'static str, message: String },
    #[error("association data was invalid: {0}")]
    Domain(String),
    #[error("storage interruption injected for verification")]
    InjectedStorageInterruption,
    #[error("{message}")]
    External { code: String, message: String },
}

impl AppError {
    pub fn code(&self) -> &str {
        match self {
            Self::CallerIdentityMissing => "caller_identity_missing",
            Self::CallerIdentityAmbiguous => "caller_identity_ambiguous",
            Self::RequestedCallerIdentityMissing { .. } => "requested_caller_identity_missing",
            Self::WorktreePathNotAbsolute(_) => "worktree_path_not_absolute",
            Self::WorktreePathInvalid(_) => "worktree_path_invalid",
            Self::WorktreePathNotRoot(_) => "worktree_path_not_root",
            Self::WorktreeNotRegistered(_) => "worktree_not_registered",
            Self::GitIo(_) => "git_io",
            Self::GitCommand { .. } => "git_command",
            Self::GitOutputEncoding => "git_output_encoding",
            Self::GitPathEncoding(_) => "git_path_encoding",
            Self::EmptyReason => "empty_reason",
            Self::EmptyIdempotencyKey => "empty_idempotency_key",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::WorkItemRepositoryMismatch => "work_item_repository_mismatch",
            Self::WorkItemNotFound => "work_item_not_found",
            Self::ConversationNotFound => "conversation_not_found",
            Self::ConversationAlreadyAssigned => "conversation_already_assigned",
            Self::ConversationNotAssigned => "conversation_not_assigned",
            Self::WorkItemRequired => "work_item_required",
            Self::ConfirmationTargetProvided => "confirmation_target_provided",
            Self::InvalidManualAction => "invalid_manual_action",
            Self::EmptyBulkAssignment => "empty_bulk_assignment",
            Self::InvalidEffectiveTime(_) => "invalid_effective_time",
            Self::EmptyWorkItemTitle => "empty_work_item_title",
            Self::EmptyMetadataRequestId => "empty_metadata_request_id",
            Self::MetadataIdempotencyConflict => "metadata_idempotency_conflict",
            Self::ConversationTitleTooLong => "conversation_title_too_long",
            Self::ConversationNotesTooLong => "conversation_notes_too_long",
            Self::SearchRepositoryNotFound => "search_repository_not_found",
            Self::ProviderDisabled => "provider_disabled",
            Self::ProviderIo(_) => "provider_io",
            Self::ProviderCommand { .. } => "provider_command",
            Self::ProviderOutput { .. } => "provider_output",
            Self::WorkItemRepositoryNotScanned => "work_item_repository_not_scanned",
            Self::ResumeRepositoryMismatch => "resume_repository_mismatch",
            Self::ResumeCheckoutNotScanned => "resume_checkout_not_scanned",
            Self::RecreateCheckoutPathExists(_) => "recreate_checkout_path_exists",
            Self::RecreateCheckoutParentMissing(_) => "recreate_checkout_parent_missing",
            Self::ResumeCheckoutRequired => "resume_checkout_required",
            Self::ConversationNotResumable(_) => "conversation_not_resumable",
            Self::NativeExecutableUnavailable(_) => "native_executable_unavailable",
            Self::TerminalExecutableUnavailable(_) => "terminal_executable_unavailable",
            Self::ResumePlatformUnsupported => "resume_platform_unsupported",
            Self::DuplicateConfirmed => "duplicate_confirmed",
            Self::DuplicateUncertain => "duplicate_uncertain",
            Self::LaunchLeaseLost => "launch_lease_lost",
            Self::LaunchTokenInvalid => "launch_token_invalid",
            Self::WorkflowOperationUnauthorized => "workflow_operation_unauthorized",
            Self::WorkflowDocumentChanged => "workflow_document_changed",
            Self::InvalidLiveObservation(_) => "invalid_live_observation",
            Self::HookInputIo(_) => "hook_input_io",
            Self::HookInputTooLarge { .. } => "hook_input_too_large",
            Self::InvalidHookInput(_) => "invalid_hook_input",
            Self::UnsupportedHookEvent { .. } => "unsupported_hook_event",
            Self::HelperHookIdentity => "helper_hook_identity",
            Self::CallerIdentityUncorrelated => "caller_identity_uncorrelated",
            Self::CallerIdentityExpired => "caller_identity_expired",
            Self::CallerIdentityMismatch => "caller_identity_mismatch",
            Self::CallerIdentityNotActive => "caller_identity_not_active",
            Self::IntegrationPathNotAbsolute { .. } => "integration_path_not_absolute",
            Self::IntegrationPathInvalid { .. } => "integration_path_invalid",
            Self::IntegrationConfigurationTooLarge { .. } => "integration_configuration_too_large",
            Self::IntegrationConfigurationMalformed { .. } => "integration_configuration_malformed",
            Self::IntegrationConfigurationChanged(_) => "integration_configuration_changed",
            Self::IntegrationConfirmationInvalid(_) => "integration_confirmation_invalid",
            Self::IntegrationUnavailable { .. } => "integration_unavailable",
            Self::IntegrationIo { .. } => "integration_io",
            Self::LaunchIo(_) => "launch_io",
            Self::DataDirectoryUnavailable => "data_directory_unavailable",
            Self::CreateDataDirectory(_) => "create_data_directory",
            Self::RecoveryIo { .. } => "recovery_io",
            Self::Storage(_) => "storage",
            Self::Encode(_) => "encode",
            Self::Adapter { .. } => "adapter",
            Self::Domain(_) => "domain",
            Self::InjectedStorageInterruption => "injected_storage_interruption",
            Self::External { code, .. } => code,
        }
    }
}
