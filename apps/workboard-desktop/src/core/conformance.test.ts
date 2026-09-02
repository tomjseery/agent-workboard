import { describe, expect, it } from "vitest";

import current from "./generated/conformance-current.json";
import previous from "./generated/conformance-previous.json";
import type {
  CheckoutAvailability,
  CheckoutPurpose,
  CheckoutPurposeSource,
  CommandCode,
  CommandOperation,
  DependencyReadiness,
  EntityRef,
  ErrorSeverity,
  EvidenceState,
  EventKind,
  EventPayload,
  HierarchyNode,
  HierarchyRef,
  ManagedSessionRole,
  Operation,
  OwnerProjection,
  PrimaryWriterEvidence,
  Provider,
  QueryRequest,
  ReadQuery,
  ReadQueryCode,
  RecoveryDispositionProjection,
  ResponseEnvelope,
  ResponseResult,
  ResyncReason,
  ServerMessage,
  SessionBindingState,
  SessionLiveState,
  SessionRestoreState,
  SessionResumability,
  SubscribeRequest,
  WorkflowState,
  WorkItemStatus,
} from "./generated";

const deserialize = <T>(value: unknown): T =>
  JSON.parse(JSON.stringify(value)) as T;

describe("generated protocol conformance", () => {
  it("round-trips typed frontend requests through the Rust fixtures", () => {
    const workspaceSummary = {
      workspaceId: "20000000-0000-0000-0000-000000000001",
      query: { type: "workspace_summary" },
    } satisfies QueryRequest;
    const hierarchyChildren = {
      workspaceId: workspaceSummary.workspaceId,
      query: {
        type: "hierarchy_children",
        value: {
          parent: { kind: "workspace", id: workspaceSummary.workspaceId },
        },
      },
    } satisfies QueryRequest;
    const subscribe = {
      type: "start",
      value: {
        workspaceId: workspaceSummary.workspaceId,
        cursor: {
          daemonInstanceId: "80000000-0000-0000-0000-000000000001",
          sequence: 40,
        },
      },
    } satisfies SubscribeRequest;

    expect(workspaceSummary).toEqual(current.frontendRequests.workspaceSummary);
    expect(hierarchyChildren).toEqual(current.frontendRequests.hierarchyChildren);
    expect(subscribe).toEqual(current.frontendRequests.subscribe);
    expect(deserialize<ResponseEnvelope>(current.responses[1]).result?.type).toBe(
      "workspace_summary",
    );
    expect(deserialize<ResponseEnvelope>(previous.responses[1]).result?.type).toBe(
      "workspace_summary",
    );
    expect(previous.responses[1].futureOptionalReadField).toBe("ignored");
    expect(
      (previous.responses[1].result.value as { futureOptionalProjectionField?: boolean }).futureOptionalProjectionField,
    ).toBe(true);
  });

  it("covers every generated discriminant", () => {
    const commandCodes = [
      "save_board_view",
      "approve_feature",
      "request_feature_revision",
      "reject_feature",
      "checkpoint_work_item",
      "start_session",
      "resume_session",
      "focus_session",
      "follow_up_session",
      "recover_session",
    ] satisfies CommandCode[];
    const operations = [
      "handshake",
      "query",
      "command",
      "subscribe",
    ] satisfies Array<Operation["type"]>;
    const readQueries = [
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
    ] satisfies Array<ReadQuery["type"]>;
    const responseResults = [
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
      "command_accepted",
    ] satisfies Array<ResponseResult["type"]>;
    const serverMessages = [
      "response",
      "event",
      "heartbeat",
      "resync_required",
    ] satisfies Array<ServerMessage["type"]>;
    const eventKinds = [
      "projection_changed",
      "board_view_saved",
      "native_sessions_refreshed",
      "partial_outcome_recorded",
      "checkout_changed",
      "session_liveness_changed",
      "proposal_changed",
      "work_item_changed",
    ] satisfies EventKind[];
    const eventPayloads = [
      "projection_changed",
      "board_view_saved",
      "native_sessions_refreshed",
      "partial_outcome",
      "board_card_changed",
      "checkout_changed",
      "session_liveness_changed",
      "proposal_changed",
      "work_item_changed",
    ] satisfies Array<EventPayload["type"]>;
    const resyncReasons = [
      "gap",
      "cursor_expired",
      "daemon_restarted",
      "incompatible_event",
      "heartbeat_lost",
    ] satisfies ResyncReason[];
    const errorSeverities = [
      "info",
      "warning",
      "error",
      "fatal",
    ] satisfies ErrorSeverity[];
    const readQueryCodes = [
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
    ] satisfies ReadQueryCode[];
    const providers = ["claude", "codex"] satisfies Provider[];
    const dependencyReadiness = [
      "ready",
      "waiting",
      "blocked",
      "complete",
    ] satisfies DependencyReadiness[];
    const statuses = [
      "backlog",
      "ready",
      "in_progress",
      "blocked",
      "review",
      "done",
      "cancelled",
    ] satisfies WorkItemStatus[];
    const workflowStates = [
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
      "cancelled",
    ] satisfies WorkflowState[];
    const checkoutAvailability = [
      "available",
      "missing",
      "deleted",
      "replaced",
    ] satisfies CheckoutAvailability[];
    const managedSessionRoles = [
      "epic_navigation",
      "feature_planning",
      "work_item_execution",
      "debugging",
      "review",
    ] satisfies ManagedSessionRole[];
    const evidenceStates = ["current", "historical", "stale", "missing", "unknown", "conflict", "not_loaded"] satisfies EvidenceState[];
    const checkoutPurposes = ["feature_integration", "work_item_write", "writer_session", "read_only_shared", "unknown"] satisfies CheckoutPurpose[];
    const checkoutPurposeSources = ["declared", "inherited", "override", "unknown"] satisfies CheckoutPurposeSource[];
    const sessionBindingStates = ["pending", "current", "stopped", "reconciliation_required"] satisfies SessionBindingState[];
    const sessionLiveStates = ["active", "idle", "stopped", "unknown", "system_error", "not_loaded"] satisfies SessionLiveState[];
    const sessionRestoreStates = ["tracked", "removed", "not_tracked", "conflict"] satisfies SessionRestoreState[];
    const sessionResumabilities = ["validated", "preflight_passed", "unknown", "missing", "corrupt", "unsupported"] satisfies SessionResumability[];
    const primaryWriterEvidence = ["confirmed_primary", "confirmed_secondary", "not_applicable", "unknown", "conflict"] satisfies PrimaryWriterEvidence[];
    const recoveryDispositions = ["ready_present", "ready_recreate", "already_live", "conflict", "unresumable", "not_loaded"] satisfies RecoveryDispositionProjection[];
    const commandOperations = current.incompatibleCommands.map(
      ({ operation }) => operation.value,
    ) as CommandOperation[];

    expect(current.discriminants.commandCodes).toEqual(commandCodes);
    expect(commandOperations.map(({ type }) => type)).toEqual(commandCodes);
    expect(current.discriminants.operations).toEqual(operations);
    expect(current.discriminants.readQueries).toEqual(readQueries);
    expect(current.discriminants.responseResults).toEqual(responseResults);
    expect(current.discriminants.serverMessages).toEqual(serverMessages);
    expect(current.discriminants.eventKinds).toEqual(eventKinds);
    expect(
      current.discriminants.eventPayloads.map(({ type }) => type),
    ).toEqual(eventPayloads);
    expect(current.discriminants.resyncReasons).toEqual(resyncReasons);
    expect(current.discriminants.errorSeverities).toEqual(errorSeverities);
    expect(current.discriminants.readQueryCodes).toEqual(readQueryCodes);
    expect(current.discriminants.providers).toEqual(providers);
    expect(current.discriminants.dependencyReadiness).toEqual(dependencyReadiness);
    expect(current.discriminants.workItemStatuses).toEqual(statuses);
    expect(current.discriminants.workflowStates).toEqual(workflowStates);
    expect(current.discriminants.checkoutAvailability).toEqual(
      checkoutAvailability,
    );
    expect(current.discriminants.managedSessionRoles).toEqual(
      managedSessionRoles,
    );
    expect(current.discriminants.evidenceStates).toEqual(evidenceStates);
    expect(current.discriminants.checkoutPurposes).toEqual(checkoutPurposes);
    expect(current.discriminants.checkoutPurposeSources).toEqual(checkoutPurposeSources);
    expect(current.discriminants.sessionBindingStates).toEqual(sessionBindingStates);
    expect(current.discriminants.sessionLiveStates).toEqual(sessionLiveStates);
    expect(current.discriminants.sessionRestoreStates).toEqual(sessionRestoreStates);
    expect(current.discriminants.sessionResumabilities).toEqual(sessionResumabilities);
    expect(current.discriminants.primaryWriterEvidence).toEqual(primaryWriterEvidence);
    expect(current.discriminants.recoveryDispositions).toEqual(recoveryDispositions);

    deserialize<HierarchyRef[]>(current.discriminants.hierarchyRefs);
    deserialize<EntityRef[]>(current.discriminants.entityRefs);
    deserialize<HierarchyNode[]>(current.discriminants.hierarchyNodes);
    deserialize<OwnerProjection[]>(current.discriminants.ownerProjections);
  });

  it("keeps typed errors, gaps, partial outcomes, and version skew compatible", () => {
    expect(current.typedErrors.map(({ severity }) => severity)).toEqual([
      "info",
      "warning",
      "error",
      "fatal",
    ]);
    expect(
      current.serverMessages.find(({ type }) => type === "resync_required")?.value
        .reason,
    ).toBe("gap");
    expect(current.partialOutcome).toMatchObject({
      succeeded: false,
      reconciliationRequired: true,
    });
    expect(previous.protocolVersion).toBe(7);
    expect(current.protocolVersion).toBe(8);
  });
});
