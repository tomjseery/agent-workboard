use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn generate() -> Self {
                Self(Uuid::new_v4())
            }

            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    };
}

define_id!(RepositoryId);
define_id!(WorkspaceId);
define_id!(EpicId);
define_id!(FeatureId);
define_id!(WorkItemId);
define_id!(CheckoutId);
define_id!(WorktreeId);
define_id!(ConversationId);
define_id!(DocumentId);
define_id!(OperationIntentId);
define_id!(TerminalLayoutId);
define_id!(TerminalTabId);
define_id!(AssociationIntervalId);
define_id!(RepositoryPathId);
define_id!(CheckoutPathId);
define_id!(RestoreMembershipId);
define_id!(AssociationEventId);
define_id!(LaunchLeaseId);
define_id!(LiveObservationId);
define_id!(WorkflowRunId);
define_id!(WorkflowEventId);
define_id!(DocumentReferenceId);
define_id!(GitOperationIntentId);
define_id!(LaunchIntentId);
define_id!(SessionBindingId);
define_id!(ManagedSessionId);

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::RepositoryId;

    #[test]
    fn parses_and_formats_an_identity() {
        let value = "b8449405-c417-4f9d-8f06-2217f7b8a82a";
        let id = RepositoryId::from_str(value).expect("the UUID should be valid");

        assert_eq!(id.to_string(), value);
    }

    #[test]
    fn serialises_an_identity_as_a_uuid_string() {
        let id = RepositoryId::from_str("b8449405-c417-4f9d-8f06-2217f7b8a82a")
            .expect("the UUID should be valid");

        assert_eq!(
            serde_json::to_string(&id).expect("the ID should serialise"),
            "\"b8449405-c417-4f9d-8f06-2217f7b8a82a\""
        );
    }
}
