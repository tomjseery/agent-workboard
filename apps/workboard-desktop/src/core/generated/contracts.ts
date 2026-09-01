export type ApprovalQueueItemProjection = { feature: FeatureReference, generation: number, revision: number, proposalHash: string, submittedAt: string, changedSincePrevious: boolean, workflowState: WorkflowState, repositories: Array<RepositoryReference>, warningCount: number, plannerCount: number, availableActions: Array<AvailableAction>, position: number, totalCount: number, };

export type ApprovalQueueProjection = { entries: Array<ApprovalQueueItemProjection>, revision: number, };

export type AssociationId = string;

export type AttentionEntryProjection = { owner: EntityRef, title: string, subtitle: string, repositories: Array<RepositoryReference>, card: BoardCardProjection | null, reasons: Array<AttentionReason>, revision: number, availableActions: Array<AvailableAction>, position: number, totalCount: number, };

export type AttentionPage = { entries: Array<AttentionEntryProjection>, nextCursor: string | null, totalCount: number, revision: number, };

export type AttentionQuery = { cursor: string | null, limit: number, repositoryIds: Array<RepositoryId>, reasonCodes: Array<AttentionReasonCode>, };

export type AttentionReason = { code: AttentionReasonCode, rank: number, message: string, };

export type AttentionReasonCode = "approval_required" | "revision_requested" | "reconciliation_required" | "blocked" | "checkpoint_due" | "interrupted_operation" | "recovery_conflict" | "stale_or_unknown_session";

export type AvailableAction = { code: CommandCode, available: boolean, unavailableReason: UnavailableReason | null, expectedRevision: number | null, };

export type BlockedByEvidence = { workItem: WorkItemReference, status: WorkItemStatus, };

export type BoardCardProjection = { workItem: WorkItemReference, feature: FeatureReference, status: WorkItemStatus, laneKey: string, lanePosition: number, laneCount: number, dependencyReadiness: DependencyReadiness, blockedBy: Array<BlockedByEvidence>, parallelReadiness: ParallelReadiness, repositories: Array<RepositoryReference>, sessionSummary: SessionSummary, checkoutIds: Array<CheckoutId>, sessionIds: Array<SessionId>, attentionReasons: Array<AttentionReason>, revision: number, availableActions: Array<AvailableAction>, };

export type BoardLaneProjection = { key: string, title: string, position: number, totalCount: number, };

export type BoardPage = { lanes: Array<BoardLaneProjection>, cards: Array<BoardCardProjection>, nextCursor: string | null, totalCount: number, revision: number, };

export type BoardQuery = { cursor: string | null, limit: number, query: string | null, repositoryIds: Array<RepositoryId>, statuses: Array<WorkItemStatus>, laneKeys: Array<string>, sort: BoardViewSort, };

export type BoardViewDefinition = { id: BoardViewId, workspaceId: WorkspaceId, title: string, filters: BoardViewFilters, grouping: BoardViewGrouping, sort: BoardViewSort, density: BoardViewDensity, revision: number, };

export type BoardViewDensity = "comfortable" | "compact";

export type BoardViewFilters = { query: string | null, repositoryIds: Array<RepositoryId>, statuses: Array<WorkItemStatus>, };

export type BoardViewGrouping = { kind: BoardViewGroupingKind, lanes: Array<BoardViewLaneDefinition>, };

export type BoardViewGroupingKind = "hierarchy" | "repository" | "status";

export type BoardViewId = string;

export type BoardViewLaneDefinition = { key: string, title: string, };

export type BoardViewSort = { field: BoardViewSortField, direction: BoardViewSortDirection, };

export type BoardViewSortDirection = "ascending" | "descending";

export type BoardViewSortField = "title" | "key";

export type CheckoutAvailability = "available" | "missing" | "deleted" | "replaced";

export type CheckoutBindingProjection = { featureId: FeatureId, workItemId: WorkItemId | null, purposeSource: CheckoutPurposeSource, };

export type CheckoutId = string;

export type CheckoutObservabilityProjection = { id: CheckoutId, repository: RepositoryReference, purpose: CheckoutPurpose, purposeSource: CheckoutPurposeSource, branch: string | null, head: string | null, isolationGeneration: number | null, reconciliationGeneration: number | null, availability: CheckoutAvailability, displayPaths: Array<ObservedDisplayPath>, replacesCheckoutId: CheckoutId | null, replacedByCheckoutId: CheckoutId | null, bindings: Array<CheckoutBindingProjection>, sessionIds: Array<SessionId>, dirtyEvidence: ClassifiedEvidence, collisionEvidence: ClassifiedEvidence, reconciliationEvidence: ClassifiedEvidence, revision: number, diagnostics: Array<Diagnostic>, };

