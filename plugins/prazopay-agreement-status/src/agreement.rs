use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const PRAZOPAY_PROGRAM_ID: &str = "DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm";
const AGREEMENT_ACCOUNT_DATA_LEN: usize = 215;
const DEFAULT_ALERT_BEFORE_SECS: u32 = 300;
const MIN_ALERT_BEFORE_SECS: u32 = 30;
const MAX_ALERT_BEFORE_SECS: u32 = 86_400;
const DEFAULT_POLL_INTERVAL_SECS: u32 = 300;
const MIN_POLL_INTERVAL_SECS: u32 = 60;
const MAX_POLL_INTERVAL_SECS: u32 = 3_600;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgreementStatusRequest {
    pub cluster: String,
    pub agreement: String,
    #[serde(default)]
    pub alert_before_secs: Option<u32>,
    #[serde(default)]
    pub poll_interval_secs: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgreementStatus {
    Proposed,
    Accepted,
    Funded,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreementView {
    pub funder: String,
    pub worker: String,
    pub task_id_hex: String,
    pub terms_hash_hex: String,
    pub amount_lamports: u64,
    pub delivery_window_secs: u32,
    pub review_window_secs: u32,
    pub funding_window_secs: u32,
    pub proposed_at: i64,
    pub proposal_expires_at: i64,
    pub accepted_at: i64,
    pub milestone: String,
    pub silence_acceptance: bool,
    pub status: AgreementStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreementAccountSnapshot {
    pub slot: u64,
    pub agreement: AgreementView,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AgreementMonitorDecision {
    pub should_notify: bool,
    pub continue_monitoring: bool,
    pub event_code: String,
    pub severity: String,
    pub responsible_role: Option<String>,
    pub reminder_stage: Option<String>,
    pub seconds_to_boundary: Option<i64>,
    pub recommended_next_check_secs: u32,
    pub event_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AgreementStatusReport {
    pub schema_version: &'static str,
    pub cluster: &'static str,
    pub agreement: String,
    pub slot: u64,
    pub observed_at: i64,
    pub time_source: &'static str,
    pub protocol_version: &'static str,
    pub status: AgreementStatus,
    pub phase: &'static str,
    pub outcome: Option<&'static str>,
    /// True once the Agreement has atomically created and funded its linked
    /// Milestone. This deliberately does not claim that funds are still
    /// locked: only `prazopay_status` can report the Milestone's live state.
    pub milestone_created: bool,
    pub funder: String,
    pub worker: String,
    pub task_id_hex: String,
    pub terms_hash_hex: String,
    pub amount_lamports: u64,
    pub delivery_window_secs: u32,
    pub review_window_secs: u32,
    pub revision_delivery_window_secs: u32,
    pub funding_window_secs: u32,
    pub proposed_at: i64,
    pub proposal_expires_at: i64,
    pub funding_expires_at: Option<i64>,
    pub active_deadline_at: Option<i64>,
    pub accepted_at: Option<i64>,
    pub milestone: Option<String>,
    pub silence_acceptance: bool,
    pub funder_actions: Vec<&'static str>,
    pub worker_actions: Vec<&'static str>,
    pub permissionless_actions: Vec<&'static str>,
    pub reason_codes: Vec<&'static str>,
    pub monitor: AgreementMonitorDecision,
}

#[derive(Debug, Error)]
pub enum AgreementStatusError {
    #[error("ARGUMENT_CLUSTER_UNSUPPORTED")]
    ClusterUnsupported,
    #[error("ARGUMENT_AGREEMENT_INVALID")]
    AgreementInvalid,
    #[error("ARGUMENT_ALERT_WINDOW_INVALID")]
    AlertWindowInvalid,
    #[error("ARGUMENT_POLL_INTERVAL_INVALID")]
    PollIntervalInvalid,
    #[error("RPC_JSON_INVALID")]
    RpcJsonInvalid,
    #[error("RPC_ERROR_RESPONSE")]
    RpcErrorResponse,
    #[error("AGREEMENT_NOT_FOUND")]
    AgreementNotFound,
    #[error("ACCOUNT_OWNER_INVALID")]
    AccountOwnerInvalid,
    #[error("ACCOUNT_EXECUTABLE_INVALID")]
    AccountExecutableInvalid,
    #[error("ACCOUNT_ENCODING_INVALID")]
    AccountEncodingInvalid,
    #[error("ACCOUNT_DATA_INVALID")]
    AccountDataInvalid,
    #[error("ACCOUNT_DISCRIMINATOR_INVALID")]
    AccountDiscriminatorInvalid,
    #[error("ACCOUNT_STATUS_INVALID")]
    AccountStatusInvalid,
    #[error("BLOCK_TIME_UNAVAILABLE")]
    BlockTimeUnavailable,
    #[error("TIME_OVERFLOW")]
    TimeOverflow,
}

#[derive(Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
    #[serde(default)]
    error: Option<Value>,
}

#[derive(Deserialize)]
struct AccountResult {
    context: RpcContext,
    value: Option<RpcAccount>,
}

#[derive(Deserialize)]
struct RpcContext {
    slot: u64,
}

#[derive(Deserialize)]
struct RpcAccount {
    data: (String, String),
    executable: bool,
    owner: String,
}

pub fn validate_agreement_request(
    request: &AgreementStatusRequest,
) -> Result<(), AgreementStatusError> {
    if request.cluster != "devnet" {
        return Err(AgreementStatusError::ClusterUnsupported);
    }
    let decoded = bs58::decode(&request.agreement)
        .into_vec()
        .map_err(|_| AgreementStatusError::AgreementInvalid)?;
    if decoded.len() != 32 {
        return Err(AgreementStatusError::AgreementInvalid);
    }
    if request
        .alert_before_secs
        .is_some_and(|seconds| !(MIN_ALERT_BEFORE_SECS..=MAX_ALERT_BEFORE_SECS).contains(&seconds))
    {
        return Err(AgreementStatusError::AlertWindowInvalid);
    }
    if request.poll_interval_secs.is_some_and(|seconds| {
        !(MIN_POLL_INTERVAL_SECS..=MAX_POLL_INTERVAL_SECS).contains(&seconds)
    }) {
        return Err(AgreementStatusError::PollIntervalInvalid);
    }
    Ok(())
}

pub fn inspect_agreement_account_response(
    raw: &[u8],
) -> Result<AgreementAccountSnapshot, AgreementStatusError> {
    let envelope: RpcEnvelope<AccountResult> =
        serde_json::from_slice(raw).map_err(|_| AgreementStatusError::RpcJsonInvalid)?;
    if envelope.error.is_some() {
        return Err(AgreementStatusError::RpcErrorResponse);
    }
    let result = envelope
        .result
        .ok_or(AgreementStatusError::RpcJsonInvalid)?;
    let account = result
        .value
        .ok_or(AgreementStatusError::AgreementNotFound)?;
    if account.owner != PRAZOPAY_PROGRAM_ID {
        return Err(AgreementStatusError::AccountOwnerInvalid);
    }
    if account.executable {
        return Err(AgreementStatusError::AccountExecutableInvalid);
    }
    if account.data.1 != "base64" {
        return Err(AgreementStatusError::AccountEncodingInvalid);
    }
    let data = BASE64
        .decode(account.data.0.as_bytes())
        .map_err(|_| AgreementStatusError::AccountDataInvalid)?;
    Ok(AgreementAccountSnapshot {
        slot: result.context.slot,
        agreement: decode_agreement(&data)?,
    })
}

pub fn parse_agreement_block_time_response(raw: &[u8]) -> Result<i64, AgreementStatusError> {
    let envelope: RpcEnvelope<i64> =
        serde_json::from_slice(raw).map_err(|_| AgreementStatusError::RpcJsonInvalid)?;
    if envelope.error.is_some() {
        return Err(AgreementStatusError::RpcErrorResponse);
    }
    envelope
        .result
        .ok_or(AgreementStatusError::BlockTimeUnavailable)
}

impl AgreementAccountSnapshot {
    pub fn report(
        &self,
        request: &AgreementStatusRequest,
        observed_at: i64,
    ) -> Result<AgreementStatusReport, AgreementStatusError> {
        validate_agreement_request(request)?;
        let agreement = &self.agreement;
        let has_acceptance = matches!(
            agreement.status,
            AgreementStatus::Accepted | AgreementStatus::Funded
        );
        let funding_expires_at = has_acceptance
            .then(|| {
                agreement
                    .accepted_at
                    .checked_add(i64::from(agreement.funding_window_secs))
                    .ok_or(AgreementStatusError::TimeOverflow)
            })
            .transpose()?;
        let active_deadline_at = match agreement.status {
            AgreementStatus::Proposed => Some(agreement.proposal_expires_at),
            AgreementStatus::Accepted => funding_expires_at,
            AgreementStatus::Funded | AgreementStatus::Rejected => None,
        };
        let expired = active_deadline_at.is_some_and(|deadline| observed_at > deadline);

        let mut funder_actions = Vec::new();
        let mut worker_actions = Vec::new();
        let permissionless_actions = Vec::new();
        let (phase, outcome, reason_code) = if expired {
            let reason = match agreement.status {
                AgreementStatus::Proposed => "AGREEMENT_PROPOSAL_EXPIRED",
                AgreementStatus::Accepted => "AGREEMENT_FUNDING_WINDOW_EXPIRED",
                AgreementStatus::Funded | AgreementStatus::Rejected => {
                    "AGREEMENT_TERMINAL_STATE_INVALID"
                }
            };
            ("expired", Some("expired"), reason)
        } else {
            match agreement.status {
                AgreementStatus::Proposed => {
                    worker_actions.extend(["accept_agreement", "reject_agreement"]);
                    (
                        "awaiting_worker",
                        None,
                        "AGREEMENT_AWAITING_WORKER_ACCEPTANCE",
                    )
                }
                AgreementStatus::Accepted => {
                    funder_actions.push("fund_accepted_agreement");
                    (
                        "awaiting_funding",
                        None,
                        "AGREEMENT_ACCEPTED_AWAITING_FUNDING",
                    )
                }
                AgreementStatus::Funded => ("funded", Some("funded"), "AGREEMENT_ESCROW_FUNDED"),
                AgreementStatus::Rejected => {
                    ("rejected", Some("rejected"), "AGREEMENT_WORKER_REJECTED")
                }
            }
        };

        let monitor = agreement_monitor_decision(
            request,
            agreement,
            observed_at,
            active_deadline_at,
            expired,
        );
        Ok(AgreementStatusReport {
            schema_version: "prazopay.agreement-status.v1",
            cluster: "devnet",
            agreement: request.agreement.clone(),
            slot: self.slot,
            observed_at,
            time_source: "solana_block_time",
            protocol_version: "v2",
            status: agreement.status,
            phase,
            outcome,
            milestone_created: agreement.status == AgreementStatus::Funded,
            funder: agreement.funder.clone(),
            worker: agreement.worker.clone(),
            task_id_hex: agreement.task_id_hex.clone(),
            terms_hash_hex: agreement.terms_hash_hex.clone(),
            amount_lamports: agreement.amount_lamports,
            delivery_window_secs: agreement.delivery_window_secs,
            review_window_secs: agreement.review_window_secs,
            revision_delivery_window_secs: agreement.review_window_secs,
            funding_window_secs: agreement.funding_window_secs,
            proposed_at: agreement.proposed_at,
            proposal_expires_at: agreement.proposal_expires_at,
            funding_expires_at,
            active_deadline_at,
            accepted_at: has_acceptance.then_some(agreement.accepted_at),
            milestone: (agreement.status == AgreementStatus::Funded)
                .then(|| agreement.milestone.clone()),
            silence_acceptance: agreement.silence_acceptance,
            funder_actions,
            worker_actions,
            permissionless_actions,
            reason_codes: vec![reason_code],
            monitor,
        })
    }
}

fn agreement_monitor_decision(
    request: &AgreementStatusRequest,
    agreement: &AgreementView,
    observed_at: i64,
    active_deadline_at: Option<i64>,
    expired: bool,
) -> AgreementMonitorDecision {
    let alert_before = i64::from(
        request
            .alert_before_secs
            .unwrap_or(DEFAULT_ALERT_BEFORE_SECS),
    );
    let poll_interval = request
        .poll_interval_secs
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
    let remaining = active_deadline_at.map(|deadline| deadline.saturating_sub(observed_at));

    let (should_notify, event_code, severity, responsible_role, reminder_stage) = if expired {
        let event_code = match agreement.status {
            AgreementStatus::Proposed => "AGREEMENT_PROPOSAL_EXPIRED",
            AgreementStatus::Accepted => "AGREEMENT_FUNDING_WINDOW_EXPIRED",
            AgreementStatus::Funded | AgreementStatus::Rejected => "AGREEMENT_EXPIRED",
        };
        (
            true,
            event_code.to_string(),
            "warning".to_string(),
            Some("both".to_string()),
            Some("final".to_string()),
        )
    } else {
        match agreement.status {
            AgreementStatus::Proposed => active_decision(
                "AGREEMENT_ACCEPTANCE_REQUIRED",
                "AGREEMENT_ACCEPTANCE_DEADLINE",
                "worker",
                observed_at.saturating_sub(agreement.proposed_at),
                remaining.unwrap_or_default(),
                alert_before,
                poll_interval,
            ),
            AgreementStatus::Accepted => active_decision(
                "AGREEMENT_FUNDING_REQUIRED",
                "AGREEMENT_FUNDING_DEADLINE",
                "funder",
                observed_at.saturating_sub(agreement.accepted_at),
                remaining.unwrap_or_default(),
                alert_before,
                poll_interval,
            ),
            AgreementStatus::Funded => (
                true,
                "AGREEMENT_FUNDED".to_string(),
                "success".to_string(),
                Some("both".to_string()),
                Some("final".to_string()),
            ),
            AgreementStatus::Rejected => (
                true,
                "AGREEMENT_REJECTED".to_string(),
                "info".to_string(),
                Some("both".to_string()),
                Some("final".to_string()),
            ),
        }
    };

    let terminal = expired
        || matches!(
            agreement.status,
            AgreementStatus::Funded | AgreementStatus::Rejected
        );
    let event_id = agreement_event_id(
        request,
        agreement,
        &event_code,
        reminder_stage.as_deref(),
        expired,
    );
    AgreementMonitorDecision {
        should_notify,
        continue_monitoring: true,
        event_code,
        severity,
        responsible_role,
        reminder_stage,
        seconds_to_boundary: (!terminal).then_some(remaining.unwrap_or_default()),
        recommended_next_check_secs: poll_interval,
        event_id,
    }
}

fn active_decision(
    active_code: &str,
    deadline_code: &str,
    role: &str,
    elapsed: i64,
    remaining: i64,
    alert_before: i64,
    poll_interval: u32,
) -> (bool, String, String, Option<String>, Option<String>) {
    if let Some(stage) = deadline_reminder_stage(remaining, alert_before, poll_interval) {
        return (
            true,
            if stage == "final" {
                format!("{deadline_code}_FINAL")
            } else {
                format!("{deadline_code}_APPROACHING")
            },
            "warning".to_string(),
            Some(role.to_string()),
            Some(stage),
        );
    }
    if let Some(stage) = sparse_reminder_stage(elapsed, poll_interval) {
        return (
            true,
            active_code.to_string(),
            "action".to_string(),
            Some(role.to_string()),
            Some(stage),
        );
    }
    // Re-emit the stable state-entry event between sparse thresholds. The relay
    // deduplicates its event ID, while a new relay state can still recover the
    // currently actionable obligation after a restart or long outage.
    (
        true,
        active_code.to_string(),
        "action".to_string(),
        Some(role.to_string()),
        Some("state_entry".to_string()),
    )
}

fn deadline_reminder_stage(
    seconds_to_boundary: i64,
    alert_before: i64,
    poll_interval_secs: u32,
) -> Option<String> {
    let poll = i64::from(poll_interval_secs);
    if seconds_to_boundary >= 0 && seconds_to_boundary <= poll {
        return Some("final".to_string());
    }
    if seconds_to_boundary <= alert_before
        && seconds_to_boundary > alert_before.saturating_sub(poll)
    {
        return Some("approaching".to_string());
    }
    None
}

fn sparse_reminder_stage(elapsed: i64, poll_interval_secs: u32) -> Option<String> {
    if elapsed < 0 {
        return None;
    }
    let poll = i64::from(poll_interval_secs);
    if elapsed < poll {
        return Some("state_entry".to_string());
    }
    for (threshold, label) in [(30 * 60_i64, "30m"), (2 * 60 * 60_i64, "2h")] {
        if elapsed >= threshold && elapsed < threshold.saturating_add(poll) {
            return Some(label.to_string());
        }
    }
    if elapsed >= 24 * 60 * 60 {
        let day = elapsed / (24 * 60 * 60);
        if elapsed % (24 * 60 * 60) < poll {
            return Some(format!("day_{day}"));
        }
    }
    None
}

fn agreement_event_id(
    request: &AgreementStatusRequest,
    agreement: &AgreementView,
    event_code: &str,
    reminder_stage: Option<&str>,
    expired: bool,
) -> String {
    let material = format!(
        "{}|{:?}|{}|{}|{}|{}|{}|{}",
        request.agreement,
        agreement.status,
        agreement.proposed_at,
        agreement.accepted_at,
        agreement.milestone,
        expired,
        event_code,
        reminder_stage.unwrap_or("none")
    );
    let digest = Sha256::digest(material.as_bytes());
    format!("prazopay:{}", hex(&digest[..16]))
}

fn decode_agreement(data: &[u8]) -> Result<AgreementView, AgreementStatusError> {
    if data.len() != AGREEMENT_ACCOUNT_DATA_LEN {
        return Err(AgreementStatusError::AccountDataInvalid);
    }
    if data[..8] != account_discriminator() {
        return Err(AgreementStatusError::AccountDiscriminatorInvalid);
    }

    let mut offset = 8;
    let funder = bs58::encode(take::<32>(data, &mut offset)?).into_string();
    let worker = bs58::encode(take::<32>(data, &mut offset)?).into_string();
    let task_id_hex = hex(&take::<32>(data, &mut offset)?);
    let terms_hash_hex = hex(&take::<32>(data, &mut offset)?);
    let amount_lamports = u64::from_le_bytes(take::<8>(data, &mut offset)?);
    let delivery_window_secs = u32::from_le_bytes(take::<4>(data, &mut offset)?);
    let review_window_secs = u32::from_le_bytes(take::<4>(data, &mut offset)?);
    let funding_window_secs = u32::from_le_bytes(take::<4>(data, &mut offset)?);
    let proposed_at = i64::from_le_bytes(take::<8>(data, &mut offset)?);
    let proposal_expires_at = i64::from_le_bytes(take::<8>(data, &mut offset)?);
    let accepted_at = i64::from_le_bytes(take::<8>(data, &mut offset)?);
    let milestone = bs58::encode(take::<32>(data, &mut offset)?).into_string();
    let silence_acceptance = match take::<1>(data, &mut offset)?[0] {
        0 => false,
        1 => true,
        _ => return Err(AgreementStatusError::AccountDataInvalid),
    };
    let status = match take::<1>(data, &mut offset)?[0] {
        0 => AgreementStatus::Proposed,
        1 => AgreementStatus::Accepted,
        2 => AgreementStatus::Funded,
        3 => AgreementStatus::Rejected,
        _ => return Err(AgreementStatusError::AccountStatusInvalid),
    };
    let _bump = take::<1>(data, &mut offset)?[0];
    if offset != AGREEMENT_ACCOUNT_DATA_LEN {
        return Err(AgreementStatusError::AccountDataInvalid);
    }

    Ok(AgreementView {
        funder,
        worker,
        task_id_hex,
        terms_hash_hex,
        amount_lamports,
        delivery_window_secs,
        review_window_secs,
        funding_window_secs,
        proposed_at,
        proposal_expires_at,
        accepted_at,
        milestone,
        silence_acceptance,
        status,
    })
}

fn account_discriminator() -> [u8; 8] {
    let digest = Sha256::digest(b"account:Agreement");
    let mut discriminator = [0; 8];
    discriminator.copy_from_slice(&digest[..8]);
    discriminator
}

fn take<const N: usize>(data: &[u8], offset: &mut usize) -> Result<[u8; N], AgreementStatusError> {
    let end = offset
        .checked_add(N)
        .ok_or(AgreementStatusError::AccountDataInvalid)?;
    let slice = data
        .get(*offset..end)
        .ok_or(AgreementStatusError::AccountDataInvalid)?;
    let array = slice
        .try_into()
        .map_err(|_| AgreementStatusError::AccountDataInvalid)?;
    *offset = end;
    Ok(array)
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROPOSED_AT: i64 = 10_000;
    const EXPIRES_AT: i64 = PROPOSED_AT + 10_000;

    fn request() -> AgreementStatusRequest {
        AgreementStatusRequest {
            cluster: "devnet".to_string(),
            agreement: bs58::encode([9; 32]).into_string(),
            alert_before_secs: Some(300),
            poll_interval_secs: Some(60),
        }
    }

    fn account_bytes(status: AgreementStatus, accepted_at: i64) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(AGREEMENT_ACCOUNT_DATA_LEN);
        bytes.extend_from_slice(&account_discriminator());
        bytes.extend_from_slice(&[1; 32]);
        bytes.extend_from_slice(&[2; 32]);
        bytes.extend_from_slice(&[3; 32]);
        bytes.extend_from_slice(&[4; 32]);
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&3_600_u32.to_le_bytes());
        bytes.extend_from_slice(&600_u32.to_le_bytes());
        bytes.extend_from_slice(&900_u32.to_le_bytes());
        bytes.extend_from_slice(&PROPOSED_AT.to_le_bytes());
        bytes.extend_from_slice(&EXPIRES_AT.to_le_bytes());
        bytes.extend_from_slice(&accepted_at.to_le_bytes());
        bytes.extend_from_slice(if status == AgreementStatus::Funded {
            &[7; 32]
        } else {
            &[0; 32]
        });
        bytes.push(1);
        bytes.push(match status {
            AgreementStatus::Proposed => 0,
            AgreementStatus::Accepted => 1,
            AgreementStatus::Funded => 2,
            AgreementStatus::Rejected => 3,
        });
        bytes.push(254);
        assert_eq!(bytes.len(), AGREEMENT_ACCOUNT_DATA_LEN);
        bytes
    }

    fn rpc_response(status: AgreementStatus, accepted_at: i64) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "result": {
                "context": {"slot": 456},
                "value": {
                    "data": [BASE64.encode(account_bytes(status, accepted_at)), "base64"],
                    "executable": false,
                    "owner": PRAZOPAY_PROGRAM_ID,
                    "space": AGREEMENT_ACCOUNT_DATA_LEN
                }
            }
        }))
        .unwrap()
    }

    fn report(
        status: AgreementStatus,
        accepted_at: i64,
        observed_at: i64,
    ) -> AgreementStatusReport {
        inspect_agreement_account_response(&rpc_response(status, accepted_at))
            .unwrap()
            .report(&request(), observed_at)
            .unwrap()
    }

    #[test]
    fn proposed_agreement_assigns_worker_actions_and_sparse_alerts() {
        let immediate = report(AgreementStatus::Proposed, 0, PROPOSED_AT);
        assert_eq!(immediate.phase, "awaiting_worker");
        assert!(!immediate.milestone_created);
        assert_eq!(
            immediate.worker_actions,
            vec!["accept_agreement", "reject_agreement"]
        );
        assert_eq!(
            immediate.monitor.event_code,
            "AGREEMENT_ACCEPTANCE_REQUIRED"
        );
        assert_eq!(
            immediate.monitor.reminder_stage.as_deref(),
            Some("state_entry")
        );

        let recovered = report(AgreementStatus::Proposed, 0, PROPOSED_AT + 600);
        assert!(recovered.monitor.should_notify);
        assert_eq!(
            recovered.monitor.event_code,
            "AGREEMENT_ACCEPTANCE_REQUIRED"
        );
        assert_eq!(recovered.monitor.event_id, immediate.monitor.event_id);
    }

    #[test]
    fn accepted_agreement_assigns_funder_action() {
        let accepted_at = PROPOSED_AT + 100;
        let accepted_report = report(AgreementStatus::Accepted, accepted_at, accepted_at);
        assert_eq!(accepted_report.phase, "awaiting_funding");
        assert_eq!(
            accepted_report.funder_actions,
            vec!["fund_accepted_agreement"]
        );
        assert_eq!(
            accepted_report.monitor.event_code,
            "AGREEMENT_FUNDING_REQUIRED"
        );
        assert_eq!(
            accepted_report.monitor.responsible_role.as_deref(),
            Some("funder")
        );
        assert_eq!(accepted_report.funding_expires_at, Some(accepted_at + 900));
        assert_eq!(accepted_report.active_deadline_at, Some(accepted_at + 900));
        assert_eq!(accepted_report.revision_delivery_window_secs, 600);

        let expired = report(AgreementStatus::Accepted, accepted_at, accepted_at + 901);
        assert_eq!(expired.phase, "expired");
        assert_eq!(
            expired.monitor.event_code,
            "AGREEMENT_FUNDING_WINDOW_EXPIRED"
        );
        assert_eq!(
            expired.reason_codes,
            vec!["AGREEMENT_FUNDING_WINDOW_EXPIRED"]
        );
    }

    #[test]
    fn proposal_deadline_has_a_distinct_event() {
        let report = report(AgreementStatus::Proposed, 0, EXPIRES_AT - 30);
        assert!(report.monitor.should_notify);
        assert_eq!(
            report.monitor.event_code,
            "AGREEMENT_ACCEPTANCE_DEADLINE_FINAL"
        );
        assert_eq!(report.monitor.seconds_to_boundary, Some(30));
    }

    #[test]
    fn expired_proposal_is_terminal_without_locked_funds() {
        let report = report(AgreementStatus::Proposed, 0, EXPIRES_AT + 1);
        assert_eq!(report.phase, "expired");
        assert_eq!(report.outcome, Some("expired"));
        assert!(!report.milestone_created);
        assert!(report.worker_actions.is_empty());
        assert_eq!(report.monitor.event_code, "AGREEMENT_PROPOSAL_EXPIRED");
        assert_eq!(report.monitor.reminder_stage.as_deref(), Some("final"));
    }

    #[test]
    fn rejected_is_terminal_and_funded_is_a_milestone_handoff() {
        let rejected = report(AgreementStatus::Rejected, 0, PROPOSED_AT + 1);
        assert_eq!(rejected.outcome, Some("rejected"));
        assert!(!rejected.milestone_created);
        assert_eq!(rejected.monitor.event_code, "AGREEMENT_REJECTED");

        let funded = report(AgreementStatus::Funded, PROPOSED_AT + 1, PROPOSED_AT + 2);
        assert_eq!(funded.outcome, Some("funded"));
        assert!(funded.milestone_created);
        assert_eq!(funded.milestone, Some(bs58::encode([7; 32]).into_string()));
        assert_eq!(funded.monitor.event_code, "AGREEMENT_FUNDED");
    }

    #[test]
    fn wrong_owner_discriminator_and_layout_fail_closed() {
        let mut wrong_owner: Value =
            serde_json::from_slice(&rpc_response(AgreementStatus::Proposed, 0)).unwrap();
        wrong_owner["result"]["value"]["owner"] = json!(bs58::encode([8; 32]).into_string());
        assert!(matches!(
            inspect_agreement_account_response(&serde_json::to_vec(&wrong_owner).unwrap()),
            Err(AgreementStatusError::AccountOwnerInvalid)
        ));

        let mut bad = account_bytes(AgreementStatus::Proposed, 0);
        bad[0] ^= 0xff;
        let response = json!({
            "result": {
                "context": {"slot": 456},
                "value": {
                    "data": [BASE64.encode(bad), "base64"],
                    "executable": false,
                    "owner": PRAZOPAY_PROGRAM_ID
                }
            }
        });
        assert!(matches!(
            inspect_agreement_account_response(&serde_json::to_vec(&response).unwrap()),
            Err(AgreementStatusError::AccountDiscriminatorInvalid)
        ));
    }

    #[test]
    fn request_and_block_time_validation_fail_closed() {
        let mut mainnet = request();
        mainnet.cluster = "mainnet-beta".to_string();
        assert!(matches!(
            validate_agreement_request(&mainnet),
            Err(AgreementStatusError::ClusterUnsupported)
        ));

        let unknown = format!(
            r#"{{"cluster":"devnet","agreement":"{}","rpc_url":"https://example.com"}}"#,
            request().agreement
        );
        assert!(serde_json::from_str::<AgreementStatusRequest>(&unknown).is_err());
        assert_eq!(
            parse_agreement_block_time_response(br#"{"jsonrpc":"2.0","id":2,"result":1234}"#)
                .unwrap(),
            1234
        );
    }
}
