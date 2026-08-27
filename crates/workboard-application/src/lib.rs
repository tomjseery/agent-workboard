#![forbid(unsafe_code)]

pub mod caller;
mod error;
pub mod git;
pub mod hooks;
pub mod integration;
pub mod legacy_import;
pub mod native_launch;
pub mod planning_store;
pub mod storage;
mod workflow_contract;
pub mod workspace;

pub use error::AppError;

pub const APPLICATION_API_VERSION: u32 = 1;
