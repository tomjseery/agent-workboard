use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use time::OffsetDateTime;
use workboard_client::framing::{read_frame, write_frame};
use workboard_client::{EndpointDescriptor, endpoint_path};
use workboard_client_protocol::{
    BoardSnapshot, BoardViewDefinition, BoardViewDensity, BoardViewFilters, BoardViewGrouping,
    BoardViewGroupingKind, BoardViewSort, BoardViewSortDirection, BoardViewSortField,
    CURRENT_PROTOCOL_VERSION, CheckoutAvailability, CheckoutObservabilityProjection,
    CheckoutPurpose, CheckoutPurposeSource, ClassifiedEvidence, CommandCapability, CommandCode,
    DaemonInstanceId, EntityRef, EventCursor, EventEnvelope, EventId, EventKind, EventPayload,
    EvidenceState, HandshakeResponse, Heartbeat, HierarchyChildren, ManagedSessionRole, Operation,
    OwnerProjection, PartialOutcome, PrimaryWriterEvidence, Provider, ReadQuery,
    RecoveryDispositionProjection, RecoveryPreviewProjection, RepositoryObservabilityProjection,
    RepositoryReference, RequestEnvelope, ResponseEnvelope, ResponseResult, ResyncReason,
    ResyncRequirement, ServerMessage, SessionBindingState, SessionLiveState,
    SessionLivenessProjection, SessionObservabilityProjection, SessionRestoreState,
    SessionResumability, UnavailableReason, WorkItemId, WorkspaceHierarchy, WorkspaceId,
    WorkspaceProjection, WorkspaceReference, WorkspaceSummary,
};

fn not_loaded_evidence() -> ClassifiedEvidence {
    ClassifiedEvidence {
        state: EvidenceState::NotLoaded,
        code: "not_loaded".to_owned(),
        message: "Evidence is not loaded in this deterministic daemon fixture.".to_owned(),
        observed_at: None,
    }
}

fn test_work_item_id() -> WorkItemId {
    "60000000-0000-0000-0000-000000000001"
        .parse()
        .expect("test Work-item ID")
}