export type CheckoutPathId = string;

export type CheckoutPurpose = "feature_integration" | "work_item_write" | "writer_session" | "read_only_shared" | "unknown";

export type CheckoutPurposeSource = "declared" | "inherited" | "override" | "unknown";

export type ClassifiedEvidence = { state: EvidenceState, code: string, message: string, observedAt: string | null, };

export type CommandCapability = { code: CommandCode, available: boolean, compatibleVersions: Array<number>, unavailableReason: UnavailableReason | null, };

export type CommandCode = "save_board_view" | "approve_feature" | "request_feature_revision" | "reject_feature" | "checkpoint_work_item" | "start_session" | "resume_session" | "focus_session" | "follow_up_session" | "recover_session";

export type CommandOperation = { "type": "save_board_view", "value": { definition: BoardViewDefinition, } } | { "type": "approve_feature", "value": { featureId: FeatureId, } } | { "type": "request_feature_revision", "value": { featureId: FeatureId, feedback: string, } } | { "type": "reject_feature", "value": { featureId: FeatureId, } } | { "type": "checkpoint_work_item", "value": { workItemId: WorkItemId, } } | { "type": "start_session", "value": { workItemId: WorkItemId, repositoryId: RepositoryId | null, provider: Provider, } } | { "type": "resume_session", "value": { sessionId: SessionId, } } | { "type": "focus_session", "value": { sessionId: SessionId, } } | { "type": "follow_up_session", "value": { sessionId: SessionId, } } | { "type": "recover_session", "value": { sessionId: SessionId, } };

export type DaemonInstanceId = string;

export type DependencyReadiness = "ready" | "waiting" | "blocked" | "complete";

export type Diagnostic = { code: string, severity: ErrorSeverity, message: string, owner: EntityRef | null, };

export type DocumentId = string;

export type DurableWorkItemSection = { entries: Array<string>, evidence: ClassifiedEvidence, };

export type EffectiveCheckoutProjection = { featureId: FeatureId, workItemId: WorkItemId | null, repositoryId: RepositoryId, checkoutId: CheckoutId, inherited: boolean, };

export type EntityRef = { "kind": "workspace", "id": WorkspaceId } | { "kind": "repository", "id": RepositoryId } | { "kind": "epic", "id": EpicId } | { "kind": "feature", "id": FeatureId } | { "kind": "work_item", "id": WorkItemId } | { "kind": "session", "id": SessionId };

export type EpicId = string;

export type EpicReference = { id: EpicId, workspaceId: WorkspaceId, slug: string, title: string, };

export type ErrorSeverity = "info" | "warning" | "error" | "fatal";

export type EventCursor = { daemonInstanceId: DaemonInstanceId, sequence: number, };

export type EventEnvelope = { protocolVersion: number, eventVersion: number, workspaceId: WorkspaceId, sequence: number, eventId: EventId, occurredAt: string, owner: EntityRef, entityRevision: number, kind: EventKind, payload: EventPayload | null, invalidationScope: InvalidationScope | null, operationCorrelationId: RequestId, partialOutcomes: Array<PartialOutcome>, };

export type EventId = string;

export type EventKind = "projection_changed" | "board_view_saved" | "native_sessions_refreshed" | "partial_outcome_recorded" | "checkout_changed" | "session_liveness_changed" | "proposal_changed" | "work_item_changed";

export type EventPayload = { "type": "projection_changed", "value": { entity: EntityRef, } } | { "type": "board_view_saved", "value": { view: BoardViewDefinition, } } | { "type": "native_sessions_refreshed", "value": { sessionCount: number, } } | { "type": "partial_outcome", "value": { outcome: PartialOutcome, } } | { "type": "board_card_changed", "value": { card: BoardCardProjection, } } | { "type": "checkout_changed", "value": { checkout: CheckoutObservabilityProjection, cards: Array<BoardCardProjection>, } } | { "type": "session_liveness_changed", "value": { session: SessionObservabilityProjection, recovery: RecoveryPreviewProjection, cards: Array<BoardCardProjection>, } } | { "type": "proposal_changed", "value": { proposal: FeatureProposalProjection, queueItem: ApprovalQueueItemProjection | null, } } | { "type": "work_item_changed", "value": { detail: WorkItemDetailProjection, card: BoardCardProjection | null, } };

export type EvidenceState = "current" | "historical" | "stale" | "missing" | "unknown" | "conflict" | "not_loaded";

export type FeatureId = string;

