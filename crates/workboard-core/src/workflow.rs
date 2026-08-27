use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Draft,
    PlanningLaunchPending,
    PlanningActive,
    ProposalReady,
    Approved,
    WorktreeCreating,
    AwaitingPlanningStop,
    DeliveryLaunchPending,
    DeliveryActive,
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
            Self::Draft => matches!(next, Self::PlanningLaunchPending | Self::Cancelled),
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
                Self::Approved | Self::PlanningActive | Self::Cancelled
            ),
            Self::Approved => matches!(
                next,
                Self::WorktreeCreating | Self::ReconciliationRequired | Self::Cancelled
            ),
            Self::WorktreeCreating => matches!(
                next,
                Self::AwaitingPlanningStop | Self::ReconciliationRequired | Self::Cancelled
            ),
            Self::AwaitingPlanningStop => matches!(
                next,
                Self::DeliveryLaunchPending
                    | Self::ReconciliationRequired
                    | Self::Blocked
                    | Self::Cancelled
            ),
            Self::DeliveryLaunchPending => matches!(
                next,
                Self::DeliveryActive | Self::ReconciliationRequired | Self::Cancelled
            ),
            Self::DeliveryActive => matches!(
                next,
                Self::ReconciliationRequired
                    | Self::Blocked
                    | Self::Paused
                    | Self::Completed
                    | Self::Cancelled
            ),
            Self::ReconciliationRequired => matches!(
                next,
                Self::PlanningActive
                    | Self::ProposalReady
                    | Self::Approved
                    | Self::AwaitingPlanningStop
                    | Self::DeliveryLaunchPending
                    | Self::DeliveryActive
                    | Self::Blocked
                    | Self::Paused
                    | Self::Cancelled
            ),
            Self::Blocked => matches!(
                next,
                Self::PlanningActive
                    | Self::DeliveryActive
                    | Self::ReconciliationRequired
                    | Self::Paused
                    | Self::Cancelled
            ),
            Self::Paused => matches!(
                next,
                Self::PlanningActive
                    | Self::DeliveryActive
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
    Roadmap,
    Plan,
    Progress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperationKind {
    CreateWorktree,
    RestoreWorktree,
    MaterialiseDocuments,
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
    Planning,
    Delivery,
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
        assert!(WorkflowState::Draft.can_transition_to(WorkflowState::PlanningLaunchPending));
        assert!(WorkflowState::DeliveryActive.can_transition_to(WorkflowState::Completed));
        assert!(!WorkflowState::PlanningActive.can_transition_to(WorkflowState::DeliveryActive));
        assert!(!WorkflowState::Completed.can_transition_to(WorkflowState::DeliveryActive));
    }

    #[test]
    fn workflow_contracts_have_stable_wire_names() {
        assert_eq!(
            serde_json::to_string(&WorkflowState::AwaitingPlanningStop)
                .expect("workflow state should serialise"),
            "\"awaiting_planning_stop\""
        );
        assert_eq!(
            serde_json::to_string(&ManagedSessionRole::Delivery)
                .expect("session role should serialise"),
            "\"delivery\""
        );
    }
}
