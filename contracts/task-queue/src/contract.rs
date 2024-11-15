#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, Decimal,
    CosmosMsg, WasmMsg, Order, OrderBy,
};
use cw2::set_contract_version;
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, Task, TASKS, CONFIG};
use lavs_apis::tasks::{ListOpenResponse, TaskMetadata};
use serde::Deserialize;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:task-queue";
const CONTRACT_VERSION: &str = "1.0.0";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::validate(deps, msg)?;
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
        ExecuteMsg::Create {
            description,
            timeout,
            payload,
            options,
            proposed_winner,
        } => execute::create_task(deps, env, info, description, timeout, payload, options, proposed_winner),
        ExecuteMsg::CompleteTask { task_id, result } => execute::complete_task(deps, env, info, task_id, result),
        ExecuteMsg::ExpireTask { task_id } => execute::expire_task(deps, env, info, task_id),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ListOpen { start_after, limit } => to_binary(&query::list_open(deps, env, start_after, limit)?),
        QueryMsg::TaskInfo { task_id } => to_binary(&query::task_info(deps, env, task_id)?),
    }
}

mod execute {
    use cw_utils::nonpayable;
    use lavs_apis::id::TaskId;

    use crate::state::{check_timeout, Timing};
    use crate::msg::CreateTaskMsg;

    use super::*;

    pub fn create_task(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        description: String,
        timeout: Option<u64>,
        payload: RequestType,
        options: Vec<String>,
        proposed_winner: String,
    ) -> Result<Response, ContractError> {
        nonpayable(&info)?;
        let mut config = CONFIG.load(deps.storage)?;
        let timeout = check_timeout(&config.timeout, timeout)?;
        config.requestor.check_requestor(&info)?;
        let task_id = TaskId::new(deps.storage);

        let task = Task::new(
            description,
            timeout,
            payload,
            options,
            proposed_winner,
        );

        TASKS.save(deps.storage, task_id.clone(), &task)?;

        Ok(Response::new()
            .add_attribute("action", "create_task")
            .add_attribute("task_id", task_id.to_string()))
    }

    pub fn complete_task(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        task_id: TaskId,
        result: ResponseType,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        // Ensure that only the Oracle Verifier can complete tasks
        if info.sender != config.verifier {
            return Err(ContractError::Unauthorized);
        }

        TASKS.update(deps.storage, task_id.clone(), |mut task| -> Result<_, ContractError> {
            task.complete(&env, result)?;
            Ok(task)
        })?;

        Ok(Response::new()
            .add_attribute("action", "complete_task")
            .add_attribute("task_id", task_id.to_string())
            .add_attribute("result", "verified"))
    }

    pub fn expire_task(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        task_id: TaskId,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        // Only requestor can expire tasks
        config.requestor.check_requestor(&info)?;

        TASKS.update(deps.storage, task_id.clone(), |mut task| -> Result<_, ContractError> {
            task.expire(&env)?;
            Ok(task)
        })?;

        Ok(Response::new()
            .add_attribute("action", "expire_task")
            .add_attribute("task_id", task_id.to_string()))
    }
}

mod query {
    use super::*;
    use cosmwasm_std::Order;

    pub fn list_open(
        deps: Deps,
        env: Env,
        start_after: Option<TaskId>,
        limit: Option<u32>,
    ) -> Result<ListOpenResponse, ContractError> {
        let limit = limit.unwrap_or(10) as usize;
        let tasks: StdResult<Vec<TaskMetadata>> = TASKS
            .range(deps.storage, None, None, Order::Ascending)
            .filter(|item| {
                if let Ok((_, task)) = item {
                    matches!(task.status, Status::Open {})
                } else {
                    false
                }
            })
            .skip_while(|item| {
                if let Ok((id, _)) = item {
                    if let Some(start_id) = &start_after {
                        return &id.0 <= &start_id.0;
                    }
                }
                false
            })
            .take(limit)
            .map(|item| {
                item.map(|(id, task)| TaskMetadata {
                    id,
                    description: task.description,
                    status: task.status,
                    timing: task.timing,
                    payload: task.payload.clone(),
                    result: task.result.clone(),
                })
            })
            .collect();

        Ok(ListOpenResponse { tasks: tasks? })
    }

