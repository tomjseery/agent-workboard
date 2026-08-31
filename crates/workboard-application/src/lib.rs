#![forbid(unsafe_code)]

pub mod board_projection;
pub mod caller;
pub mod checkout;
pub mod concertable_import;
mod error;
pub mod git;
pub mod hooks;
pub mod integration;
pub mod integration_service;
pub mod legacy_import;
pub mod native_launch;
pub mod native_sources;
pub mod operational_projection;
pub mod planning_store;
pub mod planning_workflow;
pub mod projection;
pub mod proposal_projection;
pub mod recovery;
pub mod session_launch;
pub mod storage;
pub mod work_item_projection;
mod workflow_contract;
pub mod workflow_operations;
pub mod workspace;

pub use error::AppError;

pub const APPLICATION_API_VERSION: u32 = 1;
