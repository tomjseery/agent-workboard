#![forbid(unsafe_code)]

pub mod caller;
mod error;
pub mod git;
pub mod hooks;
pub mod integration;
mod workflow_contract;

pub use error::AppError;

pub const APPLICATION_API_VERSION: u32 = 1;
