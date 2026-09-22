use workboard_core::{
    AVAILABLE_ACTIONS_SCHEMA_VERSION, AvailableAction, AvailableActionKind, AvailableActions,
    HierarchyOwner, WorkflowState,
};

pub fn workflow_actions(
    owner: HierarchyOwner,
    state: WorkflowState,
    revision: u64,
) -> AvailableActions {
    let actions = match state {
        WorkflowState::Draft => vec![
            action(
                owner,
                AvailableActionKind::Prepare,
                "Prepare checkout",
                "feature.prepare",
            ),
            action(
                owner,
                AvailableActionKind::Cancel,
                "Cancel",
                "feature.cancel",
            ),
        ],
        WorkflowState::WorktreePending => vec![
            action(
                owner,
                AvailableActionKind::Status,
                "Show checkout status",
                "feature.status",
            ),
            action(
                owner,
                AvailableActionKind::Reconcile,
                "Reconcile checkout",
                "feature.reconcile",
            ),
            action(
                owner,
                AvailableActionKind::Cancel,
                "Cancel",
                "feature.cancel",
            ),
        ],
        WorkflowState::PlanningLaunchPending => vec![
            action(
                owner,
                AvailableActionKind::Status,
                "Show launch status",
                "feature.status",
            ),
            action(
                owner,
                AvailableActionKind::Reconcile,
                "Reconcile launch",
                "feature.reconcile",
            ),
            action(
                owner,
                AvailableActionKind::Cancel,
                "Cancel",
                "feature.cancel",
            ),
        ],
        WorkflowState::PlanningActive => vec![
            action(
                owner,
                AvailableActionKind::Continue,
                "Continue planning",
                "feature.continue",
            ),
            action(
                owner,
                AvailableActionKind::Submit,
                "Submit proposal",
                "feature.submit",
            ),
            action(
                owner,
                AvailableActionKind::SendFollowUp,
                "Send follow-up",
                "session.follow_up",
            ),
            action(
                owner,
                AvailableActionKind::Cancel,
                "Cancel",
                "feature.cancel",
            ),
        ],
        WorkflowState::ProposalReady => vec![
            action(
                owner,
                AvailableActionKind::Continue,
                "Review proposal",
                "feature.open",
            ),
            action(
                owner,
                AvailableActionKind::RequestRevision,
                "Request revision",
                "feature.revise",
            ),
            action(
                owner,
                AvailableActionKind::Reject,
                "Reject",
                "feature.reject",
            ),
        ],
        WorkflowState::AwaitingApproval => vec![
            action(
                owner,
                AvailableActionKind::ApproveAndPublish,
                "Approve and publish",
                "feature.approve_publish",
            ),
            action(
                owner,
                AvailableActionKind::RequestRevision,
                "Request revision",
                "feature.revise",
            ),
            action(
                owner,
                AvailableActionKind::Reject,
                "Reject",
                "feature.reject",
            ),
        ],
        WorkflowState::Publishing => vec![
            action(
                owner,
                AvailableActionKind::Status,
                "Show publication status",
                "feature.status",
            ),
            action(
                owner,
                AvailableActionKind::Retry,
                "Retry publication",
                "feature.publish",
            ),
        ],
        WorkflowState::Planned => vec![
            action(
                owner,
                AvailableActionKind::Start,
                "Start ready Work item",
                "work.start",
            ),
            action(
                owner,
                AvailableActionKind::Status,
                "Show Work-item readiness",
                "feature.open",
            ),
        ],
        WorkflowState::WorkItemLaunchPending => vec![
            action(
                owner,
                AvailableActionKind::Status,
                "Show launch status",
                "work.open",
            ),
            action(
                owner,
                AvailableActionKind::Reconcile,
                "Reconcile launch",
                "work.reconcile",
            ),
        ],
        WorkflowState::WorkItemActive => vec![
            action(
                owner,
                AvailableActionKind::Continue,
                "Continue Work item",
                "work.continue",
            ),
            action(
                owner,
                AvailableActionKind::StartAnother,
                "Start another",
                "work.start",
            ),
            action(
                owner,
                AvailableActionKind::SendFollowUp,
                "Send follow-up",
                "session.follow_up",
            ),
            action(
                owner,
                AvailableActionKind::Checkpoint,
                "Record checkpoint",
                "workflow.checkpoint",
            ),
            action(
                owner,
                AvailableActionKind::Integrate,
                "Integrate accepted work",
                "work.integrate",
            ),
        ],
        WorkflowState::ReconciliationRequired => vec![
            action(
                owner,
                AvailableActionKind::Reconcile,
                "Reconcile",
                "feature.reconcile",
            ),
            action(
                owner,
                AvailableActionKind::Status,
                "Show reconciliation status",
                "feature.status",
            ),
            action(
                owner,
                AvailableActionKind::Cancel,
                "Cancel",
                "feature.cancel",
            ),
        ],
        WorkflowState::Blocked => vec![
            action(
                owner,
                AvailableActionKind::Status,
                "Show blockers",
                "work.open",
            ),
            action(
                owner,
                AvailableActionKind::Continue,
                "Continue when unblocked",
                "work.continue",
            ),
            action(
                owner,
                AvailableActionKind::Checkpoint,
                "Update blocker",
                "workflow.checkpoint",
            ),
        ],
        WorkflowState::Paused => vec![
            action(
                owner,
                AvailableActionKind::Continue,
                "Resume workflow",
                "work.continue",
            ),
            action(
                owner,
                AvailableActionKind::Status,
                "Show paused state",
                "work.open",
            ),
            action(
                owner,
                AvailableActionKind::Cancel,
                "Cancel",
                "feature.cancel",
            ),
        ],
        WorkflowState::Completed => vec![
            action(
                owner,
                AvailableActionKind::Evidence,
                "Show completion evidence",
                "work.open",
            ),
            action(
                owner,
                AvailableActionKind::Cleanup,
                "Clean up checkout",
                "work.cleanup",
            ),
        ],
        WorkflowState::Cancelled => vec![
            action(
                owner,
                AvailableActionKind::Evidence,
                "Show cancellation evidence",
                "feature.open",
            ),
            action(
                owner,
                AvailableActionKind::Cleanup,
                "Clean up checkout",
                "feature.cleanup",
            ),
        ],
    };
    AvailableActions {
        schema_version: AVAILABLE_ACTIONS_SCHEMA_VERSION,
        owner,
        workflow_state: Some(state),
        revision,
        actions,
        diagnostics: Vec::new(),
    }
}

