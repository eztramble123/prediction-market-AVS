use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::{Item, Map};
use lavs_apis::id::TaskId;
use lavs_apis::interfaces::tasks::TaskMetadata;

pub const CONFIG: Item<Config> = Item::new("config");
pub const VOTES: Map<(TaskId, Addr), OperatorVote> = Map::new("operator_votes");
pub const TASKS: Map<TaskId, TaskMetadata> = Map::new("tasks");
pub const SLASHED_OPERATORS: Map<Addr, bool> = Map::new("slashed_operators");

#[cw_serde]
pub struct Config {
    pub threshold_percent: Decimal,
    pub allowed_spread: Decimal,
    pub slashable_spread: Decimal,
    pub operator_contract: Addr,
}

#[cw_serde]
pub struct OperatorVote {
    pub result: Decimal,
}

#[cw_serde]
pub struct TaskOption {
    pub power: Uint128,
}

#[cw_serde]
pub struct TaskResponse {
    pub task: TaskMetadata,
}