    pub fn task_info(
        deps: Deps,
        env: Env,
        task_id: TaskId,
    ) -> Result<TaskMetadata, ContractError> {
        let task = TASKS.may_load(deps.storage, task_id.clone())?.ok_or(ContractError::TaskNotFound)?;

        Ok(TaskMetadata {
            id: task_id,
            description: task.description,
            status: task.status,
            timing: task.timing,
            payload: task.payload,
            result: task.result,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{Addr, from_binary, coins, Uint128};
    use lavs_apis::id::TaskId;
    use lavs_apis::tasks::{ResponseType, RequestType};

    #[test]
    fn test_instantiate_task_queue() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            requestor: "requestor".to_string(),
            verifier: "verifier".to_string(),
            timeout: 3600,
        };
        let info = mock_info("creator", &coins(1000, "earth"));
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert_eq!(res.attributes[0].value, "instantiate");
    }

    #[test]
    fn test_create_task() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            requestor: "requestor".to_string(),
            verifier: "verifier".to_string(),
            timeout: 3600,
        };
        let info = mock_info("requestor", &coins(1000, "earth"));
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert_eq!(res.attributes[0].value, "instantiate");
       
        let create_msg = ExecuteMsg::Create {
            description: "Will Team A win?".to_string(),
            timeout: Some(7200),
            payload: RequestType::Json("{\"event\":\"Team A vs Team B\"}".to_string()),
            options: vec!["Team A".to_string(), "Team B".to_string()],
            proposed_winner: "Team A".to_string(),
        };
        let res = execute(deps.as_mut(), mock_env(), info.clone(), create_msg).unwrap();
        assert_eq!(res.attributes.len(), 2);
        assert_eq!(res.attributes[0].value, "create_task");
        assert_eq!(res.attributes[1].value, "task_id");