fn test_work_item_detail() -> workboard_client_protocol::WorkItemDetailProjection {
    let fixture =
        workboard_client_protocol::generation::conformance_fixture(CURRENT_PROTOCOL_VERSION);
    let response = fixture["responses"]
        .as_array()
        .expect("fixture responses")
        .iter()
        .find(|response| response["result"]["type"] == "work_item_detail")
        .expect("Work-item detail response");
    let response: ResponseEnvelope =
        serde_json::from_value(response.clone()).expect("Work-item detail envelope");
    match response.result.expect("Work-item detail result") {
        ResponseResult::WorkItemDetail(detail) => *detail,
        _ => panic!("unexpected Work-item detail fixture"),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatedRequest {
    token: String,
    request: RequestEnvelope,
}

pub struct FakeDaemon {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    events: Arc<Mutex<Vec<EventEnvelope>>>,
    pub active_subscriptions: Arc<AtomicUsize>,
    pub max_active_subscriptions: Arc<AtomicUsize>,
    pub accepted_subscriptions: Arc<AtomicUsize>,
    pub forwarded_queries: Arc<AtomicUsize>,
    pub workspace_id: WorkspaceId,
    pub token: String,
}

#[derive(Default)]
pub struct FakeDaemonOptions {
    pub ready: bool,
    pub expose_token_in_handshake: bool,
    pub workspace_id: Option<WorkspaceId>,
}

impl FakeDaemon {
    pub fn start(database: &Path, options: FakeDaemonOptions) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake daemon listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking fake daemon");
        let address = listener.local_addr().expect("fake daemon address");
        let workspace_id = options.workspace_id.unwrap_or_else(WorkspaceId::generate);
        let daemon_instance_id = DaemonInstanceId::generate();
        let token = format!("{}-{}", EventId::generate(), EventId::generate());
        fs::write(
            endpoint_path(database),
            serde_json::to_vec(&EndpointDescriptor {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                address,
                token: token.clone(),
            })
            .expect("endpoint descriptor"),
        )
        .expect("write endpoint descriptor");
        let stop = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(Vec::new()));
        let active_subscriptions = Arc::new(AtomicUsize::new(0));
        let max_active_subscriptions = Arc::new(AtomicUsize::new(0));
        let accepted_subscriptions = Arc::new(AtomicUsize::new(0));
        let forwarded_queries = Arc::new(AtomicUsize::new(0));
        let server = FakeServer {
            token: token.clone(),
            workspace_id,
            daemon_instance_id,
            ready: options.ready,
            workspace_title: if options.expose_token_in_handshake {
                token.clone()
            } else {
                "Test Workspace".to_owned()
            },
            stop: Arc::clone(&stop),
            events: Arc::clone(&events),
            active_subscriptions: Arc::clone(&active_subscriptions),
            max_active_subscriptions: Arc::clone(&max_active_subscriptions),
            accepted_subscriptions: Arc::clone(&accepted_subscriptions),
            forwarded_queries: Arc::clone(&forwarded_queries),
        };
        let thread = thread::spawn(move || server.run(listener));
        Self {
            stop,
            thread: Some(thread),
            events,
            active_subscriptions,
            max_active_subscriptions,
            accepted_subscriptions,
            forwarded_queries,
            workspace_id,
            token,
        }
    }

    pub fn push_event(&self, sequence: u64, message: Option<String>) {
        let partial_outcomes = message
            .map(|message| {
                vec![PartialOutcome {
                    owner: Some(EntityRef::Workspace(self.workspace_id)),
                    code: "test_outcome".to_owned(),
                    succeeded: false,
                    message,
                    reconciliation_required: false,
                    evidence: Vec::new(),
                }]
            })
            .unwrap_or_default();
        self.events
            .lock()
            .expect("fake events")
            .push(EventEnvelope {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                event_version: 1,
                workspace_id: self.workspace_id,
                sequence,
                event_id: EventId::generate(),
                occurred_at: OffsetDateTime::now_utc(),
                owner: EntityRef::Workspace(self.workspace_id),
                entity_revision: sequence,
                kind: EventKind::ProjectionChanged,
                payload: Some(EventPayload::ProjectionChanged {
                    entity: EntityRef::Workspace(self.workspace_id),
                }),
                invalidation_scope: None,
                operation_correlation_id: workboard_client_protocol::RequestId::generate(),
                partial_outcomes,
            });
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("fake daemon thread");
        }
        wait_until(Duration::from_secs(2), || {
            self.active_subscriptions.load(Ordering::Acquire) == 0
        });
    }
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
struct FakeServer {
    token: String,
    workspace_id: WorkspaceId,
    daemon_instance_id: DaemonInstanceId,
    ready: bool,
    workspace_title: String,
    stop: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<EventEnvelope>>>,
    active_subscriptions: Arc<AtomicUsize>,
    max_active_subscriptions: Arc<AtomicUsize>,
    accepted_subscriptions: Arc<AtomicUsize>,
    forwarded_queries: Arc<AtomicUsize>,
}

impl FakeServer {
    fn run(self, listener: TcpListener) {
        while !self.stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let server = self.clone();
                    thread::spawn(move || server.handle(stream));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    }

    fn handle(&self, mut stream: TcpStream) {
        let Ok(authenticated) = read_frame::<AuthenticatedRequest>(&mut stream) else {
            return;
        };
        if authenticated.token != self.token {
            return;
        }
        match &authenticated.request.operation {
            Operation::Handshake(_) => self.handshake(&mut stream, &authenticated.request),
            Operation::Query(query) => {
                self.forwarded_queries.fetch_add(1, Ordering::Relaxed);
                self.query(&mut stream, &authenticated.request, query);
            }
            Operation::Command(_) => {
                write_message(
                    &mut stream,
                    ServerMessage::Response(Box::new(ResponseEnvelope::failure(
                        CURRENT_PROTOCOL_VERSION,
                        &authenticated.request,
                        workboard_client_protocol::ProtocolError::new(
                            "capability_unavailable",
                            "The capability is unavailable.",
                        ),
                    ))),
                );
            }
            Operation::Subscribe(subscription) => {
                self.subscription(&mut stream, &authenticated.request, subscription.cursor);
            }
        }
    }

