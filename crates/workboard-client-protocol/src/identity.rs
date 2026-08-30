use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[serde(transparent)]
        #[ts(type = "string")]
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

define_id!(RequestId);
define_id!(EventId);
define_id!(DaemonInstanceId);
define_id!(WorkspaceId);
define_id!(RepositoryId);
define_id!(RepositoryPathId);
define_id!(EpicId);
define_id!(FeatureId);
define_id!(WorkItemId);
define_id!(SessionId);
define_id!(CheckoutId);
define_id!(CheckoutPathId);
define_id!(DocumentId);
define_id!(AssociationId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum HierarchyRef {
    Workspace(WorkspaceId),
    Epic(EpicId),
    Feature(FeatureId),
    WorkItem(WorkItemId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum EntityRef {
    Workspace(WorkspaceId),
    Repository(RepositoryId),
    Epic(EpicId),
    Feature(FeatureId),
    WorkItem(WorkItemId),
    Session(SessionId),
}