export type FeatureProposalProjection = { feature: FeatureReference, generation: number, revision: number, proposalHash: string, submittedAt: string, changedSincePrevious: boolean, featureBody: string, workItems: Array<ProposedWorkItemProjection>, repositories: Array<RepositoryReference>, verificationGates: Array<string>, warnings: Array<ProposalWarningProjection>, plannerSessions: Array<PlannerSessionProjection>, diagnostics: Array<Diagnostic>, workflowState: WorkflowState, availableActions: Array<AvailableAction>, };

export type FeatureReference = { id: FeatureId, epicId: EpicId, slug: string, title: string, };

export type HandshakeRequest = { supportedReadVersions: Array<number>, supportedCommandVersions: Array<number>, };

export type HandshakeResponse = { daemonInstanceId: DaemonInstanceId, negotiatedReadVersion: number, compatibleCommandVersions: Array<number>, workspaces: Array<WorkspaceReference>, commandCapabilities: Array<CommandCapability>, eventVersion: number, heartbeatIntervalMs: number, maxFrameBytes: number, };

export type Heartbeat = { daemonInstanceId: DaemonInstanceId, workspaceId: WorkspaceId, revision: number, sentAt: string, };

export type HierarchyChildren = { parent: HierarchyRef, children: Array<HierarchyNode>, };

export type HierarchyEpic = { epic: EpicReference, repositoryIds: Array<RepositoryId>, };

export type HierarchyFeature = { feature: FeatureReference, repositoryIds: Array<RepositoryId>, };

export type HierarchyNode = { "kind": "repository", "value": RepositoryReference } | { "kind": "epic", "value": EpicReference } | { "kind": "feature", "value": FeatureReference } | { "kind": "work_item", "value": WorkItemReference };

export type HierarchyRef = { "kind": "workspace", "id": WorkspaceId } | { "kind": "epic", "id": EpicId } | { "kind": "feature", "id": FeatureId } | { "kind": "work_item", "id": WorkItemId };

export type HierarchyWorkItem = { workItem: WorkItemReference, repositoryIds: Array<RepositoryId>, status: WorkItemStatus, };

export type InvalidationScope = { queries: Array<ReadQueryCode>, owners: Array<EntityRef>, };

export type ManagedSessionRole = "epic_navigation" | "feature_planning" | "work_item_execution" | "debugging" | "review";

export type ObservedDisplayPath = { displayPath: string, state: EvidenceState, observedFrom: string, observedUntil: string | null, };

export type Operation = { "type": "handshake", "value": HandshakeRequest } | { "type": "query", "value": ReadQuery } | { "type": "command", "value": CommandOperation } | { "type": "subscribe", "value": SubscriptionRequest };

export type OwnerProjection = { "kind": "epic", "id": EpicId } | { "kind": "feature", "id": FeatureId } | { "kind": "work_item", "id": WorkItemId };

export type ParallelReadiness = { groupKey: string, readyCount: number, waitingCount: number, };

export type PartialOutcome = { owner: EntityRef | null, code: string, succeeded: boolean, message: string, reconciliationRequired: boolean, evidence: Array<Diagnostic>, };

export type PlannerSessionProjection = { id: SessionId, provider: Provider, role: ManagedSessionRole, bindingState: SessionBindingState, liveState: SessionLiveState, lastActivityAt: string | null, };

export type PrimaryWriterEvidence = "confirmed_primary" | "confirmed_secondary" | "not_applicable" | "unknown" | "conflict";

export type ProposalWarningProjection = { code: string, severity: ErrorSeverity, message: string, };

export type ProposedWorkItemProjection = { id: WorkItemId, slug: string, title: string, body: string, repositories: Array<RepositoryReference>, dependencies: Array<string>, position: number, };

export type ProtocolError = { code: string, message: string, severity: ErrorSeverity, retryable: boolean, validationFields: Array<ValidationField>, staleRevision: number | null, currentRevision: number | null, reconciliationOwner: EntityRef | null, correlationId: RequestId | null, resync: ResyncRequirement | null, };

export type Provider = "claude" | "codex";

export type ReadQuery = { "type": "workspace_summary" } | { "type": "hierarchy_children", "value": { parent: HierarchyRef, } } | { "type": "workspace_hierarchy" } | { "type": "board_views" } | { "type": "board_view", "value": { viewId: BoardViewId, } } | { "type": "board", "value": { query: BoardQuery, } } | { "type": "attention", "value": { query: AttentionQuery, } } | { "type": "approval_queue" } | { "type": "feature_proposal", "value": { featureId: FeatureId, } } | { "type": "work_item_detail", "value": { workItemId: WorkItemId, } } | { "type": "repository_observability", "value": { repositoryId: RepositoryId, } } | { "type": "checkout_observability", "value": { checkoutId: CheckoutId, } } | { "type": "session_observability", "value": { sessionId: SessionId, } } | { "type": "recovery_preview", "value": { sessionId: SessionId, } } | { "type": "board_snapshot" };

