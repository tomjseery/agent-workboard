mod association;
mod conversation;
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
pub use identity::{
    AssociationEventId, ConversationId, DocumentReferenceId, GitOperationIntentId, LaunchIntentId,
    LaunchLeaseId, LiveObservationId, ManagedSessionId, RepositoryId, SessionBindingId, WorkItemId,
    WorkflowEventId, WorkflowRunId, WorktreeId,
};
pub use launch::{
    CommandSpec, LaunchSpecError, ManagedLaunchMode, ManagedLaunchRequest, ManagedLaunchSpec,
    ResumeLaunchSpec, TerminalKind, WORKBOARD_LAUNCH_TOKEN_ENV, sanitise_terminal_title,
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
