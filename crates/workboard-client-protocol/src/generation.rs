use serde_json::{Value, json};
use ts_rs::{Config, TS};

use crate::{
    ApprovalQueueItemProjection, ApprovalQueueProjection, AssociationId, AttentionEntryProjection,
    AttentionPage, AttentionQuery, AttentionReason, AttentionReasonCode, AvailableAction,
    BlockedByEvidence, BoardCardProjection, BoardLaneProjection, BoardPage, BoardQuery,
    BoardViewDefinition, BoardViewDensity, BoardViewFilters, BoardViewGrouping,
    BoardViewGroupingKind, BoardViewId, BoardViewLaneDefinition, BoardViewSort,
    BoardViewSortDirection, BoardViewSortField, CURRENT_PROTOCOL_VERSION, CheckoutAvailability,
    CheckoutBindingProjection, CheckoutId, CheckoutObservabilityProjection, CheckoutPathId,
    CheckoutPurpose, CheckoutPurposeSource, ClassifiedEvidence, CommandCapability, CommandCode,
    CommandOperation, DaemonInstanceId, DependencyReadiness, Diagnostic, DocumentId,
    DurableWorkItemSection, EffectiveCheckoutProjection, EntityRef, EpicId, EpicReference,
    ErrorSeverity, EventCursor, EventEnvelope, EventId, EventKind, EventPayload, EvidenceState,
    FeatureId, FeatureProposalProjection, FeatureReference, HandshakeRequest, HandshakeResponse,
    Heartbeat, HierarchyChildren, HierarchyEpic, HierarchyFeature, HierarchyNode, HierarchyRef,
    HierarchyWorkItem, InvalidationScope, ManagedSessionRole, ObservedDisplayPath, Operation,
    OwnerProjection, PREVIOUS_PROTOCOL_VERSION, ParallelReadiness, PartialOutcome,
    PlannerSessionProjection, PrimaryWriterEvidence, ProposalWarningProjection,
    ProposedWorkItemProjection, ProtocolError, Provider, ReadQuery, ReadQueryCode,
    RecoveryDispositionProjection, RecoveryPreviewProjection, RepositoryId,
    RepositoryObservabilityProjection, RepositoryPathId, RepositoryReference, RequestEnvelope,
    RequestId, ResponseEnvelope, ResponseResult, ResyncReason, ResyncRequirement,
    ReviewDeliveryState, ServerMessage, SessionBindingState, SessionId, SessionLiveState,
    SessionLivenessProjection, SessionObservabilityProjection, SessionRestoreState,
    SessionResumability, SessionSummary, SubscriptionRequest, UnavailableReason, ValidationField,
    WorkItemBlockerProjection, WorkItemCheckpointId, WorkItemCheckpointProjection,
    WorkItemDetailProjection, WorkItemId, WorkItemNextActionKind, WorkItemNextActionProjection,
    WorkItemReference, WorkItemStatus, WorkflowState, WorkspaceHierarchy, WorkspaceId,
    WorkspaceReference, WorkspaceSummary,
};

const REQUEST_ID: &str = "10000000-0000-0000-0000-000000000001";
const CORRELATION_ID: &str = "10000000-0000-0000-0000-000000000002";
const WORKSPACE_ID: &str = "20000000-0000-0000-0000-000000000001";
const REPOSITORY_ID: &str = "30000000-0000-0000-0000-000000000001";
const EPIC_ID: &str = "40000000-0000-0000-0000-000000000001";
const FEATURE_ID: &str = "50000000-0000-0000-0000-000000000001";
const WORK_ITEM_ID: &str = "60000000-0000-0000-0000-000000000001";
const SESSION_ID: &str = "70000000-0000-0000-0000-000000000001";
const CHECKOUT_ID: &str = "b0000000-0000-0000-0000-000000000001";
const DAEMON_ID: &str = "80000000-0000-0000-0000-000000000001";
const EVENT_ID: &str = "90000000-0000-0000-0000-000000000001";
const BOARD_VIEW_ID: &str = "a0000000-0000-0000-0000-000000000001";
const CHECKPOINT_ID: &str = "c0000000-0000-0000-0000-000000000001";

fn board_view(revision: u64) -> Value {
    json!({
        "id": BOARD_VIEW_ID,
        "workspaceId": WORKSPACE_ID,
        "title": "Fixture service view",
        "filters": {
            "query": "fixture",
            "repositoryIds": [REPOSITORY_ID],
            "statuses": ["ready", "in_progress"]
        },
        "grouping": {
            "kind": "repository",
            "lanes": [{ "key": "fixture", "title": "Fixture" }]
        },
        "sort": { "field": "title", "direction": "ascending" },
        "density": "comfortable",
        "revision": revision
    })
}

fn board_query() -> Value {
    json!({
        "cursor": null,
        "limit": 100,
        "query": "fixture",
        "repositoryIds": [REPOSITORY_ID],
        "statuses": ["ready", "in_progress"],
        "laneKeys": ["ready", "in_progress"],
        "sort": { "field": "key", "direction": "ascending" }
    })
}

fn board_card() -> Value {
    json!({
        "workItem": { "id": WORK_ITEM_ID, "featureId": FEATURE_ID, "key": "WI-1", "slug": "fixture-work-item", "title": "Fixture Work item" },
        "feature": { "id": FEATURE_ID, "epicId": EPIC_ID, "slug": "fixture-feature", "title": "Fixture Feature" },
        "status": "in_progress",
        "laneKey": "in_progress",
        "lanePosition": 1,
        "laneCount": 1,
        "dependencyReadiness": "ready",
        "blockedBy": [],
        "parallelReadiness": { "groupKey": "fixture-ready", "readyCount": 1, "waitingCount": 0 },
        "repositories": [{ "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" }],
        "sessionSummary": { "total": 1, "active": 1, "idle": 0, "unknown": 0, "providers": ["codex"] },
        "checkoutIds": [CHECKOUT_ID],
        "sessionIds": [SESSION_ID],
        "attentionReasons": [{ "code": "checkpoint_due", "rank": 5, "message": "Checkpoint evidence is due" }],
        "revision": 3,
        "availableActions": []
    })
}

fn classified(state: &str, code: &str, message: &str) -> Value {
    json!({ "state": state, "code": code, "message": message, "observedAt": "2026-08-30T12:00:00Z" })
}

fn repository_observability() -> Value {
    json!({
        "repository": { "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" },
        "displayPaths": [{ "displayPath": "repos/fixture", "state": "current", "observedFrom": "2026-08-30T12:00:00Z", "observedUntil": null }],
        "remoteNames": ["origin"], "defaultBranch": "main",
        "remoteEvidence": classified("current", "remote_names_observed", "Remote names were observed by Workboard."),
        "defaultBranchEvidence": classified("current", "default_branch_observed", "The default branch was observed by Workboard."),
        "checkoutIds": [CHECKOUT_ID], "revision": 41, "diagnostics": []
    })
}

