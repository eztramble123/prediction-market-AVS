use cosmwasm_std::StdError;
use thiserror::Error;
#[derive(Error, Debug)]
pub enum ContractError {
#[error("{0}")]
Std(#[from] StdError),
#[error("Unauthorized")]
Unauthorized,
#[error("Failed to submit vote to Oracle Verifier")]
SubmitVoteError,
#[error("Invalid Vote Result")]
InvalidVoteResult,
// Add any other custom errors you like here.
// Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}