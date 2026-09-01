use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use workboard_application::AppError;
use workboard_application::native_sources::RefreshNativeSources;
use workboard_application::projection::{ReplayResult, core_workspace_id};
use workboard_application::workspace::WorkboardApplication;
use workboard_client_protocol::{
    AvailableAction, CURRENT_PROTOCOL_VERSION, CommandCapability, CommandCode, CommandOperation,
    DaemonInstanceId, EventCursor, HandshakeResponse, Heartbeat, MAX_COLLECTION_ITEMS,
    MAX_DIAGNOSTICS, MAX_FRAME_BYTES, Operation, PREVIOUS_PROTOCOL_VERSION, ProtocolError,
    ReadQuery, RequestEnvelope as ClientRequestEnvelope, RequestId,
    ResponseEnvelope as ClientResponseEnvelope, ResponseResult, SUPPORTED_READ_VERSIONS,
    ServerMessage, UnavailableReason, WorkspaceId,
};
use workboard_core::Tool;

use crate::client::DaemonClient;
use crate::error::DaemonError;
use crate::protocol::{
    LEGACY_PROTOCOL_VERSION, MAX_MESSAGE_BYTES, RemoteError, RequestEnvelope, ResponseEnvelope,
    WriteCommand,
};
use crate::watcher::{self, WatchConfig};

const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(500);
const SUBSCRIPTION_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub trait CommandHandler: Send + 'static {
    fn handle(&mut self, command: WriteCommand) -> Result<Value, RemoteError>;

    fn handle_protocol(
        &mut self,
        request: &ClientRequestEnvelope,
        _daemon_instance_id: DaemonInstanceId,
    ) -> ClientResponseEnvelope {
        ClientResponseEnvelope::failure(
            CURRENT_PROTOCOL_VERSION,
            request,
            ProtocolError::new(
                "operation_unavailable",
                "the typed Workboard operation is unavailable",
            ),
        )
    }

    fn replay(
        &mut self,
        workspace_id: WorkspaceId,
        cursor: EventCursor,
        version: u32,
        daemon_instance_id: DaemonInstanceId,
    ) -> Result<ReplayResult, Box<ProtocolError>> {
        let _ = (workspace_id, cursor, version, daemon_instance_id);
        Ok(ReplayResult::Events(Vec::new()))
    }

    fn revision(&mut self, workspace_id: WorkspaceId) -> Result<u64, Box<ProtocolError>> {
        let _ = workspace_id;
        Ok(0)
    }
}

impl<F> CommandHandler for F
where
    F: FnMut(WriteCommand) -> Result<Value, RemoteError> + Send + 'static,
{
    fn handle(&mut self, command: WriteCommand) -> Result<Value, RemoteError> {
        self(command)
    }
}

pub struct ApplicationCommandHandler {
    application: WorkboardApplication,
}

impl ApplicationCommandHandler {
    pub fn new(application: WorkboardApplication) -> Self {
        Self { application }
    }
}

impl CommandHandler for ApplicationCommandHandler {
    fn handle(&mut self, command: WriteCommand) -> Result<Value, RemoteError> {
        match command {
            WriteCommand::Ping => Ok(json!({ "status": "ready" })),
            WriteCommand::RefreshNativeSessions {
                claude_root,
                codex_root,
            } => {
                let observed_at = OffsetDateTime::now_utc();
                let mut outcomes = Vec::new();
                if let Some(root) = claude_root {
                    outcomes.push(refresh(
                        &mut self.application,
                        Tool::Claude,
                        root,
                        observed_at,
                    )?);
                }
                if let Some(root) = codex_root {
                    outcomes.push(refresh(
                        &mut self.application,
                        Tool::Codex,
                        root,
                        observed_at,
                    )?);
                }
                serde_json::to_value(outcomes).map_err(|error| RemoteError {
                    code: "encoding_failed".to_owned(),
                    message: error.to_string(),
                })
            }
        }
    }

    fn handle_protocol(
        &mut self,
        request: &ClientRequestEnvelope,
        daemon_instance_id: DaemonInstanceId,
    ) -> ClientResponseEnvelope {
        execute_protocol(&mut self.application, request, daemon_instance_id)
    }

    fn replay(
        &mut self,
        workspace_id: WorkspaceId,
        cursor: EventCursor,
        version: u32,
        daemon_instance_id: DaemonInstanceId,
    ) -> Result<ReplayResult, Box<ProtocolError>> {
        self.application
            .replay_client_events(
                core_workspace_id(workspace_id),
                daemon_instance_id,
                cursor,
                version,
                workboard_client_protocol::MAX_REPLAY_EVENTS,
            )
            .map_err(|error| Box::new(protocol_application_error(error)))
    }

    fn revision(&mut self, workspace_id: WorkspaceId) -> Result<u64, Box<ProtocolError>> {
        self.application
            .projection_revision(core_workspace_id(workspace_id))
            .map_err(|error| Box::new(protocol_application_error(error)))
    }
}

fn refresh(
    application: &mut WorkboardApplication,
    tool: Tool,
    root: PathBuf,
    observed_at: OffsetDateTime,
) -> Result<workboard_application::native_sources::NativeRefreshOutcome, RemoteError> {
    application
        .native_sources()
        .refresh(RefreshNativeSources {
            tool,
            root,
            observed_at,
        })
        .map_err(|error| RemoteError {
            code: error.code().to_owned(),
            message: error.to_string(),
        })
}

