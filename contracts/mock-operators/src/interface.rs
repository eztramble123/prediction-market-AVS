se lavs_apis::interfaces::voting::SubmitVoteMsg;
use cosmwasm_schema::cw_serde;
#[cw_serde]
pub trait OracleVerifierInterface {
fn submit_vote(&self, task_id: TaskId, result: Decimal) -> SubmitVoteMsg;
}