use cosmwasm_schema::cw_serde;
use lavs_apis::interfaces::tasks::TaskId;
use lavs_apis::interfaces::voting::VotingPower;
use cosmwasm_std::Decimal;

#[cw_serde]
pub struct InstantiateMsg {
    pub threshold_percent: Decimal,
    pub allowed_spread: Decimal,
    pub slashable_spread: Decimal,
    pub operator_contract: String, // Address of the Mock Operators contract
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
#[cw_orch(disable_fields_sorting)]
pub enum ExecuteMsg {
    /// Allows the Oracle Verifier to process votes for a specific task
    ProcessVotes {
        task_id: TaskId,
    },
    /// Slash operators who have deviated from the consensus
    SlashOperators {
        task_id: TaskId,
    },
    /// Receives a vote from an operator
    SubmitVote {
        task_id: TaskId,
        operator: String,
        result: Decimal,
    },
}

#[cw_serde]
pub enum QueryMsg {
    /// Query voting power of an operator at a specific height
    VotingPowerAtHeight {
        address: String,
        height: Option<u64>,
    },
    /// Query total voting power at a specific height
    TotalPowerAtHeight {
        height: Option<u64>,
    },
    /// Query all voters
    AllVoters {},
    /// Query task information
    TaskInfo {
        task_contract: String,
        task_id: TaskId,
    },
}