fn execute_protocol(
    application: &mut WorkboardApplication,
    request: &ClientRequestEnvelope,
    daemon_instance_id: DaemonInstanceId,
) -> ClientResponseEnvelope {
    if let Err(error) = request.validate() {
        return ClientResponseEnvelope::failure(CURRENT_PROTOCOL_VERSION, request, *error);
    }
    match &request.operation {
        Operation::Handshake(handshake) => {
            let negotiated = workboard_client_protocol::negotiate_read_version(
                &handshake.supported_read_versions,
            );
            let Some(negotiated) = negotiated else {
                return ClientResponseEnvelope::failure(
                    CURRENT_PROTOCOL_VERSION,
                    request,
                    ProtocolError::new(
                        "incompatible_protocol",
                        "no compatible Workboard read protocol is available",
                    ),
                );
            };
            let compatible_command_versions = handshake
                .supported_command_versions
                .iter()
                .copied()
                .filter(|version| *version == CURRENT_PROTOCOL_VERSION)
                .collect::<Vec<_>>();
            let workspaces = match application.client_workspaces() {
                Ok(workspaces) => workspaces,
                Err(error) => return application_failure(negotiated, request, error),
            };
            ClientResponseEnvelope::success(
                negotiated,
                request,
                None,
                ResponseResult::Handshake(HandshakeResponse {
                    daemon_instance_id,
                    negotiated_read_version: negotiated,
                    compatible_command_versions,
                    workspaces,
                    command_capabilities: command_capabilities(),
                    event_version: 1,
                    heartbeat_interval_ms: HEARTBEAT_INTERVAL.as_millis() as u64,
                    max_frame_bytes: MAX_FRAME_BYTES,
                }),
                Vec::new(),
            )
        }
        Operation::Query(query) => {
            if !SUPPORTED_READ_VERSIONS.contains(&request.protocol_version) {
                return incompatible(request);
            }
            let Some(workspace_id) = request.workspace_id else {
                return ClientResponseEnvelope::failure(
                    request.protocol_version,
                    request,
                    ProtocolError::validation("workspace_id", "workspace_required"),
                );
            };
            let core_id = core_workspace_id(workspace_id);
            let operational_query = matches!(
                query,
                ReadQuery::RepositoryObservability { .. }
                    | ReadQuery::CheckoutObservability { .. }
                    | ReadQuery::SessionObservability { .. }
                    | ReadQuery::RecoveryPreview { .. }
                    | ReadQuery::ApprovalQueue
                    | ReadQuery::FeatureProposal { .. }
                    | ReadQuery::WorkItemDetail { .. }
            );
            let revision = match application.projection_revision(core_id) {
                Ok(revision) => revision,
                Err(_) if operational_query => {
                    return ClientResponseEnvelope::failure(
                        request.protocol_version,
                        request,
                        ProtocolError::new(
                            "projection_unavailable",
                            "The requested Workboard evidence is unavailable.",
                        ),
                    );
                }
                Err(error) => return application_failure(request.protocol_version, request, error),
            };
            let result = match query {
                ReadQuery::WorkspaceSummary => application
                    .client_workspace_summary(core_id)
                    .map(ResponseResult::WorkspaceSummary),
                ReadQuery::HierarchyChildren { parent } => application
                    .client_hierarchy_children(core_id, *parent)
                    .map(ResponseResult::HierarchyChildren),
                ReadQuery::WorkspaceHierarchy => application
                    .client_workspace_hierarchy(core_id)
                    .map(ResponseResult::WorkspaceHierarchy),
                ReadQuery::BoardViews => application
                    .client_board_views(core_id)
                    .map(ResponseResult::BoardViews),
                ReadQuery::BoardView { view_id } => application
                    .client_board_view(core_id, *view_id)
                    .map(ResponseResult::BoardView),
                ReadQuery::Board { query } => application
                    .client_board(core_id, query.clone())
                    .map(ResponseResult::Board),
                ReadQuery::Attention { query } => application
                    .client_attention(core_id, query.clone())
                    .map(ResponseResult::Attention),
                ReadQuery::ApprovalQueue
                    if request.protocol_version == CURRENT_PROTOCOL_VERSION =>
                {
                    application
                        .client_approval_queue(core_id)
                        .map(ResponseResult::ApprovalQueue)
                }
                ReadQuery::FeatureProposal { feature_id }
                    if request.protocol_version == CURRENT_PROTOCOL_VERSION =>
                {
                    application
                        .client_feature_proposal(
                            core_id,
                            workboard_core::FeatureId::from_uuid(*feature_id.as_uuid()),
                        )
                        .map(ResponseResult::FeatureProposal)
                }
                ReadQuery::WorkItemDetail { work_item_id }
                    if request.protocol_version == CURRENT_PROTOCOL_VERSION =>
                {
                    application
                        .client_work_item_detail(
                            core_id,
                            workboard_core::WorkItemId::from_uuid(*work_item_id.as_uuid()),
                        )
                        .map(|detail| ResponseResult::WorkItemDetail(Box::new(detail)))
                }
                ReadQuery::RepositoryObservability { repository_id }
                    if request.protocol_version == CURRENT_PROTOCOL_VERSION =>
                {
                    application
                        .client_repository_observability(
                            core_id,
                            workboard_core::RepositoryId::from_uuid(*repository_id.as_uuid()),
                        )
                        .map(ResponseResult::RepositoryObservability)
                }
                ReadQuery::CheckoutObservability { checkout_id }
                    if request.protocol_version == CURRENT_PROTOCOL_VERSION =>
                {
                    application
                        .client_checkout_observability(
                            core_id,
                            workboard_core::CheckoutId::from_uuid(*checkout_id.as_uuid()),
                        )
                        .map(ResponseResult::CheckoutObservability)
                }
                ReadQuery::SessionObservability { session_id }
                    if request.protocol_version == CURRENT_PROTOCOL_VERSION =>
                {
                    application
                        .client_session_observability(
                            core_id,
                            workboard_core::ConversationId::from_uuid(*session_id.as_uuid()),
                        )
                        .map(ResponseResult::SessionObservability)
                }
                ReadQuery::RecoveryPreview { session_id }
                    if request.protocol_version == CURRENT_PROTOCOL_VERSION =>
                {
                    application
                        .client_recovery_preview(
                            core_id,
                            workboard_core::ConversationId::from_uuid(*session_id.as_uuid()),
                        )
                        .map(ResponseResult::RecoveryPreview)
                }
                ReadQuery::RepositoryObservability { .. }
                | ReadQuery::CheckoutObservability { .. }
                | ReadQuery::SessionObservability { .. }
                | ReadQuery::RecoveryPreview { .. }
                | ReadQuery::ApprovalQueue
                | ReadQuery::FeatureProposal { .. }
                | ReadQuery::WorkItemDetail { .. } => Err(AppError::External {
                    code: "projection_version_unavailable".to_owned(),
                    message:
                        "the requested projection is unavailable for the negotiated read version"
                            .to_owned(),
                }),
                ReadQuery::BoardSnapshot => application
                    .client_board_snapshot(core_id)
                    .map(ResponseResult::BoardSnapshot),
            };
            match result {
                Ok(result) if response_within_limits(&result) => ClientResponseEnvelope::success(
                    request.protocol_version,
                    request,
                    Some(revision),
                    result,
                    available_actions(revision),
                ),
                Ok(_) => ClientResponseEnvelope::failure(
                    request.protocol_version,
                    request,
                    ProtocolError::new(
                        "collection_too_large",
                        "the authoritative projection exceeds the collection bound",
                    ),
                ),
                Err(error)
                    if operational_query && error.code() != "projection_version_unavailable" =>
                {
                    ClientResponseEnvelope::failure(
                        request.protocol_version,
                        request,
                        ProtocolError::new(
                            "projection_unavailable",
                            "The requested Workboard evidence is unavailable.",
                        ),
                    )
                }
                Err(error) => application_failure(request.protocol_version, request, error),
            }
        }
        Operation::Subscribe(subscription) => {
            if !SUPPORTED_READ_VERSIONS.contains(&request.protocol_version) {
                return incompatible(request);
            }
            let Some(workspace_id) = request.workspace_id else {
                return ClientResponseEnvelope::failure(
                    request.protocol_version,
                    request,
                    ProtocolError::validation("workspace_id", "workspace_required"),
                );
            };
            let revision = match application.projection_revision(core_workspace_id(workspace_id)) {
                Ok(revision) => revision,
                Err(error) => return application_failure(request.protocol_version, request, error),
            };
            let cursor = subscription.cursor.unwrap_or(EventCursor {
                daemon_instance_id,
                sequence: revision,
            });
            ClientResponseEnvelope::success(
                request.protocol_version,
                request,
                Some(revision),
                ResponseResult::SubscriptionAccepted { cursor },
                available_actions(revision),
            )
        }
        Operation::Command(command) => {
            if request.protocol_version != CURRENT_PROTOCOL_VERSION {
                return incompatible(request);
            }
            let context = CommandContext {
                workspace_id: request.workspace_id.expect("validated Workspace"),
                expected_revision: request.expected_revision.expect("validated revision"),
                idempotency_key: request
                    .idempotency_key
                    .clone()
                    .expect("validated idempotency key"),
                request_id: request.request_id,
            };
            match dispatch_command(application, &context, command) {
                Ok(result) => {
                    let revision = application
                        .projection_revision(core_workspace_id(context.workspace_id))
                        .expect("committed command revision");
                    ClientResponseEnvelope::success(
                        request.protocol_version,
                        request,
                        Some(revision),
                        result,
                        available_actions(revision),
                    )
                }
                Err(CommandFailure::Application(error)) => {
                    application_failure(request.protocol_version, request, error)
                }
                Err(CommandFailure::Unavailable(reason)) => {
                    let mut error = ProtocolError::new("capability_unavailable", reason.message);
                    error.current_revision = application
                        .projection_revision(core_workspace_id(context.workspace_id))
                        .ok();
                    ClientResponseEnvelope::failure(request.protocol_version, request, error)
                }
            }
        }
    }
}

