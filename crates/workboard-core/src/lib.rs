mod association;
mod conversation;
mod hierarchy;
mod identity;
mod launch;
mod live;
mod workflow;

pub use association::{
    AssociationAction, AssociationAuthority, AssociationEvent, AssociationEventError,
    AssociationEventVersion, AssociationEvidence, AssociationEvidenceError,
    AssociationEvidenceKind, AssociationSource, AssociationTarget, AssociationTargetError,
    NewAssociationEvent, NewManualAssociationEvent, UnsupportedAssociationEventVersion,
};
pub use conversation::{ConversationRef, ConversationRefError, Tool};
pub use hierarchy::{
    CHECKOUT_READINESS_SCHEMA_VERSION, Checkout, CheckoutAccessMode, CheckoutAvailability,
    CheckoutEvidenceKind, CheckoutPathInterval, CheckoutPurpose, CheckoutReadiness,
    CheckoutReconciliationEvidence, DocumentRevision, EffectiveCheckout, Epic, Feature,
    HierarchyOwner, LaunchIntent, MarkdownDocument, NameError, NativeSession,
    NativeSessionAssociation, OperationIntent, Repository, RepositoryPath, RepositoryRemote,
    RestoreMembership, Slug, TerminalLayout, TerminalTab, WorkItem, WorkItemKey, WorkItemStatus,
    WorkflowEvent, WorkflowRun, Workspace, WorkspaceSnapshot,
};
pub use identity::{
    AssociationEventId, AssociationIntervalId, CheckoutId, CheckoutPathId, ConversationId,
    DocumentId, DocumentReferenceId, EpicId, FeatureId, GitOperationIntentId, ImportBatchId,
    LaunchIntentId, LaunchLeaseId, LiveObservationId, ManagedSessionId, ManagedSessionRequestId,
    OperationIntentId, RecoveryAttemptId, RepositoryId, RepositoryPathId, RestoreMembershipId,
    SessionBindingId, TerminalLayoutId, TerminalTabId, WorkItemCheckpointId, WorkItemId,
    WorkflowEventId, WorkflowRunId, WorkspaceId, WorktreeId,
};
pub use launch::{
    CommandSpec, LaunchSpecError, ManagedLaunchMode, ManagedLaunchRequest, ManagedLaunchSpec,
    ResumeLaunchSpec, TerminalKind, WORKBOARD_BUNDLE_ENV, WORKBOARD_CHECKOUT_ENV,
    WORKBOARD_LAUNCH_TOKEN_ENV, WORKBOARD_OWNER_ENV, WORKBOARD_REPOSITORY_ENV,
    WORKBOARD_SESSION_ROLE_ENV, WORKBOARD_WORKFLOW_TOKEN_ENV, sanitise_terminal_title,
};
pub use live::{
    ConversationLifecycle, LiveEvidenceSource, LiveStatus, ProcessIdentity, ProcessIdentityError,
    Resumability,
};
pub use workflow::{
    DocumentKind, GitOperationKind, IntentStatus, ManagedSessionRole, NextActionKind,
    SessionBindingStatus, WorkflowActor, WorkflowState,
};

pub const PRODUCT_NAME: &str = "Agent Workboard";
