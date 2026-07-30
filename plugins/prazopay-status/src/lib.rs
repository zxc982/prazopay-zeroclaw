//! Read-only PrazoPay milestone inspection for ZeroClaw.

mod status;

pub use status::{
    inspect_account_response, parse_block_time_response, validate_request, AccountSnapshot,
    MilestoneStatus, MilestoneView, MonitorDecision, StatusError, StatusReport, StatusRequest,
};

#[cfg(target_family = "wasm")]
mod component {
    wit_bindgen::generate!({
        path: "wit/v0",
        world: "tool-plugin",
        features: ["plugins-wit-v0"],
    });

    use crate::{
        inspect_account_response, parse_block_time_response, validate_request, StatusRequest,
    };
    use exports::zeroclaw::plugin::plugin_info::Guest as PluginInfo;
    use exports::zeroclaw::plugin::tool::{Guest as Tool, ToolResult};
    use serde_json::json;
    use std::time::{Duration, Instant};
    use waki::Client;
    use zeroclaw::plugin::logging::{
        log_record, LogLevel, PluginAction, PluginEvent, PluginOutcome,
    };

    const RPC_URL: &str = "https://api.devnet.solana.com";
    const MAX_RESPONSE_BYTES: usize = 1_048_576;

    struct PrazoPayStatusPlugin;

    impl PluginInfo for PrazoPayStatusPlugin {
        fn plugin_name() -> String {
            "prazopay-status".to_string()
        }

        fn plugin_version() -> String {
            env!("CARGO_PKG_VERSION").to_string()
        }
    }

    impl Tool for PrazoPayStatusPlugin {
        fn name() -> String {
            "prazopay_status".to_string()
        }

        fn description() -> String {
            "Read one PrazoPay milestone from Solana devnet and return deterministic \
             state, deadlines, role-specific next actions, and a machine-readable \
             proactive monitoring decision. This tool is advisory only and never \
             accepts wallet material, signs, or submits transactions."
                .to_string()
        }

        fn parameters_schema() -> String {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "cluster": {
                        "const": "devnet",
                        "description": "PrazoPay inspection is restricted to devnet."
                    },
                    "milestone": {
                        "type": "string",
                        "minLength": 32,
                        "maxLength": 44,
                        "pattern": "^[1-9A-HJ-NP-Za-km-z]+$",
                        "description": "Base58 PrazoPay milestone PDA."
                    },
                    "alert_before_secs": {
                        "type": "integer",
                        "minimum": 30,
                        "maximum": 86400,
                        "default": 300,
                        "description": "Notify this many seconds before an actionable deadline boundary."
                    },
                    "poll_interval_secs": {
                        "type": "integer",
                        "minimum": 60,
                        "maximum": 3600,
                        "default": 300,
                        "description": "Actual heartbeat interval used to create sparse notification windows."
                    }
                },
                "required": ["cluster", "milestone"]
            })
            .to_string()
        }

        fn execute(args: String) -> Result<ToolResult, String> {
            let started = Instant::now();
            let request: StatusRequest = match serde_json::from_str(&args) {
                Ok(request) => request,
                Err(_) => return Ok(failure("ARGUMENT_JSON_INVALID")),
            };
            if let Err(error) = validate_request(&request) {
                return Ok(failure(&error.to_string()));
            }

            let account_body = match rpc_call(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getAccountInfo",
                "params": [
                    request.milestone,
                    {
                        "commitment": "finalized",
                        "encoding": "base64"
                    }
                ]
            })) {
                Ok(body) => body,
                Err(code) => return Ok(failure(code)),
            };
            let snapshot = match inspect_account_response(&account_body) {
                Ok(snapshot) => snapshot,
                Err(error) => return Ok(failure(&error.to_string())),
            };

            let block_time_body = match rpc_call(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "getBlockTime",
                "params": [snapshot.slot]
            })) {
                Ok(body) => body,
                Err(code) => return Ok(failure(code)),
            };
            let observed_at = match parse_block_time_response(&block_time_body) {
                Ok(block_time) => block_time,
                Err(error) => return Ok(failure(&error.to_string())),
            };
            let report = match snapshot.report(&request, observed_at) {
                Ok(report) => report,
                Err(error) => return Ok(failure(&error.to_string())),
            };
            let output = match serde_json::to_string(&report) {
                Ok(output) => output,
                Err(_) => return Ok(failure("RESULT_SERIALIZATION_FAILED")),
            };

            log_record(
                LogLevel::Info,
                &PluginEvent {
                    function_name: "prazopay_status::execute".to_string(),
                    action: PluginAction::Complete,
                    outcome: Some(PluginOutcome::Success),
                    duration_ms: Some(
                        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    ),
                    attrs: Some(
                        json!({
                            "slot": report.slot,
                            "status": report.status,
                            "revision_count": report.revision_count
                        })
                        .to_string(),
                    ),
                    message: "inspected PrazoPay milestone".to_string(),
                },
            );

            Ok(ToolResult {
                success: true,
                output,
                error: None,
            })
        }
    }

    fn rpc_call(payload: serde_json::Value) -> Result<Vec<u8>, &'static str> {
        let body = serde_json::to_vec(&payload).map_err(|_| "RPC_REQUEST_SERIALIZATION_FAILED")?;
        let response = Client::new()
            .post(RPC_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body)
            .connect_timeout(Duration::from_secs(8))
            .send()
            .map_err(|_| "RPC_NETWORK_FAILED")?;
        if !(200..300).contains(&response.status_code()) {
            return Err("RPC_HTTP_STATUS_INVALID");
        }
        if response
            .header("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err("RPC_RESPONSE_TOO_LARGE");
        }
        match response.body() {
            Ok(bytes) if bytes.len() <= MAX_RESPONSE_BYTES => Ok(bytes),
            Ok(_) => Err("RPC_RESPONSE_TOO_LARGE"),
            Err(_) => Err("RPC_BODY_READ_FAILED"),
        }
    }

    fn failure(code: &str) -> ToolResult {
        ToolResult {
            success: false,
            output: String::new(),
            error: Some(code.to_string()),
        }
    }

    export!(PrazoPayStatusPlugin);
}