fn response_within_limits(result: &ResponseResult) -> bool {
    match result {
        ResponseResult::Handshake(value) => value.workspaces.len() <= MAX_COLLECTION_ITEMS,
        ResponseResult::WorkspaceSummary(_) => true,
        ResponseResult::HierarchyChildren(value) => value.children.len() <= MAX_COLLECTION_ITEMS,
        ResponseResult::WorkspaceHierarchy(value) => {
            value.repositories.len() <= MAX_COLLECTION_ITEMS
                && value.epics.len() <= MAX_COLLECTION_ITEMS
                && value.features.len() <= MAX_COLLECTION_ITEMS
                && value.work_items.len() <= MAX_COLLECTION_ITEMS
                && value.recent_entities.len() <= MAX_COLLECTION_ITEMS
        }
        ResponseResult::BoardViews(value) => value.len() <= MAX_COLLECTION_ITEMS,
        ResponseResult::BoardView(_) => true,
        ResponseResult::Board(value) => {
            value.lanes.len() <= MAX_COLLECTION_ITEMS && value.cards.len() <= MAX_COLLECTION_ITEMS
        }
        ResponseResult::Attention(value) => value.entries.len() <= MAX_COLLECTION_ITEMS,
        ResponseResult::ApprovalQueue(value) => value.entries.len() <= MAX_COLLECTION_ITEMS,
        ResponseResult::FeatureProposal(value) => {
            value.work_items.len() <= MAX_COLLECTION_ITEMS
                && value.repositories.len() <= MAX_COLLECTION_ITEMS
                && value.verification_gates.len() <= MAX_COLLECTION_ITEMS
                && value.warnings.len() <= MAX_DIAGNOSTICS
                && value.planner_sessions.len() <= MAX_COLLECTION_ITEMS
                && value.diagnostics.len() <= MAX_DIAGNOSTICS
        }
        ResponseResult::WorkItemDetail(value) => {
            value.blockers.len() <= MAX_COLLECTION_ITEMS
                && value.decisions.entries.len() <= MAX_COLLECTION_ITEMS
                && value.verification.entries.len() <= MAX_COLLECTION_ITEMS
                && value.repositories.len() <= MAX_COLLECTION_ITEMS
                && value.checkouts.len() <= MAX_COLLECTION_ITEMS
                && value.checkpoint_history.len() <= MAX_COLLECTION_ITEMS
                && value.sessions.len() <= MAX_COLLECTION_ITEMS
                && value.diagnostics.len() <= MAX_DIAGNOSTICS
        }
        ResponseResult::RepositoryObservability(value) => {
            value.display_paths.len() <= MAX_COLLECTION_ITEMS
                && value.checkout_ids.len() <= MAX_COLLECTION_ITEMS
        }
        ResponseResult::CheckoutObservability(value) => {
            value.display_paths.len() <= MAX_COLLECTION_ITEMS
                && value.bindings.len() <= MAX_COLLECTION_ITEMS
                && value.session_ids.len() <= MAX_COLLECTION_ITEMS
        }
        ResponseResult::SessionObservability(value) => value.diagnostics.len() <= MAX_DIAGNOSTICS,
        ResponseResult::RecoveryPreview(value) => value.conflicts.len() <= MAX_DIAGNOSTICS,
        ResponseResult::BoardSnapshot(value) => {
            value.repositories.len() <= MAX_COLLECTION_ITEMS
                && value.epics.len() <= MAX_COLLECTION_ITEMS
                && value.features.len() <= MAX_COLLECTION_ITEMS
                && value.work_items.len() <= MAX_COLLECTION_ITEMS
                && value.documents.len() <= MAX_COLLECTION_ITEMS
                && value.checkouts.len() <= MAX_COLLECTION_ITEMS
                && value.effective_checkouts.len() <= MAX_COLLECTION_ITEMS
                && value.sessions.len() <= MAX_COLLECTION_ITEMS
                && value.associations.len() <= MAX_COLLECTION_ITEMS
        }
        ResponseResult::SubscriptionAccepted { .. } | ResponseResult::CommandAccepted { .. } => {
            true
        }
    }
}

