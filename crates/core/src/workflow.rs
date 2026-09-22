use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Draft,
    WorktreePending,
    PlanningLaunchPending,
    PlanningActive,
    ProposalReady,
    AwaitingApproval,
    Publishing,
    Planned,
    WorkItemLaunchPending,
    WorkItemActive,
    ReconciliationRequired,
    Blocked,
    Paused,
    Completed,
    Cancelled,
}

impl WorkflowState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        match self {
            Self::Draft => matches!(next, Self::WorktreePending | Self::Cancelled),
            Self::WorktreePending => matches!(
                next,
                Self::PlanningLaunchPending | Self::ReconciliationRequired | Self::Cancelled
            ),
            Self::PlanningLaunchPending => matches!(
                next,
                Self::PlanningActive | Self::ReconciliationRequired | Self::Cancelled
            ),
            Self::PlanningActive => matches!(
                next,
                Self::ProposalReady
                    | Self::ReconciliationRequired
                    | Self::Blocked
                    | Self::Paused
                    | Self::Cancelled
            ),
            Self::ProposalReady => matches!(
                next,
                Self::AwaitingApproval | Self::PlanningActive | Self::Cancelled
            ),
            Self::AwaitingApproval => matches!(
                next,
                Self::Publishing | Self::PlanningActive | Self::Cancelled
            ),
            Self::Publishing => matches!(
                next,
                Self::Planned | Self::ReconciliationRequired | Self::Cancelled
            ),
            Self::Planned => matches!(
                next,
                Self::WorkItemLaunchPending | Self::Completed | Self::Cancelled
            ),
            Self::WorkItemLaunchPending => matches!(
                next,
                Self::WorkItemActive | Self::ReconciliationRequired | Self::Cancelled
            ),
            Self::WorkItemActive => matches!(
                next,
                Self::WorkItemLaunchPending
                    | Self::ReconciliationRequired
                    | Self::Blocked
                    | Self::Paused
                    | Self::Completed
                    | Self::Cancelled
            ),
            Self::ReconciliationRequired => matches!(
                next,
                Self::PlanningActive
                    | Self::ProposalReady
                    | Self::AwaitingApproval
                    | Self::Publishing
                    | Self::Planned
                    | Self::WorkItemLaunchPending
                    | Self::WorkItemActive
                    | Self::Blocked
                    | Self::Paused
                    | Self::Cancelled
            ),
            Self::Blocked => matches!(
                next,
                Self::PlanningActive
                    | Self::WorkItemActive
                    | Self::ReconciliationRequired
                    | Self::Paused
                    | Self::Cancelled
            ),
            Self::Paused => matches!(
                next,
                Self::PlanningActive
                    | Self::WorkItemActive
                    | Self::ReconciliationRequired
                    | Self::Cancelled
            ),
            Self::Completed | Self::Cancelled => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowActor {
    User,
    Integration,
    Application,
    Reconciliation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    RepositoryInstructions,
    Epic,
    Feature,
    WorkItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperationKind {
    CreateWorktree,
    ReplaceWorktree,
    RestoreWorktree,
    MaterialiseDocuments,
    CommitDocuments,
    CloseWorktree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    Pending,
    Approved,
    Executing,
    Succeeded,
    Failed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSessionRole {
    WorkspacePlanning,
    EpicNavigation,
    FeaturePlanning,
    WorkItemExecution,
    Debugging,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBindingStatus {
    Pending,
    Current,
    Stopped,
    ReconciliationRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextActionKind {
    Actionable,
    Blocked,
    Paused,
    Review,
    Delivery,
}

#[cfg(test)]
mod tests {
    use super::{ManagedSessionRole, WorkflowState};

    #[test]
    fn workflow_transitions_fail_closed() {
        assert!(WorkflowState::Draft.can_transition_to(WorkflowState::WorktreePending));
        assert!(WorkflowState::WorkItemActive.can_transition_to(WorkflowState::Completed));
        assert!(!WorkflowState::PlanningActive.can_transition_to(WorkflowState::WorkItemActive));
        assert!(!WorkflowState::Completed.can_transition_to(WorkflowState::WorkItemActive));
    }

    #[test]
    fn workflow_contracts_have_stable_wire_names() {
        assert_eq!(
            serde_json::to_string(&WorkflowState::AwaitingApproval)
                .expect("workflow state should serialise"),
            "\"awaiting_approval\""
        );
        assert_eq!(
            serde_json::to_string(&ManagedSessionRole::WorkItemExecution)
                .expect("session role should serialise"),
            "\"work_item_execution\""
        );
        assert_eq!(
            serde_json::to_string(&ManagedSessionRole::WorkspacePlanning)
                .expect("session role should serialise"),
            "\"workspace_planning\""
        );
    }
}