        // Verify task creation
        let task_id = TaskId::new(1);
        let query_msg = QueryMsg::TaskInfo { task_id: task_id.clone() };
        let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
        let task: TaskMetadata = from_binary(&res).unwrap();
        assert_eq!(task.id, task_id);
        assert_eq!(task.description, "Will Team A win?".to_string());
        assert!(matches!(task.status, Status::Open {}));
    }

    #[test]
    fn test_complete_task_success() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            requestor: "requestor".to_string(),
            verifier: "verifier".to_string(),
            timeout: 3600,
        };
        let info = mock_info("requestor", &coins(1000, "earth"));
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // Create a task
        let create_msg = ExecuteMsg::Create {
            description: "Will Team A win?".to_string(),
            timeout: Some(7200),
            payload: RequestType::Json("{\"event\":\"Team A vs Team B\"}".to_string()),
            options: vec!["Team A".to_string(), "Team B".to_string()],
            proposed_winner: "Team A".to_string(),
        };
        execute(deps.as_mut(), mock_env(), info.clone(), create_msg).unwrap();

        // Complete the task as Oracle Verifier
        let complete_msg = ExecuteMsg::CompleteTask {
            task_id: TaskId::new(1),
            result: ResponseType::Json("{\"winner\":\"Team A\"}".to_string()),
        };
        let verifier_info = mock_info("verifier", &[]);
        let res = execute(deps.as_mut(), mock_env(), verifier_info, complete_msg).unwrap();
        assert_eq!(res.attributes.len(), 3);
        assert_eq!(res.attributes[0].value, "complete_task");
        assert_eq!(res.attributes[1].value, "task_id");
        assert_eq!(res.attributes[2].value, "verified");

        // Verify task completion
        let query_msg = QueryMsg::TaskInfo { task_id: TaskId::new(1) };
        let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
        let task: TaskMetadata = from_binary(&res).unwrap();
        assert!(matches!(task.status, Status::Completed { .. }));
        assert_eq!(task.result.unwrap(), ResponseType::Json("{\"winner\":\"Team A\"}".to_string()));
    }

    #[test]
    fn test_complete_task_unauthorized() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            requestor: "requestor".to_string(),
            verifier: "verifier".to_string(),
            timeout: 3600,
        };
        let info = mock_info("requestor", &coins(1000, "earth"));
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // Create a task
        let create_msg = ExecuteMsg::Create {
            description: "Will Team A win?".to_string(),
            timeout: Some(7200),
            payload: RequestType::Json("{\"event\":\"Team A vs Team B\"}".to_string()),
            options: vec!["Team A".to_string(), "Team B".to_string()],
            proposed_winner: "Team A".to_string(),
        };
        execute(deps.as_mut(), mock_env(), info.clone(), create_msg).unwrap();

        // Attempt to complete the task as an unauthorized user
        let complete_msg = ExecuteMsg::CompleteTask {
            task_id: TaskId::new(1),
            result: ResponseType::Json("{\"winner\":\"Team A\"}".to_string()),
        };
        let unauthorized_info = mock_info("intruder", &[]);
        let err = execute(deps.as_mut(), mock_env(), unauthorized_info, complete_msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized);
    }

    #[test]
    fn test_expire_task_success() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            requestor: "requestor".to_string(),
            verifier: "verifier".to_string(),
            timeout: 1, // 1 second for testing
        };
        let info = mock_info("requestor", &coins(1000, "earth"));
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // Create a task with a short timeout
        let create_msg = ExecuteMsg::Create {
            description: "Will Team A win?".to_string(),
            timeout: Some(1),
            payload: RequestType::Json("{\"event\":\"Team A vs Team B\"}".to_string()),
            options: vec!["Team A".to_string(), "Team B".to_string()],
            proposed_winner: "Team A".to_string(),
        };
        execute(deps.as_mut(), mock_env(), info.clone(), create_msg).unwrap();

        // Fast-forward time to trigger expiration
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(2);
        env.block.height += 2;

        // Expire the task
        let expire_msg = ExecuteMsg::ExpireTask {
            task_id: TaskId::new(1),
        };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), expire_msg).unwrap();
        assert_eq!(res.attributes.len(), 2);
        assert_eq!(res.attributes[0].value, "expire_task");
        assert_eq!(res.attributes[1].value, "task_id");

        // Verify task expiration
        let query_msg = QueryMsg::TaskInfo { task_id: TaskId::new(1) };
        let res = query(deps.as_ref(), env, query_msg).unwrap();
        let task: TaskMetadata = from_binary(&res).unwrap();
        assert!(matches!(task.status, Status::Expired {}));
    }

    #[test]
    fn test_expire_task_unauthorized() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            requestor: "requestor".to_string(),
            verifier: "verifier".to_string(),
            timeout: 3600,
        };
        let info = mock_info("requestor", &coins(1000, "earth"));
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // Create a task
        let create_msg = ExecuteMsg::Create {
            description: "Will Team A win?".to_string(),
            timeout: Some(7200),
            payload: RequestType::Json("{\"event\":\"Team A vs Team B\"}".to_string()),
            options: vec!["Team A".to_string(), "Team B".to_string()],
            proposed_winner: "Team A".to_string(),
        };
        execute(deps.as_mut(), mock_env(), info.clone(), create_msg).unwrap();

        // Attempt to expire the task as an unauthorized user
        let expire_msg = ExecuteMsg::ExpireTask {
            task_id: TaskId::new(1),
        };
        let unauthorized_info = mock_info("intruder", &[]);
        let err = execute(deps.as_mut(), mock_env(), unauthorized_info, expire_msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized);
    }

    #[test]
    fn test_complete_task_already_completed() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            requestor: "requestor".to_string(),
            verifier: "verifier".to_string(),
            timeout: 3600,
        };
        let info = mock_info("requestor", &coins(1000, "earth"));
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // Create a task
        let create_msg = ExecuteMsg::Create {
            description: "Will Team A win?".to_string(),
            timeout: Some(7200),
            payload: RequestType::Json("{\"event\":\"Team A vs Team B\"}".to_string()),
            options: vec!["Team A".to_string(), "Team B".to_string()],
            proposed_winner: "Team A".to_string(),
        };
        execute(deps.as_mut(), mock_env(), info.clone(), create_msg).unwrap();

        // Complete the task as Oracle Verifier
        let complete_msg = ExecuteMsg::CompleteTask {
            task_id: TaskId::new(1),
            result: ResponseType::Json("{\"winner\":\"Team A\"}".to_string()),
        };
        let verifier_info = mock_info("verifier", &[]);
        execute(deps.as_mut(), mock_env(), verifier_info.clone(), complete_msg).unwrap();

        // Attempt to complete the same task again
        let complete_again_msg = ExecuteMsg::CompleteTask {
            task_id: TaskId::new(1),
            result: ResponseType::Json("{\"winner\":\"Team B\"}".to_string()),
        };
        let res = execute(deps.as_mut(), mock_env(), verifier_info, complete_again_msg);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), ContractError::TaskCompleted);
    }
}