struct CommandContext {
    workspace_id: WorkspaceId,
    expected_revision: u64,
    idempotency_key: String,
    request_id: RequestId,
}

enum CommandFailure {
    Application(AppError),
    Unavailable(UnavailableReason),
}

fn dispatch_command(
    application: &mut WorkboardApplication,
    context: &CommandContext,
    command: &CommandOperation,
) -> Result<ResponseResult, CommandFailure> {
    match command {
        CommandOperation::SaveBoardView { definition } => application
            .save_client_board_view(
                core_workspace_id(context.workspace_id),
                context.expected_revision,
                &context.idempotency_key,
                context.request_id,
                definition.clone(),
            )
            .map(ResponseResult::BoardView)
            .map_err(CommandFailure::Application),
        CommandOperation::ApproveFeature { feature_id } => application
            .approve_client_feature(
                core_workspace_id(context.workspace_id),
                context.expected_revision,
                &context.idempotency_key,
                context.request_id,
                workboard_core::FeatureId::from_uuid(*feature_id.as_uuid()),
            )
            .map(proposal_result)
            .map_err(CommandFailure::Application),
        CommandOperation::RequestFeatureRevision {
            feature_id,
            feedback,
        } => application
            .request_client_feature_revision(
                core_workspace_id(context.workspace_id),
                context.expected_revision,
                &context.idempotency_key,
                context.request_id,
                workboard_core::FeatureId::from_uuid(*feature_id.as_uuid()),
                feedback,
            )
            .map(proposal_result)
            .map_err(CommandFailure::Application),
        CommandOperation::StartSession {
            work_item_id,
            repository_id,
            provider,
        } => application
            .start_client_session(workboard_application::projection::StartClientSession {
                workspace_id: core_workspace_id(context.workspace_id),
                expected_revision: context.expected_revision,
                idempotency_key: context.idempotency_key.clone(),
                request_id: context.request_id,
                work_item_id: workboard_core::WorkItemId::from_uuid(*work_item_id.as_uuid()),
                repository_id: repository_id
                    .map(|id| workboard_core::RepositoryId::from_uuid(*id.as_uuid())),
                tool: core_tool(*provider),
            })
            .map(session_result)
            .map_err(CommandFailure::Application),
        CommandOperation::ResumeSession { session_id } => application
            .resume_client_session(
                core_workspace_id(context.workspace_id),
                context.expected_revision,
                &context.idempotency_key,
                context.request_id,
                workboard_core::ConversationId::from_uuid(*session_id.as_uuid()),
            )
            .map(session_result)
            .map_err(CommandFailure::Application),
        CommandOperation::RejectFeature { .. }
        | CommandOperation::CheckpointWorkItem { .. }
        | CommandOperation::FocusSession { .. }
        | CommandOperation::FollowUpSession { .. }
        | CommandOperation::RecoverSession { .. } => Err(CommandFailure::Unavailable(
            command_unavailable_reason(command.code()).unwrap_or_else(accepted_capability_reason),
        )),
    }
}

