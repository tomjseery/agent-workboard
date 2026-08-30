#![forbid(unsafe_code)]

pub mod actions;
pub mod batch_launch;
pub mod caller;
pub mod capability_bundle;
pub mod checkout;
pub mod concertable_import;
mod error;
pub mod feature_integration;
pub mod follow_up;
pub mod git;
pub mod hooks;
pub mod integration;
pub mod integration_service;
pub mod legacy_import;
pub mod native_launch;
pub mod native_sources;
pub mod planning_store;
pub mod planning_workflow;
pub mod recovery;
pub mod session_launch;
pub mod storage;
pub mod work_projection;
mod workflow_contract;
pub mod workflow_operations;
pub mod workspace;
pub mod workspace_planning;

pub use error::AppError;

pub const APPLICATION_API_VERSION: u32 = 1;