    fn handshake(&self, stream: &mut TcpStream, request: &RequestEnvelope) {
        let capability = CommandCapability {
            code: CommandCode::SaveBoardView,
            available: self.ready,
            compatible_versions: vec![CURRENT_PROTOCOL_VERSION],
            unavailable_reason: (!self.ready).then(|| UnavailableReason {
                code: "not_accepted".to_owned(),
                message: "The capability is not accepted.".to_owned(),
            }),
        };
        let response = ResponseEnvelope::success(
            CURRENT_PROTOCOL_VERSION,
            request,
            None,
            ResponseResult::Handshake(HandshakeResponse {
                daemon_instance_id: self.daemon_instance_id,
                negotiated_read_version: CURRENT_PROTOCOL_VERSION,
                compatible_command_versions: vec![CURRENT_PROTOCOL_VERSION],
                workspaces: vec![WorkspaceReference {
                    id: self.workspace_id,
                    slug: "test-workspace".to_owned(),
                    title: self.workspace_title.clone(),
                }],
                command_capabilities: vec![capability],
                event_version: 1,
                heartbeat_interval_ms: 100,
                max_frame_bytes: workboard_client_protocol::MAX_FRAME_BYTES,
            }),
            Vec::new(),
        );
        write_message(stream, ServerMessage::Response(Box::new(response)));
    }