fn core_tool(provider: workboard_client_protocol::Provider) -> Tool {
    match provider {
        workboard_client_protocol::Provider::Claude => Tool::Claude,
        workboard_client_protocol::Provider::Codex => Tool::Codex,
    }
}

fn session_result(
    outcome: workboard_application::projection::SessionCommandOutcome,
) -> ResponseResult {
    ResponseResult::WorkItemDetail(outcome.detail)
}

fn proposal_result(
    outcome: workboard_application::projection::ProposalCommandOutcome,
) -> ResponseResult {
    ResponseResult::FeatureProposal(outcome.proposal)
}

fn accepted_capability_reason() -> UnavailableReason {
    UnavailableReason {
        code: "upstream_capability_not_accepted".to_owned(),
        message: "the authoritative Workboard operation has not been accepted".to_owned(),
    }
}

fn command_unavailable_reason(code: CommandCode) -> Option<UnavailableReason> {
    let (code, message) = match code {
        CommandCode::SaveBoardView => return None,
        CommandCode::ApproveFeature | CommandCode::RequestFeatureRevision => return None,
        CommandCode::RejectFeature => (
            "terminal_rejection_unavailable",
            "Rejecting a proposal outright is unavailable; request a revision instead.",
        ),
        CommandCode::CheckpointWorkItem => (
            "structured_checkpoint_unavailable",
            "Structured checkpoint editing is unavailable because the daemon has not accepted a revision-checked atomic structured checkpoint operation.",
        ),
        CommandCode::StartSession | CommandCode::ResumeSession => return None,
        CommandCode::FocusSession => (
            "session_focus_unavailable",
            "Focusing a running session is unavailable; Workboard cannot yet activate a terminal window.",
        ),
        CommandCode::FollowUpSession => (
            "session_follow_up_unavailable",
            "Sending a follow-up is unavailable; Workboard cannot yet deliver a prompt to a live session.",
        ),
        CommandCode::RecoverSession => (
            "session_recovery_unavailable",
            "Recovery is unavailable from Desktop; it must preview before executing.",
        ),
    };
    Some(UnavailableReason {
        code: code.to_owned(),
        message: message.to_owned(),
    })
}

fn command_capabilities() -> Vec<CommandCapability> {
    CommandCode::ALL
        .into_iter()
        .map(|code| {
            let unavailable_reason = command_unavailable_reason(code);
            CommandCapability {
                code,
                available: unavailable_reason.is_none(),
                compatible_versions: if unavailable_reason.is_none() {
                    vec![CURRENT_PROTOCOL_VERSION]
                } else {
                    Vec::new()
                },
                unavailable_reason,
            }
        })
        .collect()
}

fn available_actions(revision: u64) -> Vec<AvailableAction> {
    CommandCode::ALL
        .into_iter()
        .map(|code| {
            let unavailable_reason = command_unavailable_reason(code);
            AvailableAction {
                code,
                available: unavailable_reason.is_none(),
                unavailable_reason,
                expected_revision: Some(revision),
            }
        })
        .collect()
}

fn incompatible(request: &ClientRequestEnvelope) -> ClientResponseEnvelope {
    ClientResponseEnvelope::failure(
        CURRENT_PROTOCOL_VERSION,
        request,
        ProtocolError::new(
            "incompatible_protocol",
            "the requested Workboard protocol version is incompatible",
        ),
    )
}

fn application_failure(
    version: u32,
    request: &ClientRequestEnvelope,
    error: AppError,
) -> ClientResponseEnvelope {
    ClientResponseEnvelope::failure(version, request, protocol_application_error(error))
}

fn protocol_application_error(error: AppError) -> ProtocolError {
    let mut remote = ProtocolError::new(error.code(), error.to_string());
    remote.retryable = matches!(error.code(), "storage" | "storage_io" | "git_io");
    remote
}

pub(crate) enum WriterRequest {
    Legacy {
        command: WriteCommand,
        response: Sender<ResponseEnvelope>,
    },
    Protocol {
        request: Box<ClientRequestEnvelope>,
        daemon_instance_id: DaemonInstanceId,
        response: Sender<ClientResponseEnvelope>,
    },
    Replay {
        workspace_id: WorkspaceId,
        cursor: EventCursor,
        version: u32,
        daemon_instance_id: DaemonInstanceId,
        response: Sender<Result<ReplayResult, Box<ProtocolError>>>,
    },
    Revision {
        workspace_id: WorkspaceId,
        response: Sender<Result<u64, Box<ProtocolError>>>,
    },
}