fn checkout_observability() -> Value {
    json!({
        "id": CHECKOUT_ID,
        "repository": { "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" },
        "purpose": "work_item_write", "purposeSource": "override", "branch": "feature/fixture", "head": "0123456789abcdef",
        "isolationGeneration": 2, "reconciliationGeneration": 3, "availability": "available",
        "displayPaths": [{ "displayPath": "worktrees/fixture", "state": "current", "observedFrom": "2026-08-30T12:00:00Z", "observedUntil": null }],
        "replacesCheckoutId": null, "replacedByCheckoutId": null,
        "bindings": [{ "featureId": FEATURE_ID, "workItemId": WORK_ITEM_ID, "purposeSource": "override" }],
        "sessionIds": [SESSION_ID],
        "dirtyEvidence": classified("not_loaded", "dirty_evidence_not_loaded", "No authoritative dirty-state observation is loaded."),
        "collisionEvidence": classified("unknown", "collision_evidence_unknown", "No authoritative collision scan is loaded."),
        "reconciliationEvidence": classified("current", "checkout_reconciled", "The latest recorded checkout generation is available."),
        "revision": 41, "diagnostics": []
    })
}

fn session_observability() -> Value {
    json!({
        "id": SESSION_ID, "provider": "codex", "role": "work_item_execution",
        "owner": { "kind": "work_item", "id": WORK_ITEM_ID },
        "authoritativeProfile": "reviewed", "authoritativeModel": "fixture-model",
        "profileEvidence": classified("current", "profile_observed", "The profile was observed by Workboard."),
        "bindingState": "current",
        "liveness": {
            "state": "active", "stale": false, "observedAt": "2026-08-30T12:00:00Z", "expiresAt": "2026-08-30T12:05:00Z",
            "evidence": classified("current", "liveness_observed", "The liveness state is backed by current Workboard evidence.")
        },
        "restoreState": "tracked", "lastActivityAt": "2026-08-30T12:00:00Z",
        "checkoutId": CHECKOUT_ID, "resumability": "validated", "primaryWriter": "confirmed_primary",
        "revision": 41, "diagnostics": []
    })
}

fn recovery_preview() -> Value {
    json!({
        "sessionId": SESSION_ID, "disposition": "already_live", "conflicts": [],
        "observedAt": "2026-08-30T12:00:00Z", "stale": false, "revision": 41
    })
}

fn feature_proposal() -> Value {
    json!({
        "feature": { "id": FEATURE_ID, "epicId": EPIC_ID, "slug": "fixture-feature", "title": "Fixture Feature" },
        "generation": 2,
        "revision": 41,
        "proposalHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "submittedAt": "2026-08-30T12:00:00Z",
        "changedSincePrevious": true,
        "featureBody": "# Fixture Feature\n\n<script>not executable</script>\n\n[unsafe](javascript:alert(1))",
        "workItems": [{
            "id": WORK_ITEM_ID,
            "slug": "fixture-work-item",
            "title": "Fixture Work item",
            "body": "Review the complete proposal as text.",
            "repositories": [{ "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" }],
            "dependencies": [],
            "position": 1
        }],
        "repositories": [{ "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" }],
        "verificationGates": ["The focused verification suite passes."],
        "warnings": [{ "code": "proposal_changed", "severity": "warning", "message": "This proposal replaces an earlier submitted generation." }],
        "plannerSessions": [{ "id": SESSION_ID, "provider": "codex", "role": "feature_planning", "bindingState": "current", "liveState": "active", "lastActivityAt": "2026-08-30T12:00:00Z" }],
        "diagnostics": [],
        "workflowState": "awaiting_approval",
        "availableActions": [{
            "code": "approve_feature",
            "available": false,
            "unavailableReason": { "code": "publication_policy_unavailable", "message": "Desktop approval actions are unavailable until the daemon accepts the typed publication policy." },
            "expectedRevision": 41
        }]
    })
}

fn approval_queue() -> Value {
    let proposal = feature_proposal();
    json!({
        "entries": [{
            "feature": proposal["feature"],
            "generation": proposal["generation"],
            "revision": proposal["revision"],
            "proposalHash": proposal["proposalHash"],
            "submittedAt": proposal["submittedAt"],
            "changedSincePrevious": proposal["changedSincePrevious"],
            "workflowState": proposal["workflowState"],
            "repositories": proposal["repositories"],
            "warningCount": 1,
            "plannerCount": 1,
            "availableActions": proposal["availableActions"],
            "position": 1,
            "totalCount": 1
        }],
        "revision": 41
    })
}

fn work_item_detail() -> Value {
    json!({
        "workItem": { "id": WORK_ITEM_ID, "featureId": FEATURE_ID, "key": "WI-1", "slug": "fixture-work-item", "title": "Fixture Work item" },
        "feature": { "id": FEATURE_ID, "epicId": EPIC_ID, "slug": "fixture-feature", "title": "Fixture Feature" },
        "outcomeDesignSummary": "Long hostile content remains plain text: <script>alert('no')</script>\n".repeat(40),
        "currentState": { "entries": [], "evidence": classified("not_loaded", "structured_checkpoint_unavailable", "Structured current-state evidence is unavailable.") },
        "dependencyReadiness": "waiting",
        "blockers": [{ "code": "dependency_incomplete", "message": "Prerequisite is in progress.", "prerequisite": { "id": "60000000-0000-0000-0000-000000000002", "featureId": FEATURE_ID, "key": "WI-0", "slug": "prerequisite", "title": "Prerequisite" } }],
        "decisions": { "entries": [], "evidence": classified("not_loaded", "structured_checkpoint_unavailable", "Structured decision evidence is unavailable.") },
        "verification": { "entries": [], "evidence": classified("not_loaded", "structured_checkpoint_unavailable", "Structured verification evidence is unavailable.") },
        "nextAction": { "kind": "review", "recordedAt": "2026-08-30T12:00:00Z" },
        "reviewDeliveryState": "review_requested",
        "workflowState": "reconciliation_required",
        "status": "review",
        "repositories": [{ "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" }],
        "checkouts": [checkout_observability()],
        "revision": 41,
        "contentRevision": 3,
        "contentHash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "checkpointHistory": [{ "id": CHECKPOINT_ID, "sessionId": SESSION_ID, "nextAction": "review", "summary": "Opaque checkpoint summary <img src=x onerror=alert(1)>", "recordedAt": "2026-08-30T12:00:00Z" }],
        "sessions": [session_observability()],
        "diagnostics": [{ "code": "work_item_reconciliation_required", "severity": "error", "message": "This Work item requires authoritative reconciliation outside Desktop.", "owner": { "kind": "work_item", "id": WORK_ITEM_ID } }],
        "availableActions": [{ "code": "checkpoint_work_item", "available": false, "unavailableReason": { "code": "structured_checkpoint_unavailable", "message": "Structured checkpoint editing is unavailable." }, "expectedRevision": 41 }]
    })
}