export type ReadQueryCode = "workspace_summary" | "hierarchy_children" | "workspace_hierarchy" | "board_views" | "board_view" | "board" | "attention" | "approval_queue" | "feature_proposal" | "work_item_detail" | "repository_observability" | "checkout_observability" | "session_observability" | "recovery_preview" | "board_snapshot";

export type RecoveryDispositionProjection = "ready_present" | "ready_recreate" | "already_live" | "conflict" | "unresumable" | "not_loaded";

export type RecoveryPreviewProjection = { sessionId: SessionId, disposition: RecoveryDispositionProjection, conflicts: Array<Diagnostic>, observedAt: string, stale: boolean, revision: number, };

export type RepositoryId = string;

export type RepositoryObservabilityProjection = { repository: RepositoryReference, displayPaths: Array<ObservedDisplayPath>, remoteNames: Array<string>, defaultBranch: string | null, remoteEvidence: ClassifiedEvidence, defaultBranchEvidence: ClassifiedEvidence, checkoutIds: Array<CheckoutId>, revision: number, diagnostics: Array<Diagnostic>, };

export type RepositoryPathId = string;

export type RepositoryReference = { id: RepositoryId, workspaceId: WorkspaceId, slug: string, title: string, };

export type RequestEnvelope = { protocolVersion: number, requestId: RequestId, workspaceId: WorkspaceId | null, expectedRevision: number | null, idempotencyKey: string | null, operation: Operation, };

export type RequestId = string;

export type ResponseEnvelope = { protocolVersion: number, requestId: RequestId, correlationId: RequestId, workspaceId: WorkspaceId | null, authoritativeRevision: number | null, serverTimestamp: string, result: ResponseResult | null, error: ProtocolError | null, diagnostics: Array<Diagnostic>, availableActions: Array<AvailableAction>, partialOutcomes: Array<PartialOutcome>, };

export type ResponseResult = { "type": "handshake", "value": HandshakeResponse } | { "type": "workspace_summary", "value": WorkspaceSummary } | { "type": "hierarchy_children", "value": HierarchyChildren } | { "type": "workspace_hierarchy", "value": WorkspaceHierarchy } | { "type": "board_views", "value": Array<BoardViewDefinition> } | { "type": "board_view", "value": BoardViewDefinition } | { "type": "board", "value": BoardPage } | { "type": "attention", "value": AttentionPage } | { "type": "approval_queue", "value": ApprovalQueueProjection } | { "type": "feature_proposal", "value": FeatureProposalProjection } | { "type": "work_item_detail", "value": WorkItemDetailProjection } | { "type": "repository_observability", "value": RepositoryObservabilityProjection } | { "type": "checkout_observability", "value": CheckoutObservabilityProjection } | { "type": "session_observability", "value": SessionObservabilityProjection } | { "type": "recovery_preview", "value": RecoveryPreviewProjection } | { "type": "board_snapshot", "value": unknown } | { "type": "subscription_accepted", "value": { cursor: EventCursor, } } | { "type": "command_accepted", "value": { code: CommandCode, } };

export type ResyncReason = "gap" | "cursor_expired" | "daemon_restarted" | "incompatible_event" | "heartbeat_lost";

export type ResyncRequirement = { reason: ResyncReason, workspaceId: WorkspaceId, authoritativeRevision: number, oldestReplayableSequence: number, requiredQueries: Array<ReadQueryCode>, };

export type ReviewDeliveryState = "not_requested" | "review_requested" | "delivery_requested";

export type ServerMessage = { "type": "response", "value": ResponseEnvelope } | { "type": "event", "value": EventEnvelope } | { "type": "heartbeat", "value": Heartbeat } | { "type": "resync_required", "value": ResyncRequirement };

export type SessionBindingState = "pending" | "current" | "stopped" | "reconciliation_required";

export type SessionId = string;

export type SessionLiveState = "active" | "idle" | "stopped" | "unknown" | "system_error" | "not_loaded";

export type SessionLivenessProjection = { state: SessionLiveState, stale: boolean, observedAt: string | null, expiresAt: string | null, evidence: ClassifiedEvidence, };