pub struct DaemonServer {
    address: SocketAddr,
    token: String,
    daemon_instance_id: DaemonInstanceId,
    stopping: Arc<AtomicBool>,
    listener_thread: Option<JoinHandle<()>>,
    writer_sender: Option<Sender<WriterRequest>>,
    writer_thread: Option<JoinHandle<()>>,
    watcher_thread: Option<JoinHandle<()>>,
}

impl DaemonServer {
    pub fn start<H>(
        handler: H,
        address: SocketAddr,
        token: impl Into<String>,
    ) -> Result<Self, DaemonError>
    where
        H: CommandHandler,
    {
        if !address.ip().is_loopback() {
            return Err(DaemonError::NonLoopbackAddress(address));
        }
        let token = token.into();
        if token.is_empty() || token.chars().any(char::is_control) {
            return Err(DaemonError::InvalidToken);
        }
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let daemon_instance_id = DaemonInstanceId::generate();
        let stopping = Arc::new(AtomicBool::new(false));
        let (writer_sender, writer_receiver) = mpsc::channel();
        let writer_thread = thread::spawn(move || writer_loop(handler, writer_receiver));
        let listener_stopping = Arc::clone(&stopping);
        let listener_token = token.clone();
        let listener_writer = writer_sender.clone();
        let listener_thread = thread::spawn(move || {
            listener_loop(
                listener,
                listener_token,
                listener_writer,
                daemon_instance_id,
                listener_stopping,
            );
        });
        Ok(Self {
            address,
            token,
            daemon_instance_id,
            stopping,
            listener_thread: Some(listener_thread),
            writer_sender: Some(writer_sender),
            writer_thread: Some(writer_thread),
            watcher_thread: None,
        })
    }

    pub fn start_application(
        application: WorkboardApplication,
        address: SocketAddr,
        token: impl Into<String>,
    ) -> Result<Self, DaemonError> {
        Self::start(ApplicationCommandHandler::new(application), address, token)
    }

    pub fn enable_watcher(&mut self, watch: WatchConfig) -> Result<(), DaemonError> {
        if self.watcher_thread.is_some() {
            return Err(DaemonError::WatcherUnavailable);
        }
        let writer = self
            .writer_sender
            .as_ref()
            .ok_or(DaemonError::WatcherUnavailable)?
            .clone();
        let stopping = Arc::clone(&self.stopping);
        self.watcher_thread = Some(thread::spawn(move || {
            watcher::watch_loop(watch, writer, stopping);
        }));
        Ok(())
    }

    pub fn client(&self) -> DaemonClient {
        DaemonClient::new(self.address, &self.token)
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn daemon_instance_id(&self) -> DaemonInstanceId {
        self.daemon_instance_id
    }

    pub fn descriptor(&self) -> crate::endpoint::EndpointDescriptor {
        crate::endpoint::EndpointDescriptor {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            address: self.address,
            token: self.token.clone(),
        }
    }

    pub fn wait(mut self) -> Result<(), DaemonError> {
        self.join()
    }

    fn join(&mut self) -> Result<(), DaemonError> {
        if let Some(thread) = self.listener_thread.take() {
            thread.join().map_err(|_| DaemonError::ServerThreadFailed)?;
        }
        if let Some(thread) = self.watcher_thread.take() {
            thread.join().map_err(|_| DaemonError::ServerThreadFailed)?;
        }
        self.writer_sender.take();
        if let Some(thread) = self.writer_thread.take() {
            thread.join().map_err(|_| DaemonError::ServerThreadFailed)?;
        }
        Ok(())
    }
}

impl Drop for DaemonServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
        let _ = self.join();
    }
}

fn listener_loop(
    listener: TcpListener,
    token: String,
    writer: Sender<WriterRequest>,
    daemon_instance_id: DaemonInstanceId,
    stopping: Arc<AtomicBool>,
) {
    while !stopping.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let token = token.clone();
                let writer = writer.clone();
                let stopping = Arc::clone(&stopping);
                thread::spawn(move || {
                    handle_connection(stream, &token, &writer, daemon_instance_id, &stopping);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) if transient_accept_error(&error) => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn transient_accept_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    )
}

fn handle_connection(
    mut stream: TcpStream,
    token: &str,
    writer: &Sender<WriterRequest>,
    daemon_instance_id: DaemonInstanceId,
    stopping: &AtomicBool,
) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut first = [0_u8; 1];
    match stream.peek(&mut first) {
        Ok(1) if first[0] == b'{' => handle_legacy_connection(stream, token, writer),
        Ok(1) => handle_framed_connection(&mut stream, token, writer, daemon_instance_id, stopping),
        _ => {}
    }
}

fn handle_legacy_connection(mut stream: TcpStream, token: &str, writer: &Sender<WriterRequest>) {
    let response = read_legacy_request(&mut stream)
        .and_then(|request| validate_legacy_request(request, token))
        .and_then(|command| send_to_writer(command, writer))
        .unwrap_or_else(|error| error);
    if let Ok(body) = serde_json::to_vec(&response) {
        let _ = stream.write_all(&body);
        let _ = stream.shutdown(Shutdown::Write);
    }
}

