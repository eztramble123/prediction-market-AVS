#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
    Addr, Decimal, CosmosMsg, WasmMsg, Reply, SubMsg,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, InstantiateOperator, QueryMsg};
use crate::state::{Config, OpInfo, CONFIG};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:mock-operators";
const CONTRACT_VERSION: &str = "1.0.0";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let oracle_verifier_addr = deps.api.addr_validate(&msg.oracle_verifier)?;
    let mut total_power = Uint128::zero();

    let operators = msg
        .operators
        .into_iter()
        .map(|InstantiateOperator { addr, voting_power }| {
            let op = deps.api.addr_validate(&addr)?;
            let power = Uint128::from(voting_power);
            total_power += power;
            Ok(OpInfo { op, power })
        })
        .collect::<StdResult<Vec<>>>()?;

    let config = Config {
        operators,
        total_power,
        oracle_verifier: oracle_verifier_addr,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SubmitVote { task_id, result } => execute::submit_vote(deps, info, task_id, result),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::AllVoters {} => to_json_binary(&query::all_voters(deps)?),
        QueryMsg::VotingPowerAtHeight { address, height } => {
            to_json_binary(&query::voting_power(deps, height, address)?)
        }
        QueryMsg::TotalPowerAtHeight { height } => {
            to_json_binary(&query::total_power(deps, height)?)
        }
    }
}

mod execute {
    use super::*;
    use cosmwasm_std::{StdError, Reply, ReplyOn};

    pub fn submit_vote(
        deps: DepsMut,
        info: MessageInfo,
        task_id: TaskId,
        result: Decimal,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        // Ensure that the sender is a registered operator
        let operator = config
            .operators
            .iter()
            .find(|op| op.op == info.sender)
            .ok_or(ContractError::Unauthorized)?;

        // Construct the message to call the Oracle Verifier's SubmitVote function
        let verifier_contract = config.oracle_verifier;
        let submit_vote_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: verifier_contract.to_string(),
            msg: to_binary(&crate::msg::verifier::SubmitVoteMsg { task_id, result })?,
            funds: vec![],
        });

        // Optional: You can include additional logic or events here
        let response = Response::new()
            .add_message(submit_vote_msg)
            .add_attribute("action", "submit_vote")
            .add_attribute("operator", info.sender)
            .add_attribute("task_id", task_id.to_string())
            .add_attribute("result", result.to_string());

        Ok(response)
    }
}

mod query {
    use super::*;
    use lavs_apis::verifier_simple::{AllVotersResponse, TotalPowerResponse, VotingPowerResponse};

    pub fn voting_power(
        deps: Deps,
        height: Option<u64>,
        address: String,
    ) -> StdResult<VotingPowerResponse> {
        let addr = deps.api.addr_validate(&address)?;
        let config = CONFIG.load(deps.storage)?;
        let op = config.operators.iter().find(|op| op.op == addr);
        let power = op.map(|op| op.power).unwrap_or_default();

        Ok(VotingPowerResponse {
            power,
            height: height.unwrap_or(0),
        })
    }

    pub fn total_power(deps: Deps, height: Option<u64>) -> StdResult<TotalPowerResponse> {
        let config = CONFIG.load(deps.storage)?;
        Ok(TotalPowerResponse {
            power: config.total_power,
            height: height.unwrap_or(0),
        })
    }

    pub fn all_voters(deps: Deps) -> StdResult<AllVotersResponse> {
        let config = CONFIG.load(deps.storage)?;
        let voters = config
            .operators
            .iter()
            .map(|op| lavs_apis::verifier_simple::VoterInfo {
                power: op.power,
                address: op.op.to_string(),
            })
            .collect();

        Ok(AllVotersResponse { voters })
    }
}