pub fn typescript_declarations() -> String {
    let config = Config::default();
    let mut declarations = Vec::new();
    macro_rules! declaration {
        ($type:ty) => {
            declarations.push((
                <$type as TS>::name(&config),
                <$type as TS>::decl(&config).replace("bigint", "number"),
            ));
        };
    }

    declaration!(RequestId);
    declaration!(EventId);
    declaration!(DaemonInstanceId);
    declaration!(WorkspaceId);
    declaration!(RepositoryId);
    declaration!(RepositoryPathId);
    declaration!(EpicId);
    declaration!(FeatureId);
    declaration!(WorkItemId);
    declaration!(WorkItemCheckpointId);
    declaration!(SessionId);
    declaration!(CheckoutId);
    declaration!(CheckoutPathId);
    declaration!(DocumentId);
    declaration!(AssociationId);
    declaration!(BoardViewId);
    declaration!(HierarchyRef);
    declaration!(EntityRef);
    declaration!(WorkspaceReference);
    declaration!(RepositoryReference);
    declaration!(EpicReference);
    declaration!(FeatureReference);
    declaration!(WorkItemReference);
    declaration!(WorkspaceHierarchy);
    declaration!(HierarchyEpic);
    declaration!(HierarchyFeature);
    declaration!(HierarchyWorkItem);
    declaration!(BoardViewDefinition);
    declaration!(BoardQuery);
    declaration!(AttentionQuery);
    declaration!(BoardPage);
    declaration!(AttentionPage);
    declaration!(ApprovalQueueProjection);
    declaration!(ApprovalQueueItemProjection);
    declaration!(FeatureProposalProjection);
    declaration!(WorkItemDetailProjection);
    declaration!(DurableWorkItemSection);
    declaration!(WorkItemBlockerProjection);
    declaration!(WorkItemNextActionKind);
    declaration!(WorkItemNextActionProjection);
    declaration!(ReviewDeliveryState);
    declaration!(WorkItemCheckpointProjection);
    declaration!(ProposedWorkItemProjection);
    declaration!(ProposalWarningProjection);
    declaration!(PlannerSessionProjection);
    declaration!(BoardLaneProjection);
    declaration!(BoardCardProjection);
    declaration!(AttentionEntryProjection);
    declaration!(DependencyReadiness);
    declaration!(BlockedByEvidence);
    declaration!(ParallelReadiness);
    declaration!(SessionSummary);
    declaration!(AttentionReason);
    declaration!(AttentionReasonCode);
    declaration!(BoardViewFilters);
    declaration!(BoardViewGrouping);
    declaration!(BoardViewLaneDefinition);
    declaration!(BoardViewGroupingKind);
    declaration!(BoardViewSort);
    declaration!(BoardViewSortField);
    declaration!(BoardViewSortDirection);
    declaration!(BoardViewDensity);
    declaration!(WorkspaceSummary);
    declaration!(HierarchyChildren);
    declaration!(HierarchyNode);
    declaration!(OwnerProjection);
    declaration!(EffectiveCheckoutProjection);
    declaration!(Provider);
    declaration!(WorkItemStatus);
    declaration!(WorkflowState);
    declaration!(CheckoutAvailability);
    declaration!(ManagedSessionRole);
    declaration!(ObservedDisplayPath);
    declaration!(EvidenceState);
    declaration!(ClassifiedEvidence);
    declaration!(RepositoryObservabilityProjection);
    declaration!(CheckoutPurpose);
    declaration!(CheckoutPurposeSource);
    declaration!(CheckoutBindingProjection);
    declaration!(CheckoutObservabilityProjection);
    declaration!(SessionBindingState);
    declaration!(SessionLiveState);
    declaration!(SessionRestoreState);
    declaration!(SessionResumability);
    declaration!(PrimaryWriterEvidence);
    declaration!(SessionLivenessProjection);
    declaration!(SessionObservabilityProjection);
    declaration!(RecoveryDispositionProjection);
    declaration!(RecoveryPreviewProjection);
    declaration!(HandshakeRequest);
    declaration!(HandshakeResponse);
    declaration!(RequestEnvelope);
    declaration!(Operation);
    declaration!(ReadQuery);
    declaration!(CommandCode);
    declaration!(CommandOperation);
    declaration!(SubscriptionRequest);
    declaration!(EventCursor);
    declaration!(ResponseEnvelope);
    declaration!(ResponseResult);
    declaration!(ProtocolError);
    declaration!(ErrorSeverity);
    declaration!(ValidationField);
    declaration!(Diagnostic);
    declaration!(AvailableAction);
    declaration!(CommandCapability);
    declaration!(UnavailableReason);
    declaration!(PartialOutcome);
    declaration!(EventEnvelope);
    declaration!(EventKind);
    declaration!(EventPayload);
    declaration!(InvalidationScope);
    declaration!(ReadQueryCode);
    declaration!(ServerMessage);
    declaration!(Heartbeat);
    declaration!(ResyncRequirement);
    declaration!(ResyncReason);

    declarations.sort_by(|left, right| left.0.cmp(&right.0));
    declarations
        .into_iter()
        .map(|(_, declaration)| format!("export {declaration}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn conformance_fixture(protocol_version: u32) -> Value {
    assert!([CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION].contains(&protocol_version));
    let read_requests = read_requests(protocol_version);
    let mut responses = response_results(protocol_version);
    if protocol_version == PREVIOUS_PROTOCOL_VERSION {
        for response in &mut responses {
            response
                .as_object_mut()
                .expect("response object")
                .insert("futureOptionalReadField".to_owned(), json!("ignored"));
            if let Some(value) = response
                .pointer_mut("/result/value")
                .and_then(Value::as_object_mut)
            {
                value.insert("futureOptionalProjectionField".to_owned(), json!(true));
            }
        }
    }
    json!({
        "fixtureVersion": 1,
        "protocolVersion": protocol_version,
        "readRequests": read_requests,
        "frontendRequests": {
            "workspaceSummary": query_request(json!({ "type": "workspace_summary" })),
            "hierarchyChildren": query_request(json!({
                "type": "hierarchy_children",
                "value": { "parent": { "kind": "workspace", "id": WORKSPACE_ID } }
            })),
            "board": query_request(json!({ "type": "board", "value": { "query": board_query() } })),
            "attention": query_request(json!({ "type": "attention", "value": { "query": { "cursor": null, "limit": 100, "repositoryIds": [], "reasonCodes": [] } } })),
            "approvalQueue": query_request(json!({ "type": "approval_queue" })),
            "featureProposal": query_request(json!({ "type": "feature_proposal", "value": { "featureId": FEATURE_ID } })),
            "workItemDetail": query_request(json!({ "type": "work_item_detail", "value": { "workItemId": WORK_ITEM_ID } })),
            "repositoryObservability": query_request(json!({ "type": "repository_observability", "value": { "repositoryId": REPOSITORY_ID } })),
            "checkoutObservability": query_request(json!({ "type": "checkout_observability", "value": { "checkoutId": CHECKOUT_ID } })),
            "sessionObservability": query_request(json!({ "type": "session_observability", "value": { "sessionId": SESSION_ID } })),
            "recoveryPreview": query_request(json!({ "type": "recovery_preview", "value": { "sessionId": SESSION_ID } })),
            "execute": execute_request(),
            "subscribe": {
                "type": "start",
                "value": {
                    "workspaceId": WORKSPACE_ID,
                    "cursor": { "daemonInstanceId": DAEMON_ID, "sequence": 40 }
                }
            }
        },
        "incompatibleCommands": incompatible_commands(),
        "responses": responses,
        "typedErrors": typed_errors(),
        "serverMessages": server_messages(protocol_version),
        "partialOutcome": partial_outcome(),
        "discriminants": discriminants()
    })
}

pub fn fixture_bytes(protocol_version: u32) -> Vec<u8> {
    let mut bytes =
        serde_json::to_vec_pretty(&conformance_fixture(protocol_version)).expect("fixture JSON");
    bytes.push(b'\n');
    bytes
}

fn request(operation: Value, protocol_version: u32, workspace_id: Option<&str>) -> Value {
    json!({
        "protocolVersion": protocol_version,
        "requestId": REQUEST_ID,
        "workspaceId": workspace_id,
        "expectedRevision": null,
        "idempotencyKey": null,
        "operation": operation
    })
}

fn read_requests(protocol_version: u32) -> Vec<Value> {
    let mut requests = vec![
        request(
            json!({
                "type": "handshake",
                "value": {
                    "supportedReadVersions": [
                        CURRENT_PROTOCOL_VERSION,
                        PREVIOUS_PROTOCOL_VERSION
                    ],
                    "supportedCommandVersions": [CURRENT_PROTOCOL_VERSION]
                }
            }),
            protocol_version,
            None,
        ),
        request(
            json!({ "type": "query", "value": { "type": "workspace_summary" } }),
            protocol_version,
            Some(WORKSPACE_ID),
        ),
        request(
            json!({
                "type": "query",
                "value": {
                    "type": "hierarchy_children",
                    "value": { "parent": { "kind": "workspace", "id": WORKSPACE_ID } }
                }
            }),
            protocol_version,
            Some(WORKSPACE_ID),
        ),
        request(
            json!({ "type": "query", "value": { "type": "workspace_hierarchy" } }),
            protocol_version,
            Some(WORKSPACE_ID),
        ),
        request(
            json!({ "type": "query", "value": { "type": "board_views" } }),
            protocol_version,
            Some(WORKSPACE_ID),
        ),
        request(
            json!({ "type": "query", "value": { "type": "board_view", "value": { "viewId": BOARD_VIEW_ID } } }),
            protocol_version,
            Some(WORKSPACE_ID),
        ),
        request(
            json!({ "type": "query", "value": { "type": "board_snapshot" } }),
            protocol_version,
            Some(WORKSPACE_ID),
        ),
        request(
            json!({
                "type": "subscribe",
                "value": {
                    "cursor": { "daemonInstanceId": DAEMON_ID, "sequence": 40 }
                }
            }),
            protocol_version,
            Some(WORKSPACE_ID),
        ),
    ];
    if protocol_version == CURRENT_PROTOCOL_VERSION {
        requests.insert(6, request(json!({ "type": "query", "value": { "type": "board", "value": { "query": board_query() } } }), protocol_version, Some(WORKSPACE_ID)));
        requests.insert(7, request(json!({ "type": "query", "value": { "type": "attention", "value": { "query": { "cursor": null, "limit": 100, "repositoryIds": [], "reasonCodes": [] } } } }), protocol_version, Some(WORKSPACE_ID)));
        requests.insert(
            8,
            request(
                json!({ "type": "query", "value": { "type": "approval_queue" } }),
                protocol_version,
                Some(WORKSPACE_ID),
            ),
        );
        requests.insert(9, request(json!({ "type": "query", "value": { "type": "feature_proposal", "value": { "featureId": FEATURE_ID } } }), protocol_version, Some(WORKSPACE_ID)));
        requests.insert(10, request(json!({ "type": "query", "value": { "type": "work_item_detail", "value": { "workItemId": WORK_ITEM_ID } } }), protocol_version, Some(WORKSPACE_ID)));
        requests.insert(11, request(json!({ "type": "query", "value": { "type": "repository_observability", "value": { "repositoryId": REPOSITORY_ID } } }), protocol_version, Some(WORKSPACE_ID)));
        requests.insert(12, request(json!({ "type": "query", "value": { "type": "checkout_observability", "value": { "checkoutId": CHECKOUT_ID } } }), protocol_version, Some(WORKSPACE_ID)));
        requests.insert(13, request(json!({ "type": "query", "value": { "type": "session_observability", "value": { "sessionId": SESSION_ID } } }), protocol_version, Some(WORKSPACE_ID)));
        requests.insert(14, request(json!({ "type": "query", "value": { "type": "recovery_preview", "value": { "sessionId": SESSION_ID } } }), protocol_version, Some(WORKSPACE_ID)));
    }
    requests
}

fn query_request(query: Value) -> Value {
    json!({ "workspaceId": WORKSPACE_ID, "query": query })
}

fn execute_request() -> Value {
    json!({
        "workspaceId": WORKSPACE_ID,
        "expectedRevision": 41,
        "idempotencyKey": "fixture-command-1",
        "command": {
            "type": "approve_feature",
            "value": { "featureId": FEATURE_ID }
        }
    })
}

fn incompatible_commands() -> Vec<Value> {
    let commands = [
        json!({ "type": "save_board_view", "value": { "definition": board_view(1) } }),
        json!({ "type": "approve_feature", "value": { "featureId": FEATURE_ID } }),
        json!({
            "type": "request_feature_revision",
            "value": { "featureId": FEATURE_ID, "feedback": "Split the migration item." }
        }),
        json!({ "type": "reject_feature", "value": { "featureId": FEATURE_ID } }),
        json!({ "type": "checkpoint_work_item", "value": { "workItemId": WORK_ITEM_ID } }),
        json!({ "type": "start_session", "value": { "workItemId": WORK_ITEM_ID } }),
        json!({ "type": "resume_session", "value": { "sessionId": SESSION_ID } }),
        json!({ "type": "focus_session", "value": { "sessionId": SESSION_ID } }),
        json!({ "type": "follow_up_session", "value": { "sessionId": SESSION_ID } }),
        json!({ "type": "recover_session", "value": { "sessionId": SESSION_ID } }),
    ];
    commands
        .into_iter()
        .map(|command| {
            json!({
                "protocolVersion": PREVIOUS_PROTOCOL_VERSION,
                "requestId": REQUEST_ID,
                "workspaceId": WORKSPACE_ID,
                "expectedRevision": 41,
                "idempotencyKey": "fixture-incompatible-command",
                "operation": { "type": "command", "value": command }
            })
        })
        .collect()
}

fn response_envelope(protocol_version: u32, result: Value) -> Value {
    json!({
        "protocolVersion": protocol_version,
        "requestId": REQUEST_ID,
        "correlationId": CORRELATION_ID,
        "workspaceId": WORKSPACE_ID,
        "authoritativeRevision": 41,
        "serverTimestamp": "2026-08-30T12:00:00Z",
        "result": result,
        "error": null,
        "diagnostics": [],
        "availableActions": [{
            "code": "approve_feature",
            "available": false,
            "unavailableReason": {
                "code": "not_accepted",
                "message": "The capability is not accepted."
            },
            "expectedRevision": 41
        }],
        "partialOutcomes": []
    })
}

fn response_results(protocol_version: u32) -> Vec<Value> {
    let mut results = vec![
        response_envelope(
            protocol_version,
            json!({
                "type": "handshake",
                "value": {
                    "daemonInstanceId": DAEMON_ID,
                    "negotiatedReadVersion": protocol_version,
                    "compatibleCommandVersions": [CURRENT_PROTOCOL_VERSION],
                    "workspaces": [{
                        "id": WORKSPACE_ID,
                        "slug": "fixture-workspace",
                        "title": "Fixture Workspace"
                    }],
                    "commandCapabilities": [{
                        "code": "save_board_view",
                        "available": false,
                        "compatibleVersions": [CURRENT_PROTOCOL_VERSION],
                        "unavailableReason": {
                            "code": "not_accepted",
                            "message": "The capability is not accepted."
                        }
                    }],
                    "eventVersion": 1,
                    "heartbeatIntervalMs": 1000,
                    "maxFrameBytes": 8388608
                }
            }),
        ),
        response_envelope(
            protocol_version,
            json!({
                "type": "workspace_summary",
                "value": {
                    "workspace": {
                        "id": WORKSPACE_ID,
                        "slug": "fixture-workspace",
                        "title": "Fixture Workspace"
                    },
                    "repositoryCount": 1,
                    "epicCount": 1,
                    "featureCount": 1,
                    "workItemCount": 1,
                    "sessionCount": 1
                }
            }),
        ),
        response_envelope(
            protocol_version,
            json!({
                "type": "hierarchy_children",
                "value": {
                    "parent": { "kind": "workspace", "id": WORKSPACE_ID },
                    "children": [{
                        "kind": "repository",
                        "value": {
                            "id": REPOSITORY_ID,
                            "workspaceId": WORKSPACE_ID,
                            "slug": "fixture-repository",
                            "title": "Fixture Repository"
                        }
                    }]
                }
            }),
        ),
        response_envelope(
            protocol_version,
            json!({
                "type": "workspace_hierarchy",
                "value": {
                    "workspace": { "id": WORKSPACE_ID, "slug": "fixture-workspace", "title": "Fixture Workspace" },
                    "repositories": [{ "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" }],
                    "epics": [{
                        "epic": { "id": EPIC_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-epic", "title": "Fixture Epic" },
                        "repositoryIds": [REPOSITORY_ID]
                    }],
                    "features": [{
                        "feature": { "id": FEATURE_ID, "epicId": EPIC_ID, "slug": "fixture-feature", "title": "Fixture Feature" },
                        "repositoryIds": [REPOSITORY_ID]
                    }],
                    "workItems": [{
                        "workItem": { "id": WORK_ITEM_ID, "featureId": FEATURE_ID, "key": "WI-1", "slug": "fixture-work-item", "title": "Fixture Work item" },
                        "repositoryIds": [REPOSITORY_ID],
                        "status": "in_progress"
                    }],
                    "recentEntities": [{ "kind": "work_item", "id": WORK_ITEM_ID }],
                    "focusedEntity": { "kind": "work_item", "id": WORK_ITEM_ID }
                }
            }),
        ),
        response_envelope(
            protocol_version,
            json!({ "type": "board_views", "value": [board_view(1)] }),
        ),
        response_envelope(
            protocol_version,
            json!({ "type": "board_view", "value": board_view(1) }),
        ),
        response_envelope(
            protocol_version,
            json!({
                "type": "board_snapshot",
                "value": {
                    "workspace": {
                        "id": WORKSPACE_ID,
                        "slug": "fixture-workspace",
                        "title": "Fixture Workspace",
                        "planning_store_repository_id": REPOSITORY_ID
                    },
                    "repositories": [],
                    "epics": [],
                    "features": [],
                    "workItems": [],
                    "documents": [],
                    "checkouts": [],
                    "effectiveCheckouts": [],
                    "sessions": [],
                    "associations": []
                }
            }),
        ),
        response_envelope(
            protocol_version,
            json!({
                "type": "subscription_accepted",
                "value": {
                    "cursor": { "daemonInstanceId": DAEMON_ID, "sequence": 40 }
                }
            }),
        ),
        response_envelope(
            CURRENT_PROTOCOL_VERSION,
            json!({
                "type": "command_accepted",
                "value": { "code": "approve_feature" }
            }),
        ),
    ];
    if protocol_version == CURRENT_PROTOCOL_VERSION {
        results.insert(6, response_envelope(protocol_version, json!({
            "type": "board",
            "value": {
                "lanes": [{ "key": "in_progress", "title": "In progress", "position": 1, "totalCount": 1 }],
                "cards": [board_card()],
                "nextCursor": null,
                "totalCount": 1,
                "revision": 41
            }
        })));
        results.insert(7, response_envelope(protocol_version, json!({
            "type": "attention",
            "value": {
                "entries": [{
                    "owner": { "kind": "work_item", "id": WORK_ITEM_ID },
                    "title": "Fixture Work item",
                    "subtitle": "WI-1",
                    "repositories": [{ "id": REPOSITORY_ID, "workspaceId": WORKSPACE_ID, "slug": "fixture-repository", "title": "Fixture Repository" }],
                    "card": board_card(),
                    "reasons": [{ "code": "checkpoint_due", "rank": 5, "message": "Checkpoint evidence is due" }],
                    "revision": 3,
                    "availableActions": [],
                    "position": 1,
                    "totalCount": 1
                }],
                "nextCursor": null,
                "totalCount": 1,
                "revision": 41
            }
        })));
        results.insert(
            8,
            response_envelope(
                protocol_version,
                json!({ "type": "approval_queue", "value": approval_queue() }),
            ),
        );
        results.insert(
            9,
            response_envelope(
                protocol_version,
                json!({ "type": "feature_proposal", "value": feature_proposal() }),
            ),
        );
        results.insert(
            10,
            response_envelope(
                protocol_version,
                json!({ "type": "work_item_detail", "value": work_item_detail() }),
            ),
        );
        results.insert(
            11,
            response_envelope(
                protocol_version,
                json!({ "type": "repository_observability", "value": repository_observability() }),
            ),
        );
        results.insert(
            12,
            response_envelope(
                protocol_version,
                json!({ "type": "checkout_observability", "value": checkout_observability() }),
            ),
        );
        results.insert(
            13,
            response_envelope(
                protocol_version,
                json!({ "type": "session_observability", "value": session_observability() }),
            ),
        );
        results.insert(
            14,
            response_envelope(
                protocol_version,
                json!({ "type": "recovery_preview", "value": recovery_preview() }),
            ),
        );
    }
    results
}

fn typed_errors() -> Vec<Value> {
    ["info", "warning", "error", "fatal"]
        .into_iter()
        .enumerate()
        .map(|(index, severity)| {
            json!({
                "code": if index == 0 { "incompatible_command_version" } else { "typed_error" },
                "message": "A safe fixture error.",
                "severity": severity,
                "retryable": index == 1,
                "validationFields": if index == 2 {
                    json!([{
                        "field": "expectedRevision",
                        "code": "stale_revision",
                        "message": "The revision is stale."
                    }])
                } else {
                    json!([])
                },
                "staleRevision": if index == 2 { json!(40) } else { Value::Null },
                "currentRevision": if index == 2 { json!(41) } else { Value::Null },
                "reconciliationOwner": if index == 3 {
                    json!({ "kind": "feature", "id": FEATURE_ID })
                } else {
                    Value::Null
                },
                "correlationId": CORRELATION_ID,
                "resync": if index == 1 { resync("gap") } else { Value::Null }
            })
        })
        .collect()
}

fn partial_outcome() -> Value {
    json!({
        "owner": { "kind": "feature", "id": FEATURE_ID },
        "code": "planning_store_pending",
        "succeeded": false,
        "message": "The durable operation requires reconciliation.",
        "reconciliationRequired": true,
        "evidence": [{
            "code": "safe_evidence",
            "severity": "warning",
            "message": "Safe evidence is available.",
            "owner": { "kind": "feature", "id": FEATURE_ID }
        }]
    })
}

fn resync(reason: &str) -> Value {
    json!({
        "reason": reason,
        "workspaceId": WORKSPACE_ID,
        "authoritativeRevision": 41,
        "oldestReplayableSequence": 20,
        "requiredQueries": ["workspace_summary", "hierarchy_children", "board_snapshot"]
    })
}

fn server_messages(protocol_version: u32) -> Vec<Value> {
    let event = json!({
        "protocolVersion": protocol_version,
        "eventVersion": 1,
        "workspaceId": WORKSPACE_ID,
        "sequence": 42,
        "eventId": EVENT_ID,
        "occurredAt": "2026-08-30T12:00:01Z",
        "owner": { "kind": "feature", "id": FEATURE_ID },
        "entityRevision": 42,
        "kind": "partial_outcome_recorded",
        "payload": { "type": "partial_outcome", "value": { "outcome": partial_outcome() } },
        "invalidationScope": {
            "queries": ["workspace_summary", "hierarchy_children", "board_snapshot"],
            "owners": [{ "kind": "feature", "id": FEATURE_ID }]
        },
        "operationCorrelationId": CORRELATION_ID,
        "partialOutcomes": [partial_outcome()]
    });
    let response = response_envelope(
        protocol_version,
        json!({
            "type": "workspace_summary",
            "value": {
                "workspace": {
                    "id": WORKSPACE_ID,
                    "slug": "fixture-workspace",
                    "title": "Fixture Workspace"
                },
                "repositoryCount": 1,
                "epicCount": 1,
                "featureCount": 1,
                "workItemCount": 1,
                "sessionCount": 1
            }
        }),
    );
    vec![
        json!({ "type": "response", "value": response }),
        json!({ "type": "event", "value": event }),
        json!({
            "type": "heartbeat",
            "value": {
                "daemonInstanceId": DAEMON_ID,
                "workspaceId": WORKSPACE_ID,
                "revision": 42,
                "sentAt": "2026-08-30T12:00:02Z"
            }
        }),
        json!({ "type": "resync_required", "value": resync("gap") }),
    ]
}

fn discriminants() -> Value {
    json!({
        "operations": ["handshake", "query", "command", "subscribe"],
        "readQueries": ["workspace_summary", "hierarchy_children", "workspace_hierarchy", "board_views", "board_view", "board", "attention", "approval_queue", "feature_proposal", "work_item_detail", "repository_observability", "checkout_observability", "session_observability", "recovery_preview", "board_snapshot"],
        "responseResults": [
            "handshake",
            "workspace_summary",
            "hierarchy_children",
            "workspace_hierarchy",
            "board_views",
            "board_view",
            "board",
            "attention",
            "approval_queue",
            "feature_proposal",
            "work_item_detail",
            "repository_observability",
            "checkout_observability",
            "session_observability",
            "recovery_preview",
            "board_snapshot",
            "subscription_accepted",
            "command_accepted"
        ],
        "commandCodes": [
            "save_board_view",
            "approve_feature",
            "request_feature_revision",
            "reject_feature",
            "checkpoint_work_item",
            "start_session",
            "resume_session",
            "focus_session",
            "follow_up_session",
            "recover_session"
        ],
        "serverMessages": ["response", "event", "heartbeat", "resync_required"],
        "eventKinds": [
            "projection_changed",
            "board_view_saved",
            "native_sessions_refreshed",
            "partial_outcome_recorded",
            "checkout_changed",
            "session_liveness_changed"
            ,"proposal_changed"
            ,"work_item_changed"
        ],
        "eventPayloads": [
            { "type": "projection_changed", "value": {
                "entity": { "kind": "feature", "id": FEATURE_ID }
            }},
            { "type": "board_view_saved", "value": { "view": board_view(1) }},
            { "type": "native_sessions_refreshed", "value": { "sessionCount": 1 }},
            { "type": "partial_outcome", "value": { "outcome": partial_outcome() }}
            ,{ "type": "board_card_changed", "value": { "card": board_card() }}
            ,{ "type": "checkout_changed", "value": { "checkout": checkout_observability(), "cards": [board_card()] }}
            ,{ "type": "session_liveness_changed", "value": { "session": session_observability(), "recovery": recovery_preview(), "cards": [board_card()] }}
            ,{ "type": "proposal_changed", "value": { "proposal": feature_proposal(), "queueItem": approval_queue()["entries"][0] }}
            ,{ "type": "work_item_changed", "value": { "detail": work_item_detail(), "card": board_card() }}
        ],
        "resyncReasons": [
            "gap",
            "cursor_expired",
            "daemon_restarted",
            "incompatible_event",
            "heartbeat_lost"
        ],
        "errorSeverities": ["info", "warning", "error", "fatal"],
        "readQueryCodes": ["workspace_summary", "hierarchy_children", "workspace_hierarchy", "board_views", "board_view", "board", "attention", "approval_queue", "feature_proposal", "work_item_detail", "repository_observability", "checkout_observability", "session_observability", "recovery_preview", "board_snapshot"],
        "hierarchyRefs": [
            { "kind": "workspace", "id": WORKSPACE_ID },
            { "kind": "epic", "id": EPIC_ID },
            { "kind": "feature", "id": FEATURE_ID },
            { "kind": "work_item", "id": WORK_ITEM_ID }
        ],
        "entityRefs": [
            { "kind": "workspace", "id": WORKSPACE_ID },
            { "kind": "repository", "id": REPOSITORY_ID },
            { "kind": "epic", "id": EPIC_ID },
            { "kind": "feature", "id": FEATURE_ID },
            { "kind": "work_item", "id": WORK_ITEM_ID },
            { "kind": "session", "id": SESSION_ID }
        ],
        "hierarchyNodes": [
            { "kind": "repository", "value": {
                "id": REPOSITORY_ID,
                "workspaceId": WORKSPACE_ID,
                "slug": "fixture-repository",
                "title": "Fixture Repository"
            }},
            { "kind": "epic", "value": {
                "id": EPIC_ID,
                "workspaceId": WORKSPACE_ID,
                "slug": "fixture-epic",
                "title": "Fixture Epic"
            }},
            { "kind": "feature", "value": {
                "id": FEATURE_ID,
                "epicId": EPIC_ID,
                "slug": "fixture-feature",
                "title": "Fixture Feature"
            }},
            { "kind": "work_item", "value": {
                "id": WORK_ITEM_ID,
                "featureId": FEATURE_ID,
                "key": "WI-1",
                "slug": "fixture-work-item",
                "title": "Fixture Work item"
            }}
        ],
        "ownerProjections": [
            { "kind": "epic", "id": EPIC_ID },
            { "kind": "feature", "id": FEATURE_ID },
            { "kind": "work_item", "id": WORK_ITEM_ID }
        ],
        "providers": ["claude", "codex"],
        "workItemStatuses": [
            "backlog", "ready", "in_progress", "blocked", "review", "done", "cancelled"
        ],
        "workflowStates": [
            "draft",
            "worktree_pending",
            "planning_launch_pending",
            "planning_active",
            "proposal_ready",
            "awaiting_approval",
            "publishing",
            "planned",
            "work_item_launch_pending",
            "work_item_active",
            "reconciliation_required",
            "blocked",
            "paused",
            "completed",
            "cancelled"
        ],
        "checkoutAvailability": ["available", "missing", "deleted", "replaced"],
        "managedSessionRoles": [
            "epic_navigation",
            "feature_planning",
            "work_item_execution",
            "debugging",
            "review"
        ],
        "evidenceStates": ["current", "historical", "stale", "missing", "unknown", "conflict", "not_loaded"],
        "checkoutPurposes": ["feature_integration", "work_item_write", "writer_session", "read_only_shared", "unknown"],
        "checkoutPurposeSources": ["declared", "inherited", "override", "unknown"],
        "sessionBindingStates": ["pending", "current", "stopped", "reconciliation_required"],
        "sessionLiveStates": ["active", "idle", "stopped", "unknown", "system_error", "not_loaded"],
        "sessionRestoreStates": ["tracked", "removed", "not_tracked", "conflict"],
        "sessionResumabilities": ["validated", "preflight_passed", "unknown", "missing", "corrupt", "unsupported"],
        "primaryWriterEvidence": ["confirmed_primary", "confirmed_secondary", "not_applicable", "unknown", "conflict"],
        "recoveryDispositions": ["ready_present", "ready_recreate", "already_live", "conflict", "unresumable", "not_loaded"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_item_fixture_covers_durable_read_sections_and_keeps_opaque_checkpoint_mutation_closed()
    {
        let detail = serde_json::from_value::<WorkItemDetailProjection>(work_item_detail())
            .expect("Work-item detail projection");
        assert_eq!(detail.dependency_readiness, DependencyReadiness::Waiting);
        assert_eq!(detail.blockers.len(), 1);
        assert!(detail.decisions.entries.is_empty());
        assert!(detail.verification.entries.is_empty());
        assert_eq!(detail.checkpoint_history.len(), 1);
        assert_eq!(detail.sessions.len(), 1);
        assert_eq!(detail.checkouts.len(), 1);
        assert_eq!(
            detail.available_actions[0].code,
            CommandCode::CheckpointWorkItem
        );
        assert!(!detail.available_actions[0].available);
        assert_eq!(
            detail.available_actions[0]
                .unavailable_reason
                .as_ref()
                .map(|reason| reason.code.as_str()),
            Some("structured_checkpoint_unavailable")
        );
        assert!(matches!(
            CommandOperation::CheckpointWorkItem {
                work_item_id: WORK_ITEM_ID.parse().expect("Work-item ID")
            },
            CommandOperation::CheckpointWorkItem { .. }
        ));
    }

    #[test]
    fn deterministic_operational_fixtures_cover_scale_history_cardinality_and_uncertainty() {
        let repository =
            serde_json::from_value::<RepositoryObservabilityProjection>(repository_observability())
                .expect("repository projection");
        let repositories = (1..=100)
            .map(|index| {
                let mut projection = repository.clone();
                projection.repository.id = format!("30000000-0000-0000-0000-{index:012}")
                    .parse()
                    .expect("repository ID");
                projection.repository.slug = format!("service-{index:03}");
                projection.repository.title = format!("Service {index:03}");
                projection
            })
            .collect::<Vec<_>>();

        let current =
            serde_json::from_value::<CheckoutObservabilityProjection>(checkout_observability())
                .expect("checkout projection");
        let mut historical = current.clone();
        historical.display_paths.insert(
            0,
            ObservedDisplayPath {
                display_path: "worktrees/previous".to_owned(),
                state: EvidenceState::Historical,
                observed_from: "2026-08-29T12:00:00Z".to_owned(),
                observed_until: Some("2026-08-30T12:00:00Z".to_owned()),
            },
        );
        let mut missing = current.clone();
        missing.id = "b0000000-0000-0000-0000-000000000002"
            .parse()
            .expect("missing checkout ID");
        missing.availability = CheckoutAvailability::Missing;
        missing.reconciliation_evidence.state = EvidenceState::Conflict;
        let mut replaced = current.clone();
        replaced.id = "b0000000-0000-0000-0000-000000000003"
            .parse()
            .expect("replaced checkout ID");
        replaced.availability = CheckoutAvailability::Replaced;
        replaced.collision_evidence.state = EvidenceState::Conflict;
        let checkouts = [current, historical, missing, replaced];

        let base =
            serde_json::from_value::<SessionObservabilityProjection>(session_observability())
                .expect("session projection");
        let live_states = [
            SessionLiveState::Active,
            SessionLiveState::Idle,
            SessionLiveState::Stopped,
            SessionLiveState::Unknown,
            SessionLiveState::SystemError,
            SessionLiveState::NotLoaded,
        ];
        let roles = [
            ManagedSessionRole::EpicNavigation,
            ManagedSessionRole::FeaturePlanning,
            ManagedSessionRole::WorkItemExecution,
            ManagedSessionRole::Debugging,
            ManagedSessionRole::Review,
        ];
        let many_sessions = live_states
            .into_iter()
            .enumerate()
            .map(|(index, state)| {
                let mut session = base.clone();
                session.id = format!("70000000-0000-0000-0000-{:012}", index + 1)
                    .parse()
                    .expect("session ID");
                session.provider = if index % 2 == 0 {
                    Provider::Claude
                } else {
                    Provider::Codex
                };
                session.role = roles[index % roles.len()];
                session.liveness.state = state;
                session.liveness.stale = state == SessionLiveState::Unknown;
                if index % 3 == 0 {
                    session.authoritative_profile = None;
                    session.authoritative_model = None;
                    session.profile_evidence.state = EvidenceState::NotLoaded;
                }
                if state == SessionLiveState::NotLoaded {
                    session.resumability = SessionResumability::Missing;
                }
                session
            })
            .collect::<Vec<_>>();
        let session_cardinalities = [Vec::new(), vec![base], many_sessions];

        let already_live = serde_json::from_value::<RecoveryPreviewProjection>(recovery_preview())
            .expect("recovery projection");
        let mut unresumable = already_live.clone();
        unresumable.disposition = RecoveryDispositionProjection::Unresumable;
        let mut conflict = already_live.clone();
        conflict.disposition = RecoveryDispositionProjection::Conflict;
        conflict.conflicts.push(Diagnostic {
            code: "checkout_collision".to_owned(),
            severity: ErrorSeverity::Warning,
            message: "Checkout recovery evidence conflicts.".to_owned(),
            owner: None,
        });
        let recovery = [already_live, unresumable, conflict];

        assert_eq!(repositories.len(), 100);
        assert!(checkouts.iter().any(|checkout| {
            checkout
                .display_paths
                .iter()
                .any(|path| path.state == EvidenceState::Historical)
        }));
        assert_eq!(
            checkouts
                .iter()
                .map(|checkout| checkout.availability)
                .collect::<Vec<_>>(),
            vec![
                CheckoutAvailability::Available,
                CheckoutAvailability::Available,
                CheckoutAvailability::Missing,
                CheckoutAvailability::Replaced,
            ]
        );
        assert_eq!(
            session_cardinalities
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![0, 1, 6]
        );
        assert!(
            session_cardinalities[2]
                .iter()
                .any(|session| session.liveness.stale)
        );
        assert!(
            session_cardinalities[2]
                .iter()
                .any(|session| session.resumability == SessionResumability::Missing)
        );
        assert_eq!(
            recovery
                .iter()
                .map(|preview| preview.disposition)
                .collect::<Vec<_>>(),
            vec![
                RecoveryDispositionProjection::AlreadyLive,
                RecoveryDispositionProjection::Unresumable,
                RecoveryDispositionProjection::Conflict,
            ]
        );
        assert_eq!(recovery[2].conflicts.len(), 1);
    }

    #[test]
    fn current_and_previous_fixtures_round_trip_through_rust() {
        for version in [CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION] {
            let fixture = conformance_fixture(version);
            for request in fixture["readRequests"].as_array().expect("read requests") {
                let request =
                    serde_json::from_value::<RequestEnvelope>(request.clone()).expect("request");
                request.validate().expect("valid read request");
            }
            for request in fixture["incompatibleCommands"]
                .as_array()
                .expect("incompatible commands")
            {
                let request =
                    serde_json::from_value::<RequestEnvelope>(request.clone()).expect("command");
                request.validate().expect("valid command shape");
                assert_eq!(request.protocol_version, PREVIOUS_PROTOCOL_VERSION);
            }
            for response in fixture["responses"].as_array().expect("responses") {
                serde_json::from_value::<ResponseEnvelope>(response.clone()).expect("response");
            }
            for error in fixture["typedErrors"].as_array().expect("typed errors") {
                serde_json::from_value::<ProtocolError>(error.clone()).expect("typed error");
            }
            for message in fixture["serverMessages"]
                .as_array()
                .expect("server messages")
            {
                serde_json::from_value::<ServerMessage>(message.clone()).expect("server message");
            }
        }
    }

    #[test]
    fn fixtures_cover_every_published_discriminant() {
        let fixture = conformance_fixture(CURRENT_PROTOCOL_VERSION);
        let discriminants = &fixture["discriminants"];
        assert_eq!(
            discriminants["commandCodes"],
            serde_json::to_value(CommandCode::ALL).expect("command codes")
        );
        assert_eq!(
            discriminants["resyncReasons"],
            json!([
                "gap",
                "cursor_expired",
                "daemon_restarted",
                "incompatible_event",
                "heartbeat_lost"
            ])
        );
        assert_eq!(
            fixture["incompatibleCommands"].as_array().map(Vec::len),
            Some(10)
        );
        assert_eq!(fixture["typedErrors"].as_array().map(Vec::len), Some(4));
    }

    #[test]
    fn fixture_keys_exclude_sensitive_transport_and_machine_fields() {
        for version in [CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION] {
            assert_safe_keys(&conformance_fixture(version));
        }
    }

    fn assert_safe_keys(value: &Value) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
                    assert!(
                        ![
                            "token",
                            "credential",
                            "credentials",
                            "password",
                            "secret",
                            "path",
                            "paths",
                            "url",
                            "urls",
                            "socket",
                            "commandline",
                            "providercommand",
                            "internaldiagnostic",
                            "internaldiagnostics",
                        ]
                        .contains(&normalized.as_str()),
                        "forbidden fixture field {key}"
                    );
                    assert_safe_keys(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    assert_safe_keys(value);
                }
            }
            _ => {}
        }
    }
}