fn handle_framed_connection(
    stream: &mut TcpStream,
    token: &str,
    writer: &Sender<WriterRequest>,
    daemon_instance_id: DaemonInstanceId,
    stopping: &AtomicBool,
) {
    let authenticated = match read_frame::<AuthenticatedRequest>(stream) {
        Ok(request) => request,
        Err(error) => {
            write_protocol_failure(stream, error.code, error.message);
            return;
        }
    };
    if authenticated.token != token {
        let response = ClientResponseEnvelope::failure(
            authenticated.request.protocol_version,
            &authenticated.request,
            ProtocolError::new("authentication_failed", "daemon authentication failed"),
        );
        let _ = write_frame(stream, &ServerMessage::Response(Box::new(response)));
        return;
    }
    if let Err(error) = authenticated.request.validate() {
        let response = ClientResponseEnvelope::failure(
            authenticated.request.protocol_version,
            &authenticated.request,
            *error,
        );
        let _ = write_frame(stream, &ServerMessage::Response(Box::new(response)));
        return;
    }
    let subscription = matches!(authenticated.request.operation, Operation::Subscribe(_));
    let response = match send_protocol(authenticated.request.clone(), daemon_instance_id, writer) {
        Ok(response) => response,
        Err(error) => ClientResponseEnvelope::failure(
            authenticated.request.protocol_version,
            &authenticated.request,
            *error,
        ),
    };
    let accepted_cursor = match &response.result {
        Some(ResponseResult::SubscriptionAccepted { cursor }) => Some(*cursor),
        _ => None,
    };
    if write_frame(stream, &ServerMessage::Response(Box::new(response))).is_err()
        || !subscription
        || accepted_cursor.is_none()
    {
        return;
    }
    run_subscription(
        stream,
        writer,
        authenticated
            .request
            .workspace_id
            .expect("validated Workspace"),
        accepted_cursor.expect("accepted cursor"),
        authenticated.request.protocol_version,
        daemon_instance_id,
        stopping,
    );
}