    fn query(&self, stream: &mut TcpStream, request: &RequestEnvelope, query: &ReadQuery) {
        let result = match query {
            ReadQuery::WorkspaceSummary => ResponseResult::WorkspaceSummary(WorkspaceSummary {
                workspace: self.workspace_reference(),
                repository_count: 0,
                epic_count: 0,
                feature_count: 0,
                work_item_count: 0,
                session_count: 0,
            }),
            ReadQuery::HierarchyChildren { parent } => {
                ResponseResult::HierarchyChildren(HierarchyChildren {
                    parent: *parent,
                    children: Vec::new(),
                })
            }
            ReadQuery::BoardSnapshot => ResponseResult::BoardSnapshot(self.snapshot()),
            ReadQuery::WorkspaceHierarchy => {
                ResponseResult::WorkspaceHierarchy(WorkspaceHierarchy {
                    workspace: self.workspace_reference(),
                    repositories: Vec::new(),
                    epics: Vec::new(),
                    features: Vec::new(),
                    work_items: Vec::new(),
                    recent_entities: Vec::new(),
                    focused_entity: None,
                })
            }
            ReadQuery::BoardViews => ResponseResult::BoardViews(Vec::new()),
            ReadQuery::BoardView { view_id } => ResponseResult::BoardView(BoardViewDefinition {
                id: *view_id,
                workspace_id: self.workspace_id,
                title: "Test view".to_owned(),
                filters: BoardViewFilters {
                    query: None,
                    repository_ids: Vec::new(),
                    statuses: Vec::new(),
                },
                grouping: BoardViewGrouping {
                    kind: BoardViewGroupingKind::Hierarchy,
                    lanes: Vec::new(),
                },
                sort: BoardViewSort {
                    field: BoardViewSortField::Title,
                    direction: BoardViewSortDirection::Ascending,
                },
                density: BoardViewDensity::Comfortable,
                revision: 1,
            }),
            ReadQuery::Board { .. } => {
                ResponseResult::Board(workboard_client_protocol::BoardPage {
                    lanes: Vec::new(),
                    cards: Vec::new(),
                    next_cursor: None,
                    total_count: 0,
                    revision: 0,
                })
            }
            ReadQuery::Attention { .. } => {
                ResponseResult::Attention(workboard_client_protocol::AttentionPage {
                    entries: Vec::new(),
                    next_cursor: None,
                    total_count: 0,
                    revision: 0,
                })
            }
            ReadQuery::ApprovalQueue => {
                ResponseResult::ApprovalQueue(workboard_client_protocol::ApprovalQueueProjection {
                    entries: Vec::new(),
                    revision: self.revision(),
                })
            }
            ReadQuery::FeatureProposal { feature_id } => ResponseResult::FeatureProposal(
                workboard_client_protocol::FeatureProposalProjection {
                    feature: workboard_client_protocol::FeatureReference {
                        id: *feature_id,
                        epic_id: "40000000-0000-0000-0000-000000000001"
                            .parse()
                            .expect("test Epic ID"),
                        slug: "test-proposal".to_owned(),
                        title: "Test proposal".to_owned(),
                    },
                    generation: 1,
                    revision: self.revision(),
                    proposal_hash: "a".repeat(64),
                    submitted_at: "2026-08-31T12:00:00Z".to_owned(),
                    changed_since_previous: false,
                    feature_body: "Test proposal body".to_owned(),
                    work_items: Vec::new(),
                    repositories: Vec::new(),
                    verification_gates: Vec::new(),
                    warnings: Vec::new(),
                    planner_sessions: Vec::new(),
                    diagnostics: Vec::new(),
                    workflow_state: workboard_client_protocol::WorkflowState::AwaitingApproval,
                    available_actions: Vec::new(),
                },
            ),
            ReadQuery::WorkItemDetail { work_item_id } => {
                let mut detail = test_work_item_detail();
                detail.work_item.id = *work_item_id;
                ResponseResult::WorkItemDetail(Box::new(detail))
            }
            ReadQuery::RepositoryObservability { repository_id } => {
                ResponseResult::RepositoryObservability(RepositoryObservabilityProjection {
                    repository: RepositoryReference {
                        id: *repository_id,
                        workspace_id: self.workspace_id,
                        slug: "test-repository".to_owned(),
                        title: "Test repository".to_owned(),
                    },
                    display_paths: Vec::new(),
                    remote_names: Vec::new(),
                    default_branch: None,
                    remote_evidence: not_loaded_evidence(),
                    default_branch_evidence: not_loaded_evidence(),
                    checkout_ids: Vec::new(),
                    revision: self.revision(),
                    diagnostics: Vec::new(),
                })
            }
            ReadQuery::CheckoutObservability { checkout_id } => {
                ResponseResult::CheckoutObservability(CheckoutObservabilityProjection {
                    id: *checkout_id,
                    repository: self.test_repository(),
                    purpose: CheckoutPurpose::Unknown,
                    purpose_source: CheckoutPurposeSource::Unknown,
                    branch: None,
                    head: None,
                    isolation_generation: None,
                    reconciliation_generation: None,
                    availability: CheckoutAvailability::Missing,
                    display_paths: Vec::new(),
                    replaces_checkout_id: None,
                    replaced_by_checkout_id: None,
                    bindings: Vec::new(),
                    session_ids: Vec::new(),
                    dirty_evidence: not_loaded_evidence(),
                    collision_evidence: not_loaded_evidence(),
                    reconciliation_evidence: not_loaded_evidence(),
                    revision: self.revision(),
                    diagnostics: Vec::new(),
                })
            }
            ReadQuery::SessionObservability { session_id } => {
                ResponseResult::SessionObservability(SessionObservabilityProjection {
                    id: *session_id,
                    provider: Provider::Codex,
                    role: ManagedSessionRole::WorkItemExecution,
                    owner: OwnerProjection::WorkItem(test_work_item_id()),
                    authoritative_profile: None,
                    authoritative_model: None,
                    profile_evidence: not_loaded_evidence(),
                    binding_state: SessionBindingState::Pending,
                    liveness: SessionLivenessProjection {
                        state: SessionLiveState::NotLoaded,
                        stale: false,
                        observed_at: None,
                        expires_at: None,
                        evidence: not_loaded_evidence(),
                    },
                    restore_state: SessionRestoreState::NotTracked,
                    last_activity_at: None,
                    checkout_id: None,
                    resumability: SessionResumability::Unknown,
                    primary_writer: PrimaryWriterEvidence::Unknown,
                    revision: self.revision(),
                    diagnostics: Vec::new(),
                })
            }
            ReadQuery::RecoveryPreview { session_id } => {
                ResponseResult::RecoveryPreview(RecoveryPreviewProjection {
                    session_id: *session_id,
                    disposition: RecoveryDispositionProjection::NotLoaded,
                    conflicts: Vec::new(),
                    observed_at: "2026-08-31T12:00:00Z".to_owned(),
                    stale: false,
                    revision: self.revision(),
                })
            }
        };
        write_message(
            stream,
            ServerMessage::Response(Box::new(ResponseEnvelope::success(
                CURRENT_PROTOCOL_VERSION,
                request,
                Some(self.revision()),
                result,
                Vec::new(),
            ))),
        );
    }

