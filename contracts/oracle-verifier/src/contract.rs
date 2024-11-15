#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, Decimal,
    CosmosMsg, WasmMsg, Order,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, OperatorVote, SLASHED_OPERATORS, CONFIG, VOTES, TASKS, TaskResponse};
use lavs_apis::verifier_simple::{AllVotersResponse, TaskMetadata, TaskQueryMsg, VotingPowerResponse};

const CONTRACT_NAME: &str = "crates.io:oracle-verifier";
const CONTRACT_VERSION: &str = "1.0.0";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let operator_contract = deps.api.addr_validate(&msg.operator_contract)?;
    let config = Config {
        threshold_percent: msg.threshold_percent,
        allowed_spread: msg.allowed_spread,
        slashable_spread: msg.slashable_spread,
        operator_contract: operator_contract.clone(),
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SubmitVote { task_id, operator, result } => {
            execute::submit_vote(deps, env, info, task_id, operator, result)
        }
        ExecuteMsg::ProcessVotes { task_id } => {
            execute::process_votes(deps, env, info, task_id)
        }
        ExecuteMsg::SlashOperators { task_id } => {
            execute::slash_operators(deps, env, info, task_id)
        }
    }
}

pub mod execute {
    use super::*;
    use cosmwasm_std::Order;
    use lavs_apis::interfaces::voting::VoterInfo;

    pub fn submit_vote(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        task_id: TaskId,
        operator: String,
        result: Decimal,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;
        let operator_addr = deps.api.addr_validate(&operator)?;

        // Verify that the sender is a registered operator by querying the Mock Operators contract
        let voting_power_query = VotingPowerResponse {};
        let voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
            &config.operator_contract,
            &QueryMsg::VotingPowerAtHeight {
                address: operator.clone(),
                height: Some(env.block.height),
            },
        )?;

        if voting_power.power.is_zero() {
            return Err(ContractError::Unauthorized {});
        }

        // Check if the operator has already voted for this task
        if VOTES.may_load(deps.storage, (task_id.clone(), operator_addr.clone()))?.is_some() {
            return Err(ContractError::OperatorAlreadyVoted(operator.clone()));
        }

        // Record the vote
        let vote = OperatorVote { result };
        VOTES.save(deps.storage, (task_id.clone(), operator_addr.clone()), &vote)?;