fn run_subscription(
    stream: &mut TcpStream,
    writer: &Sender<WriterRequest>,
    workspace_id: WorkspaceId,
    mut cursor: EventCursor,
    version: u32,
    daemon_instance_id: DaemonInstanceId,
    stopping: &AtomicBool,
) {
    let mut heartbeat_at = Instant::now() + HEARTBEAT_INTERVAL;
    while !stopping.load(Ordering::Acquire) {
        match replay(writer, workspace_id, cursor, version, daemon_instance_id) {
            Ok(ReplayResult::Events(events)) => {
                for event in events {
                    cursor.sequence = event.sequence;
                    if write_frame(stream, &ServerMessage::Event(Box::new(event))).is_err() {
                        return;
                    }
                }
            }
            Ok(ReplayResult::Resync(requirement)) => {
                let _ = write_frame(stream, &ServerMessage::ResyncRequired(requirement));
                return;
            }
            Err(error) => {
                let request = synthetic_request(version, Some(workspace_id));
                let response = ClientResponseEnvelope::failure(version, &request, *error);
                let _ = write_frame(stream, &ServerMessage::Response(Box::new(response)));
                return;
            }
        }
        if Instant::now() >= heartbeat_at {
            let revision = match revision(writer, workspace_id) {
                Ok(revision) => revision,
                Err(_) => return,
            };
            let heartbeat = Heartbeat {
                daemon_instance_id,
                workspace_id,
                revision,
                sent_at: OffsetDateTime::now_utc(),
            };
            if write_frame(stream, &ServerMessage::Heartbeat(heartbeat)).is_err() {
                return;
            }
            heartbeat_at = Instant::now() + HEARTBEAT_INTERVAL;
        }
        thread::sleep(SUBSCRIPTION_POLL_INTERVAL);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatedRequest {
    token: String,
    request: ClientRequestEnvelope,
}

struct FrameFailure {
    code: &'static str,
    message: String,
}

fn read_frame<T: serde::de::DeserializeOwned>(stream: &mut impl Read) -> Result<T, FrameFailure> {
    let mut length = [0_u8; 4];
    stream
        .read_exact(&mut length)
        .map_err(|error| FrameFailure {
            code: "protocol_io",
            message: error.to_string(),
        })?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 {
        return Err(FrameFailure {
            code: "invalid_frame",
            message: "daemon request frame is empty".to_owned(),
        });
    }
    if length > MAX_FRAME_BYTES {
        return Err(FrameFailure {
            code: "message_too_large",
            message: "daemon request exceeds the frame bound".to_owned(),
        });
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(|error| FrameFailure {
        code: "protocol_io",
        message: error.to_string(),
    })?;
    serde_json::from_slice(&body).map_err(|error| FrameFailure {
        code: "invalid_request",
        message: error.to_string(),
    })
}

fn write_frame<T: serde::Serialize>(
    stream: &mut impl Write,
    value: &T,
) -> Result<(), std::io::Error> {
    let body = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "daemon response exceeds the frame bound",
        ));
    }
    let length = u32::try_from(body.len()).map_err(std::io::Error::other)?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

fn write_protocol_failure(stream: &mut impl Write, code: &str, message: String) {
    let request = synthetic_request(CURRENT_PROTOCOL_VERSION, None);
    let response = ClientResponseEnvelope::failure(
        CURRENT_PROTOCOL_VERSION,
        &request,
        ProtocolError::new(code, message),
    );
    let _ = write_frame(stream, &ServerMessage::Response(Box::new(response)));
}

fn synthetic_request(version: u32, workspace_id: Option<WorkspaceId>) -> ClientRequestEnvelope {
    ClientRequestEnvelope {
        protocol_version: version,
        request_id: RequestId::generate(),
        workspace_id,
        expected_revision: None,
        idempotency_key: None,
        operation: Operation::Handshake(workboard_client_protocol::HandshakeRequest {
            supported_read_versions: vec![CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION],
            supported_command_versions: Vec::new(),
        }),
    }
}

fn read_legacy_request(stream: &mut TcpStream) -> Result<RequestEnvelope, ResponseEnvelope> {
    let mut body = Vec::new();
    stream
        .take(MAX_MESSAGE_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| ResponseEnvelope::failure("protocol_io", error.to_string()))?;
    if body.len() as u64 > MAX_MESSAGE_BYTES {
        return Err(ResponseEnvelope::failure(
            "message_too_large",
            "daemon request exceeds the message bound",
        ));
    }
    serde_json::from_slice(&body)
        .map_err(|error| ResponseEnvelope::failure("invalid_request", error.to_string()))
}

fn validate_legacy_request(
    request: RequestEnvelope,
    token: &str,
) -> Result<WriteCommand, ResponseEnvelope> {
    if request.protocol_version != LEGACY_PROTOCOL_VERSION {
        return Err(ResponseEnvelope::failure(
            "unsupported_protocol",
            format!(
                "protocol version {} is unsupported",
                request.protocol_version
            ),
        ));
    }
    if request.token != token {
        return Err(ResponseEnvelope::failure(
            "authentication_failed",
            "daemon authentication failed",
        ));
    }
    Ok(request.command)
}

pub(crate) fn send_to_writer(
    command: WriteCommand,
    writer: &Sender<WriterRequest>,
) -> Result<ResponseEnvelope, ResponseEnvelope> {
    let (response_sender, response_receiver) = mpsc::channel();
    writer
        .send(WriterRequest::Legacy {
            command,
            response: response_sender,
        })
        .map_err(|_| ResponseEnvelope::failure("writer_stopped", "daemon writer stopped"))?;
    response_receiver
        .recv()
        .map_err(|_| ResponseEnvelope::failure("writer_stopped", "daemon writer stopped"))
}

fn send_protocol(
    request: ClientRequestEnvelope,
    daemon_instance_id: DaemonInstanceId,
    writer: &Sender<WriterRequest>,
) -> Result<ClientResponseEnvelope, Box<ProtocolError>> {
    let (response_sender, response_receiver) = mpsc::channel();
    writer
        .send(WriterRequest::Protocol {
            request: Box::new(request),
            daemon_instance_id,
            response: response_sender,
        })
        .map_err(|_| {
            Box::new(ProtocolError::new(
                "writer_stopped",
                "daemon writer stopped",
            ))
        })?;
    response_receiver.recv().map_err(|_| {
        Box::new(ProtocolError::new(
            "writer_stopped",
            "daemon writer stopped",
        ))
    })
}

fn replay(
    writer: &Sender<WriterRequest>,
    workspace_id: WorkspaceId,
    cursor: EventCursor,
    version: u32,
    daemon_instance_id: DaemonInstanceId,
) -> Result<ReplayResult, Box<ProtocolError>> {
    let (response_sender, response_receiver) = mpsc::channel();
    writer
        .send(WriterRequest::Replay {
            workspace_id,
            cursor,
            version,
            daemon_instance_id,
            response: response_sender,
        })
        .map_err(|_| {
            Box::new(ProtocolError::new(
                "writer_stopped",
                "daemon writer stopped",
            ))
        })?;
    response_receiver.recv().map_err(|_| {
        Box::new(ProtocolError::new(
            "writer_stopped",
            "daemon writer stopped",
        ))
    })?
}

fn revision(
    writer: &Sender<WriterRequest>,
    workspace_id: WorkspaceId,
) -> Result<u64, Box<ProtocolError>> {
    let (response_sender, response_receiver) = mpsc::channel();
    writer
        .send(WriterRequest::Revision {
            workspace_id,
            response: response_sender,
        })
        .map_err(|_| {
            Box::new(ProtocolError::new(
                "writer_stopped",
                "daemon writer stopped",
            ))
        })?;
    response_receiver.recv().map_err(|_| {
        Box::new(ProtocolError::new(
            "writer_stopped",
            "daemon writer stopped",
        ))
    })?
}

fn writer_loop<H>(mut handler: H, requests: Receiver<WriterRequest>)
where
    H: CommandHandler,
{
    for request in requests {
        match request {
            WriterRequest::Legacy { command, response } => {
                let envelope = match command {
                    WriteCommand::Ping => ResponseEnvelope::success(json!({ "status": "ready" })),
                    command => handler
                        .handle(command)
                        .map(ResponseEnvelope::success)
                        .unwrap_or_else(|error| {
                            ResponseEnvelope::failure(error.code, error.message)
                        }),
                };
                let _ = response.send(envelope);
            }
            WriterRequest::Protocol {
                request,
                daemon_instance_id,
                response,
            } => {
                let result = handler.handle_protocol(&request, daemon_instance_id);
                let _ = response.send(result);
            }
            WriterRequest::Replay {
                workspace_id,
                cursor,
                version,
                daemon_instance_id,
                response,
            } => {
                let result = handler.replay(workspace_id, cursor, version, daemon_instance_id);
                let _ = response.send(result);
            }
            WriterRequest::Revision {
                workspace_id,
                response,
            } => {
                let result = handler.revision(workspace_id);
                let _ = response.send(result);
            }
        }
    }
}
