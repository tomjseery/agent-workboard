use serde_json::{Value, json};
use ts_rs::{Config, TS};

use crate::{
    AssociationId, AvailableAction, BoardViewDefinition, BoardViewDensity, BoardViewFilters,
    BoardViewGrouping, BoardViewGroupingKind, BoardViewId, BoardViewLaneDefinition, BoardViewSort,
    BoardViewSortDirection, BoardViewSortField, CURRENT_PROTOCOL_VERSION, CheckoutAvailability,
    CheckoutId, CheckoutPathId, CommandCapability, CommandCode, CommandOperation, DaemonInstanceId,
    Diagnostic, DocumentId, EffectiveCheckoutProjection, EntityRef, EpicId, EpicReference,
    ErrorSeverity, EventCursor, EventEnvelope, EventId, EventKind, EventPayload, FeatureId,
    FeatureReference, HandshakeRequest, HandshakeResponse, Heartbeat, HierarchyChildren,
    HierarchyEpic, HierarchyFeature, HierarchyNode, HierarchyRef, HierarchyWorkItem,
    InvalidationScope, ManagedSessionRole, Operation, OwnerProjection, PREVIOUS_PROTOCOL_VERSION,
    PartialOutcome, ProtocolError, Provider, ReadQuery, ReadQueryCode, RepositoryId,
    RepositoryPathId, RepositoryReference, RequestEnvelope, RequestId, ResponseEnvelope,
    ResponseResult, ResyncReason, ResyncRequirement, ServerMessage, SessionId, SubscriptionRequest,
    UnavailableReason, ValidationField, WorkItemId, WorkItemReference, WorkItemStatus,
    WorkflowState, WorkspaceHierarchy, WorkspaceId, WorkspaceReference, WorkspaceSummary,
};

const REQUEST_ID: &str = "10000000-0000-0000-0000-000000000001";
const CORRELATION_ID: &str = "10000000-0000-0000-0000-000000000002";
const WORKSPACE_ID: &str = "20000000-0000-0000-0000-000000000001";
const REPOSITORY_ID: &str = "30000000-0000-0000-0000-000000000001";
const EPIC_ID: &str = "40000000-0000-0000-0000-000000000001";
const FEATURE_ID: &str = "50000000-0000-0000-0000-000000000001";
const WORK_ITEM_ID: &str = "60000000-0000-0000-0000-000000000001";
const SESSION_ID: &str = "70000000-0000-0000-0000-000000000001";
const DAEMON_ID: &str = "80000000-0000-0000-0000-000000000001";
const EVENT_ID: &str = "90000000-0000-0000-0000-000000000001";
const BOARD_VIEW_ID: &str = "a0000000-0000-0000-0000-000000000001";

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
    vec![
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
    ]
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
        json!({ "type": "request_feature_revision", "value": { "featureId": FEATURE_ID } }),
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
    vec![
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
    ]
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
        "readQueries": ["workspace_summary", "hierarchy_children", "workspace_hierarchy", "board_views", "board_view", "board_snapshot"],
        "responseResults": [
            "handshake",
            "workspace_summary",
            "hierarchy_children",
            "workspace_hierarchy",
            "board_views",
            "board_view",
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
            "partial_outcome_recorded"
        ],
        "eventPayloads": [
            { "type": "projection_changed", "value": {
                "entity": { "kind": "feature", "id": FEATURE_ID }
            }},
            { "type": "board_view_saved", "value": { "view": board_view(1) }},
            { "type": "native_sessions_refreshed", "value": { "sessionCount": 1 }},
            { "type": "partial_outcome", "value": { "outcome": partial_outcome() }}
        ],
        "resyncReasons": [
            "gap",
            "cursor_expired",
            "daemon_restarted",
            "incompatible_event",
            "heartbeat_lost"
        ],
        "errorSeverities": ["info", "warning", "error", "fatal"],
        "readQueryCodes": ["workspace_summary", "hierarchy_children", "workspace_hierarchy", "board_views", "board_view", "board_snapshot"],
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
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
