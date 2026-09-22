use serde::{Deserialize, Serialize};

use crate::{
    CheckoutAccessMode, ConversationId, HierarchyOwner, LaunchProfile, ManagedSessionRole,
    WorkflowState,
};

pub const AVAILABLE_ACTIONS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailableActionKind {
    Prepare,
    Reconcile,
    Status,
    Continue,
    Submit,
    ApproveAndPublish,
    RequestRevision,
    Reject,
    Start,
    Resume,
    StartAnother,
    SendFollowUp,
    Checkpoint,
    Integrate,
    Cleanup,
    Evidence,
    Retry,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableAction {
    pub kind: AvailableActionKind,
    pub label: String,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub confirmation: Option<String>,
    pub target: HierarchyOwner,
    pub session_id: Option<ConversationId>,
    pub role: Option<ManagedSessionRole>,
    pub access_mode: Option<CheckoutAccessMode>,
    pub profile: Option<LaunchProfile>,
    pub operation_route: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableActions {
    pub schema_version: u32,
    pub owner: HierarchyOwner,
    pub workflow_state: Option<WorkflowState>,
    pub revision: u64,
    pub actions: Vec<AvailableAction>,
    pub diagnostics: Vec<String>,
}

impl AvailableAction {
    pub fn enabled(
        kind: AvailableActionKind,
        label: impl Into<String>,
        target: HierarchyOwner,
        operation_route: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            label: label.into(),
            enabled: true,
            disabled_reason: None,
            confirmation: None,
            target,
            session_id: None,
            role: None,
            access_mode: None,
            profile: None,
            operation_route: operation_route.into(),
        }
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.enabled = false;
        self.disabled_reason = Some(reason.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{AvailableAction, AvailableActionKind};
    use crate::{HierarchyOwner, WorkItemId};

    #[test]
    fn disabled_actions_retain_their_route_and_reason() {
        let owner = HierarchyOwner::WorkItem(WorkItemId::generate());
        let action =
            AvailableAction::enabled(AvailableActionKind::Start, "Start", owner, "work.start")
                .disabled("dependency is incomplete");

        assert!(!action.enabled);
        assert_eq!(
            action.disabled_reason.as_deref(),
            Some("dependency is incomplete")
        );
        assert_eq!(action.operation_route, "work.start");
    }
}