        Ok(Response::new()
            .add_attribute("action", "submit_vote")
            .add_attribute("operator", operator)
            .add_attribute("task_id", task_id.to_string())
            .add_attribute("result", result.to_string()))
    }

    pub fn process_votes(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        task_id: TaskId,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        // Ensure that the caller is the Mock Operators contract
        if info.sender != config.operator_contract {
            return Err(ContractError::Unauthorized {});
        }

        // Fetch all votes for the task
        let votes: Vec<(Addr, OperatorVote)> = VOTES
            .range(deps.storage, (Bound::Inclusive(task_id.clone()),), None, Order::Ascending)
            .collect::<StdResult<Vec<_>>>()?;

        if votes.is_empty() {
            return Err(ContractError::VoteProcessingFailed);
        }

        // Calculate the median result
        let mut results: Vec<Decimal> = votes.iter().map(|(_, vote)| vote.result).collect();
        results.sort();
        let median = results[results.len() / 2];

        // Identify slashed operators
        let mut slashed = Vec::new();
        for (operator, vote) in votes.iter() {
            if (vote.result - median).abs() > config.allowed_spread {
                slashed.push(operator.clone());
            }
        }

        // Slash the operators
        for operator in slashed.iter() {
            SLASHED_OPERATORS.save(deps.storage, operator, &true)?;
            // Additional slashing logic can be implemented here (e.g., deducting tokens)
        }

        // Determine if the threshold is met
        let total_power = deps.querier.query_wasm_smart(
            &config.operator_contract,
            &QueryMsg::TotalPowerAtHeight {
                height: Some(env.block.height),
            },
        )?;
        let required_power = total_power.power * config.threshold_percent;

        // Calculate the aggregated voting power in favor
        let aggregated_power: Uint128 = votes
            .iter()
            .filter(|(_, vote)| vote.result >= median)
            .map(|(_, vote)| vote.result.into())
            .sum();

        let threshold_met = aggregated_power >= required_power;

        // Optionally, interact with the Task Queue to mark the task as complete
        if threshold_met {
            let task_info = TASKS
                .load(deps.storage, task_id.clone())
                .map_err(|_| ContractError::VoteProcessingFailed)?;

            // Mark task as complete by sending a message to Task Queue
            let mark_complete_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: task_info.task.task_contract.clone(),
                msg: to_binary(&TaskQueryMsg::MarkComplete { task_id: task_id.clone() })?,
                funds: vec![],
            });

            Ok(Response::new()
                .add_message(mark_complete_msg)
                .add_attribute("action", "process_votes")
                .add_attribute("task_id", task_id.to_string())
                .add_attribute("median", median.to_string())
                .add_attribute("threshold_met", threshold_met.to_string()))
        } else {
            Err(ContractError::ThresholdNotMet)
        }
    }

    pub fn slash_operators(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        task_id: TaskId,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        // Ensure that the caller is the Mock Operators contract
        if info.sender != config.operator_contract {
            return Err(ContractError::Unauthorized {});
        }

        // Fetch all slashed operators for the task
        let slashed_operators: Vec<Addr> = SLASHED_OPERATORS
            .range(deps.storage, None, None, Order::Ascending)
            .filter_map(|item| {
                let (addr, slashed) = item.ok()?;
                if slashed {
                    Some(addr)
                } else {
                    None
                }
            })
            .collect();

        // Implement slashing logic (e.g., deducting tokens)
        for operator in slashed_operators.iter() {
            // Example: Create a message to slash tokens from the operator
            // This is a placeholder and should be replaced with actual slashing implementation
            // let slash_msg = CosmosMsg::Custom(...);
            // messages.push(slash_msg);
        }

        // Clear the slashed operators after slashing
        for operator in slashed_operators.iter() {
            SLASHED_OPERATORS.remove(deps.storage, operator);
        }

        // Optionally, interact with the Task Queue to mark the task as complete
        let mark_complete_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "task-queue-address".to_string(), // Replace with actual Task Queue address
            msg: to_binary(&TaskQueryMsg::MarkComplete { task_id: task_id.clone() })?,
            funds: vec![],
        });

        Ok(Response::new()
            .add_message(mark_complete_msg)
            .add_attribute("action", "slash_operators")
            .add_attribute("task_id", task_id.to_string())
            .add_attribute("slashed_count", slashed_operators.len().to_string()))
    }
}

mod query {
    use super::*;

    pub fn voting_power(
        deps: Deps,
        env: Env,
        address: String,
        height: Option<u64>,
    ) -> StdResult<VotingPowerResponse> {
        let height = height.unwrap_or(env.block.height);
        let config = CONFIG.load(deps.storage)?;
        let query_msg = QueryMsg::VotingPowerAtHeight { address: address.clone(), height: Some(height) };
        let res: VotingPowerResponse = deps.querier.query_wasm_smart(&config.operator_contract, &query_msg)?;
        Ok(res)
    }

    pub fn total_power(
        deps: Deps,
        env: Env,
        height: Option<u64>,
    ) -> StdResult<TotalPowerResponse> {
        let height = height.unwrap_or(env.block.height);
        let config = CONFIG.load(deps.storage)?;
        let query_msg = QueryMsg::TotalPowerAtHeight { height: Some(height) };
        let res: TotalPowerResponse = deps.querier.query_wasm_smart(&config.operator_contract, &query_msg)?;
        Ok(res)
    }

    pub fn all_voters(
        deps: Deps,
        env: Env,
    ) -> StdResult<AllVotersResponse> {
        let config = CONFIG.load(deps.storage)?;
        let query_msg = QueryMsg::AllVoters {};
        let res: AllVotersResponse = deps.querier.query_wasm_smart(&config.operator_contract, &query_msg)?;
        Ok(res)
    }

    pub fn task_info(
        deps: Deps,
        env: Env,
        task_contract: String,
        task_id: TaskId,
    ) -> Result<TaskResponse, ContractError> {
        let task_contract_addr = deps.api.addr_validate(&task_contract)?;
        let query_msg = TaskQueryMsg::Task { id: task_id.clone() };
        let res: TaskMetadata = deps.querier.query_wasm_smart(&task_contract_addr, &query_msg)?;
        Ok(TaskResponse { task: res })
    }
}
