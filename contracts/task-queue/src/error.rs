rust:contracts/task-queue/src/error.rs
use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;
#[derive(Error, Debug)]
pub enum ContractError {
#[error("{0}")]
Std(#[from] StdError),
#[error("Unauthorized")]
Unauthorized,
#[error("Insufficient payment: needed {0} {1}")]
InsufficientPayment(Uint128, String),
#[error("Invalid timeout configuration")]
InvalidTimeoutInfo,
#[error("Timeout too short: minimum {0} seconds")]
TimeoutTooShort(u64),
#[error("Timeout too long: maximum {0} seconds")]
TimeoutTooLong(u64),
#[error("Task not found")]
TaskNotFound,
#[error("Task already completed")]
TaskCompleted,
#[error("Task has already expired")]
TaskExpired,
#[error("Task is not expired yet")]
TaskNotExpired,
#[error("Failed to complete task")]
CompleteTaskError,
#[error("Failed to expire task")]
ExpireTaskError,
}