fn action(
    owner: HierarchyOwner,
    kind: AvailableActionKind,
    label: &str,
    route: &str,
) -> AvailableAction {
    AvailableAction::enabled(kind, label, owner, route)
}

#[cfg(test)]
mod tests {
    use workboard_core::{AvailableActionKind, FeatureId, HierarchyOwner, WorkflowState};

    use super::workflow_actions;

    #[test]
    fn every_workflow_state_projects_actions_and_approval_order_is_stable() {
        let owner = HierarchyOwner::Feature(FeatureId::generate());
        for state in [
            WorkflowState::Draft,
            WorkflowState::WorktreePending,
            WorkflowState::PlanningLaunchPending,
            WorkflowState::PlanningActive,
            WorkflowState::ProposalReady,
            WorkflowState::AwaitingApproval,
            WorkflowState::Publishing,
            WorkflowState::Planned,
            WorkflowState::WorkItemLaunchPending,
            WorkflowState::WorkItemActive,
            WorkflowState::ReconciliationRequired,
            WorkflowState::Blocked,
            WorkflowState::Paused,
            WorkflowState::Completed,
            WorkflowState::Cancelled,
        ] {
            assert!(!workflow_actions(owner, state, 1).actions.is_empty());
        }
        let approval = workflow_actions(owner, WorkflowState::AwaitingApproval, 7);
        assert_eq!(
            approval
                .actions
                .iter()
                .map(|action| action.kind)
                .collect::<Vec<_>>(),
            [
                AvailableActionKind::ApproveAndPublish,
                AvailableActionKind::RequestRevision,
                AvailableActionKind::Reject,
            ]
        );
    }
}