export type SessionObservabilityProjection = { id: SessionId, provider: Provider, role: ManagedSessionRole, owner: OwnerProjection, authoritativeProfile: string | null, authoritativeModel: string | null, profileEvidence: ClassifiedEvidence, bindingState: SessionBindingState, liveness: SessionLivenessProjection, restoreState: SessionRestoreState, lastActivityAt: string | null, checkoutId: CheckoutId | null, resumability: SessionResumability, primaryWriter: PrimaryWriterEvidence, revision: number, diagnostics: Array<Diagnostic>, };

export type SessionRestoreState = "tracked" | "removed" | "not_tracked" | "conflict";

export type SessionResumability = "validated" | "preflight_passed" | "unknown" | "missing" | "corrupt" | "unsupported";

export type SessionSummary = { total: number, active: number, idle: number, unknown: number, providers: Array<Provider>, };

export type SubscriptionRequest = { cursor: EventCursor | null, };

export type UnavailableReason = { code: string, message: string, };

export type ValidationField = { field: string, code: string, message: string, };

export type WorkItemBlockerProjection = { code: string, message: string, prerequisite: WorkItemReference | null, };

export type WorkItemCheckpointId = string;

export type WorkItemCheckpointProjection = { id: WorkItemCheckpointId, sessionId: SessionId, nextAction: WorkItemNextActionKind, summary: string, recordedAt: string, };

export type WorkItemDetailProjection = { workItem: WorkItemReference, feature: FeatureReference, outcomeDesignSummary: string, currentState: DurableWorkItemSection, dependencyReadiness: DependencyReadiness, blockers: Array<WorkItemBlockerProjection>, decisions: DurableWorkItemSection, verification: DurableWorkItemSection, nextAction: WorkItemNextActionProjection | null, reviewDeliveryState: ReviewDeliveryState, workflowState: WorkflowState, status: WorkItemStatus, repositories: Array<RepositoryReference>, checkouts: Array<CheckoutObservabilityProjection>, revision: number, contentRevision: number, contentHash: string, checkpointHistory: Array<WorkItemCheckpointProjection>, sessions: Array<SessionObservabilityProjection>, diagnostics: Array<Diagnostic>, availableActions: Array<AvailableAction>, };

export type WorkItemId = string;

export type WorkItemNextActionKind = "actionable" | "blocked" | "paused" | "review" | "delivery";

export type WorkItemNextActionProjection = { kind: WorkItemNextActionKind, recordedAt: string, };

export type WorkItemReference = { id: WorkItemId, featureId: FeatureId, key: string, slug: string, title: string, };

export type WorkItemStatus = "backlog" | "ready" | "in_progress" | "blocked" | "review" | "done" | "cancelled";

export type WorkflowState = "draft" | "worktree_pending" | "planning_launch_pending" | "planning_active" | "proposal_ready" | "awaiting_approval" | "publishing" | "planned" | "work_item_launch_pending" | "work_item_active" | "reconciliation_required" | "blocked" | "paused" | "completed" | "cancelled";

export type WorkspaceHierarchy = { workspace: WorkspaceReference, repositories: Array<RepositoryReference>, epics: Array<HierarchyEpic>, features: Array<HierarchyFeature>, workItems: Array<HierarchyWorkItem>, recentEntities: Array<EntityRef>, focusedEntity: EntityRef | null, };

export type WorkspaceId = string;

export type WorkspaceReference = { id: WorkspaceId, slug: string, title: string, };

export type WorkspaceSummary = { workspace: WorkspaceReference, repositoryCount: number, epicCount: number, featureCount: number, workItemCount: number, sessionCount: number, };
export type BootstrapHandshake = { state: BootstrapState, subscriptions: Array<SubscriptionTarget>, };

export type BootstrapState = "connecting" | "disconnected" | "incompatible" | "read_only" | "resyncing" | "ready";

export type BridgeError = { code: string, message: string, };

export type ExecuteRequest = { workspaceId: WorkspaceId, expectedRevision: number, idempotencyKey: string, command: CommandOperation, };

export type QueryRequest = { workspaceId: WorkspaceId, query: ReadQuery, };

export type SubscribeRequest = { "type": "start", "value": { workspaceId: WorkspaceId, cursor: EventCursor | null, } } | { "type": "cancel", "value": { subscriptionId: number, } };

export type SubscriptionMessage = { "type": "connected", "value": { state: BootstrapState, } } | { "type": "event", "value": EventEnvelope } | { "type": "resyncing", "value": ResyncRequirement } | { "type": "resynced", "value": { requirement: ResyncRequirement, snapshot: unknown, } } | { "type": "disconnected", "value": { code: string, } } | { "type": "incompatible" };

export type SubscriptionReceipt = { subscriptionId: number, };

export type SubscriptionTarget = { workspaceId: WorkspaceId, };
