use cosmwasm_schema::cw_serde;
use lavs_apis::interfaces::tasks::;
use lavs_apis::interfaces::voting::;
use cosmwasm_std::Decimal;

#[cw_serde]
pub struct InstantiateMsg {
    pub requestor: String,
    pub verifier: String, // Address of the Oracle Verifier contract
    pub timeout: u64,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
#[cw_orch(disable_fields_sorting)]
pub enum ExecuteMsg {
    /// Creates a new task
    Create {
        description: String,
        timeout: Option<u64>,
        payload: RequestType,
        options: Vec<String>,
        proposed_winner: String,
    },
    /// Completes a task with the verified result
    CompleteTask {
        task_id: TaskId,
        result: ResponseType,
    },
    /// Expires a task if not completed within the timeout
    ExpireTask {
        task_id: TaskId,
    },
}

#[cw_serde]
pub enum QueryMsg {
    /// Lists all open tasks with optional pagination
    ListOpen {
        start_after: Option<TaskId>,
        limit: Option<u32>,
    },
    /// Retrieves detailed information about a specific task
    TaskInfo {
        task_id: TaskId,
    },
}
