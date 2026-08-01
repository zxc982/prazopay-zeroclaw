//! Read-only PrazoPay Agreement inspection for ZeroClaw.

mod agreement;

pub use agreement::{
    inspect_agreement_account_response, parse_agreement_block_time_response,
    validate_agreement_request, AgreementAccountSnapshot, AgreementMonitorDecision,
    AgreementStatus, AgreementStatusError, AgreementStatusReport, AgreementStatusRequest,
    AgreementView,
};

#[cfg(target_family = "wasm")]
mod component {
    wit_bindgen::generate!({
        path: "../prazopay-status/wit/v0",
        world: "tool-plugin",
        features: ["plugins-wit-v0"],
    });

    use crate::{
        inspect_agreement_account_response, parse_agreement_block_time_response,
        validate_agreement_request, AgreementStatusRequest,
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

    struct PrazoPayAgreementStatusPlugin;

    impl PluginInfo for PrazoPayAgreementStatusPlugin {
        fn plugin_name() -> String {
            "prazopay-agreement-status".to_string()
        }

        fn plugin_version() -> String {
            env!("CARGO_PKG_VERSION").to_string()
        }
    }

    impl Tool for PrazoPayAgreementStatusPlugin {
        fn name() -> String {
            "prazopay_agreement_status".to_string()
        }

        fn description() -> String {
            "Read one PrazoPay protocol-v2 Agreement from Solana devnet and return \
             deterministic acceptance, funding, expiry, and proactive monitoring \
             facts. This tool is advisory and never accepts wallet material, signs, \
             or submits transactions."
                .to_string()
        }

        fn parameters_schema() -> String {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "cluster": {
                        "const": "devnet",
                        "description": "PrazoPay Agreement inspection is restricted to devnet."
                    },
                    "agreement": {
                        "type": "string",
                        "minLength": 32,
                        "maxLength": 44,
                        "pattern": "^[1-9A-HJ-NP-Za-km-z]+$",
                        "description": "Base58 PrazoPay v2 Agreement PDA."
                    },
                    "alert_before_secs": {
                        "type": "integer",
                        "minimum": 30,
                        "maximum": 86400,
                        "default": 300,
                        "description": "Notify this many seconds before proposal expiry."
                    },
                    "poll_interval_secs": {
                        "type": "integer",
                        "minimum": 60,
                        "maximum": 3600,
                        "default": 300,
                        "description": "Heartbeat cadence used to create sparse notification windows."
                    }
                },
                "required": ["cluster", "agreement"]
            })
            .to_string()
        }

        fn execute(args: String) -> Result<ToolResult, String> {
            let started = Instant::now();
            let request: AgreementStatusRequest = match serde_json::from_str(&args) {
                Ok(request) => request,
                Err(_) => return Ok(failure("ARGUMENT_JSON_INVALID")),
            };
            if let Err(error) = validate_agreement_request(&request) {
                return Ok(failure(&error.to_string()));
            }

            let account_body = match rpc_call(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getAccountInfo",
                "params": [
                    request.agreement,
                    {"commitment": "finalized", "encoding": "base64"}
                ]
            })) {
                Ok(body) => body,
                Err(code) => return Ok(failure(code)),
            };
            let snapshot = match inspect_agreement_account_response(&account_body) {
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
            let observed_at = match parse_agreement_block_time_response(&block_time_body) {
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
                    function_name: "prazopay_agreement_status::execute".to_string(),
                    action: PluginAction::Complete,
                    outcome: Some(PluginOutcome::Success),
                    duration_ms: Some(
                        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    ),
                    attrs: Some(json!({"slot": report.slot, "status": report.status}).to_string()),
                    message: "inspected PrazoPay Agreement".to_string(),
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

    export!(PrazoPayAgreementStatusPlugin);
}