    fn test_repository(&self) -> RepositoryReference {
        RepositoryReference {
            id: "30000000-0000-0000-0000-000000000001"
                .parse()
                .expect("test repository ID"),
            workspace_id: self.workspace_id,
            slug: "test-repository".to_owned(),
            title: "Test repository".to_owned(),
        }
    }

    fn subscription(
        &self,
        stream: &mut TcpStream,
        request: &RequestEnvelope,
        requested_cursor: Option<EventCursor>,
    ) {
        self.accepted_subscriptions.fetch_add(1, Ordering::Relaxed);
        let cursor = requested_cursor.unwrap_or(EventCursor {
            daemon_instance_id: self.daemon_instance_id,
            sequence: self.revision(),
        });
        write_message(
            stream,
            ServerMessage::Response(Box::new(ResponseEnvelope::success(
                CURRENT_PROTOCOL_VERSION,
                request,
                Some(self.revision()),
                ResponseResult::SubscriptionAccepted { cursor },
                Vec::new(),
            ))),
        );
        self.active_subscriptions.fetch_add(1, Ordering::AcqRel);
        self.max_active_subscriptions.fetch_max(
            self.active_subscriptions.load(Ordering::Acquire),
            Ordering::AcqRel,
        );
        if cursor.daemon_instance_id != self.daemon_instance_id {
            write_message(
                stream,
                ServerMessage::ResyncRequired(ResyncRequirement {
                    reason: ResyncReason::DaemonRestarted,
                    workspace_id: self.workspace_id,
                    authoritative_revision: self.revision(),
                    oldest_replayable_sequence: 0,
                    required_queries: vec![workboard_client_protocol::ReadQueryCode::BoardSnapshot],
                }),
            );
            self.active_subscriptions.fetch_sub(1, Ordering::AcqRel);
            return;
        }
        let mut sequence = cursor.sequence;
        let mut heartbeat_at = Instant::now();
        while !self.stop.load(Ordering::Acquire) {
            for event in self.events.lock().expect("fake events").clone() {
                if event.sequence > sequence {
                    sequence = event.sequence;
                    if write_frame(stream, &ServerMessage::Event(Box::new(event))).is_err() {
                        self.active_subscriptions.fetch_sub(1, Ordering::AcqRel);
                        return;
                    }
                }
            }
            if heartbeat_at.elapsed() >= Duration::from_millis(100) {
                if write_frame(
                    stream,
                    &ServerMessage::Heartbeat(Heartbeat {
                        daemon_instance_id: self.daemon_instance_id,
                        workspace_id: self.workspace_id,
                        revision: self.revision(),
                        sent_at: OffsetDateTime::now_utc(),
                    }),
                )
                .is_err()
                {
                    self.active_subscriptions.fetch_sub(1, Ordering::AcqRel);
                    return;
                }
                heartbeat_at = Instant::now();
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.active_subscriptions.fetch_sub(1, Ordering::AcqRel);
    }

    fn revision(&self) -> u64 {
        self.events
            .lock()
            .expect("fake events")
            .last()
            .map_or(0, |event| event.sequence)
    }

    fn workspace_reference(&self) -> WorkspaceReference {
        WorkspaceReference {
            id: self.workspace_id,
            slug: "test-workspace".to_owned(),
            title: self.workspace_title.clone(),
        }
    }

    fn snapshot(&self) -> BoardSnapshot {
        BoardSnapshot {
            workspace: WorkspaceProjection {
                id: self.workspace_id,
                slug: "test-workspace".to_owned(),
                title: self.workspace_title.clone(),
                planning_store_repository_id: workboard_client_protocol::RepositoryId::generate(),
            },
            repositories: Vec::new(),
            epics: Vec::new(),
            features: Vec::new(),
            work_items: Vec::new(),
            documents: Vec::new(),
            checkouts: Vec::new(),
            effective_checkouts: Vec::new(),
            sessions: Vec::new(),
            associations: Vec::new(),
        }
    }
}

fn write_message(stream: &mut TcpStream, message: ServerMessage) {
    write_frame(stream, &message).expect("write fake daemon message");
}

pub fn wait_until(timeout: Duration, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout;
    while !condition() {
        assert!(Instant::now() < deadline, "condition timed out");
        thread::sleep(Duration::from_millis(10));
    }
}
