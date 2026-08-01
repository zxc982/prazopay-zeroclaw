use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const PRAZOPAY_PROGRAM_ID: &str = "DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm";
const ACCOUNT_DATA_LEN: usize = 231;
const MAX_REVISIONS: u8 = 3;
const PROTOCOL_V1_FLAG: u8 = 0b1000_0000;
const PROTOCOL_V2_FLAG: u8 = 0b0100_0000;
const REVISION_COUNT_MASK: u8 = 0b0011_1111;
const MAX_CLAIM_GRACE_SECS: u32 = 60 * 60;
const DEFAULT_ALERT_BEFORE_SECS: u32 = 300;
const MIN_ALERT_BEFORE_SECS: u32 = 30;
const MAX_ALERT_BEFORE_SECS: u32 = 86_400;
const DEFAULT_POLL_INTERVAL_SECS: u32 = 300;
const MIN_POLL_INTERVAL_SECS: u32 = 60;
const MAX_POLL_INTERVAL_SECS: u32 = 3_600;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    pub cluster: String,
    pub milestone: String,
    #[serde(default)]
    pub alert_before_secs: Option<u32>,
    #[serde(default)]
    pub poll_interval_secs: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneStatus {
    Open,
    Submitted,
    Paid,
    Refunded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MilestoneView {
    pub funder: String,
    pub worker: String,
    pub task_id_hex: String,
    pub terms_hash_hex: String,
    pub evidence_hash_hex: String,
    pub feedback_hash_hex: String,
    pub amount_lamports: u64,
    pub due_at: i64,
    pub review_window_secs: u32,
    pub submitted_at: i64,
    pub revision_count: u8,
    pub protocol_version: u8,
    pub claim_grace_secs: u32,
    pub status: MilestoneStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub slot: u64,
    pub milestone: MilestoneView,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MonitorDecision {
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
pub struct StatusReport {
    pub schema_version: &'static str,
    pub cluster: &'static str,
    pub milestone: String,
    pub slot: u64,
    pub observed_at: i64,
    pub time_source: &'static str,
    pub protocol_version: &'static str,
    pub acceptance_policy: &'static str,
    pub status: MilestoneStatus,
    pub funder: String,
    pub worker: String,
    pub task_id_hex: String,
    pub terms_hash_hex: String,
    pub evidence_hash_hex: String,
    pub feedback_hash_hex: String,
    pub amount_lamports: u64,
    pub due_at: i64,
    pub review_window_secs: u32,
    pub claim_grace_secs: u32,
    pub submitted_at: Option<i64>,
    pub terminal_at: Option<i64>,
    pub outcome: Option<&'static str>,
    pub review_ends_at: Option<i64>,
    pub claimable_at: Option<i64>,
    pub revision_count: u8,
    pub funder_actions: Vec<&'static str>,
    pub worker_actions: Vec<&'static str>,
    pub permissionless_actions: Vec<&'static str>,
    pub reason_codes: Vec<&'static str>,
    pub monitor: MonitorDecision,
}

#[derive(Debug, Error)]
pub enum StatusError {
    #[error("ARGUMENT_CLUSTER_UNSUPPORTED")]
    ClusterUnsupported,
    #[error("ARGUMENT_MILESTONE_INVALID")]
    MilestoneInvalid,
    #[error("ARGUMENT_ALERT_WINDOW_INVALID")]
    AlertWindowInvalid,
    #[error("ARGUMENT_POLL_INTERVAL_INVALID")]
    PollIntervalInvalid,
    #[error("RPC_JSON_INVALID")]
    RpcJsonInvalid,
    #[error("RPC_ERROR_RESPONSE")]
    RpcErrorResponse,
    #[error("MILESTONE_NOT_FOUND")]
    MilestoneNotFound,
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

pub fn validate_request(request: &StatusRequest) -> Result<(), StatusError> {
    if request.cluster != "devnet" {
        return Err(StatusError::ClusterUnsupported);
    }
    let decoded = bs58::decode(&request.milestone)
        .into_vec()
        .map_err(|_| StatusError::MilestoneInvalid)?;
    if decoded.len() != 32 {
        return Err(StatusError::MilestoneInvalid);
    }
    if request
        .alert_before_secs
        .is_some_and(|seconds| !(MIN_ALERT_BEFORE_SECS..=MAX_ALERT_BEFORE_SECS).contains(&seconds))
    {
        return Err(StatusError::AlertWindowInvalid);
    }
    if request.poll_interval_secs.is_some_and(|seconds| {
        !(MIN_POLL_INTERVAL_SECS..=MAX_POLL_INTERVAL_SECS).contains(&seconds)
    }) {
        return Err(StatusError::PollIntervalInvalid);
    }
    Ok(())
}

pub fn inspect_account_response(raw: &[u8]) -> Result<AccountSnapshot, StatusError> {
    let envelope: RpcEnvelope<AccountResult> =
        serde_json::from_slice(raw).map_err(|_| StatusError::RpcJsonInvalid)?;
    if envelope.error.is_some() {
        return Err(StatusError::RpcErrorResponse);
    }
    let result = envelope.result.ok_or(StatusError::RpcJsonInvalid)?;
    let account = result.value.ok_or(StatusError::MilestoneNotFound)?;
    if account.owner != PRAZOPAY_PROGRAM_ID {
        return Err(StatusError::AccountOwnerInvalid);
    }
    if account.executable {
        return Err(StatusError::AccountExecutableInvalid);
    }
    if account.data.1 != "base64" {
        return Err(StatusError::AccountEncodingInvalid);
    }
    let data = BASE64
        .decode(account.data.0.as_bytes())
        .map_err(|_| StatusError::AccountDataInvalid)?;
    let milestone = decode_milestone(&data)?;
    Ok(AccountSnapshot {
        slot: result.context.slot,
        milestone,
    })
}

pub fn parse_block_time_response(raw: &[u8]) -> Result<i64, StatusError> {
    let envelope: RpcEnvelope<i64> =
        serde_json::from_slice(raw).map_err(|_| StatusError::RpcJsonInvalid)?;
    if envelope.error.is_some() {
        return Err(StatusError::RpcErrorResponse);
    }
    envelope.result.ok_or(StatusError::BlockTimeUnavailable)
}

impl AccountSnapshot {
    pub fn report(
        &self,
        request: &StatusRequest,
        observed_at: i64,
    ) -> Result<StatusReport, StatusError> {
        validate_request(request)?;
        let milestone = &self.milestone;
        let review_ends_at = if milestone.status == MilestoneStatus::Submitted {
            Some(
                milestone
                    .submitted_at
                    .checked_add(i64::from(milestone.review_window_secs))
                    .ok_or(StatusError::TimeOverflow)?,
            )
        } else {
            None
        };
        let claimable_at = review_ends_at
            .map(|review_end| {
                review_end
                    .checked_add(i64::from(milestone.claim_grace_secs))
                    .ok_or(StatusError::TimeOverflow)
            })
            .transpose()?;
        let terminal_at = (milestone.protocol_version >= 1
            && matches!(
                milestone.status,
                MilestoneStatus::Paid | MilestoneStatus::Refunded
            )
            && milestone.submitted_at > 0)
            .then_some(milestone.submitted_at);
        let outcome = match milestone.status {
            MilestoneStatus::Paid => Some("success"),
            MilestoneStatus::Refunded => Some("failed"),
            _ => None,
        };

        let mut funder_actions = Vec::new();
        let mut worker_actions = Vec::new();
        let mut permissionless_actions = Vec::new();
        let mut reason_codes = Vec::new();

        match milestone.status {
            MilestoneStatus::Open if observed_at <= milestone.due_at => {
                worker_actions.push("submit_delivery");
                reason_codes.push("AWAITING_DELIVERY");
            }
            MilestoneStatus::Open => {
                permissionless_actions.push("refund_expired");
                reason_codes.push("DEADLINE_ELAPSED_WITHOUT_SUBMISSION");
            }
            MilestoneStatus::Submitted => {
                funder_actions.push("approve_milestone");
                let review_end = review_ends_at.ok_or(StatusError::TimeOverflow)?;
                let revision_is_available = milestone.revision_count < MAX_REVISIONS
                    && (milestone.protocol_version >= 1 || observed_at < milestone.due_at);
                if observed_at < review_end && revision_is_available {
                    funder_actions.push("request_revision");
                    reason_codes.push("REVIEW_WINDOW_OPEN");
                } else if observed_at < review_end {
                    reason_codes.push("REVISION_UNAVAILABLE");
                } else if observed_at < claimable_at.ok_or(StatusError::TimeOverflow)? {
                    reason_codes.push("CLAIM_GRACE_ACTIVE");
                } else if milestone.protocol_version >= 1 {
                    worker_actions.push("claim_after_review");
                    permissionless_actions.push("settle_after_review");
                    reason_codes.push("SILENCE_ACCEPTANCE_SETTLEABLE");
                } else {
                    worker_actions.push("claim_after_review");
                    reason_codes.push("LEGACY_REVIEW_ELAPSED_CLAIMABLE");
                }
            }
            MilestoneStatus::Paid => reason_codes.push("TERMINAL_PAID"),
            MilestoneStatus::Refunded => reason_codes.push("TERMINAL_REFUNDED"),
        }

        let monitor = monitor_decision(
            request,
            milestone,
            observed_at,
            review_ends_at,
            claimable_at,
            reason_codes
                .first()
                .copied()
                .ok_or(StatusError::AccountStatusInvalid)?,
        );

        Ok(StatusReport {
            schema_version: "prazopay.status.v2",
            cluster: "devnet",
            milestone: request.milestone.clone(),
            slot: self.slot,
            observed_at,
            time_source: "solana_block_time",
            protocol_version: match milestone.protocol_version {
                2 => "v2",
                1 => "v1",
                _ => "v0_legacy",
            },
            acceptance_policy: match milestone.protocol_version {
                2 => "worker_signed_silence_acceptance",
                1 => "explicit_silence_acceptance",
                _ => "legacy_terms_hash_only",
            },
            status: milestone.status,
            funder: milestone.funder.clone(),
            worker: milestone.worker.clone(),
            task_id_hex: milestone.task_id_hex.clone(),
            terms_hash_hex: milestone.terms_hash_hex.clone(),
            evidence_hash_hex: milestone.evidence_hash_hex.clone(),
            feedback_hash_hex: milestone.feedback_hash_hex.clone(),
            amount_lamports: milestone.amount_lamports,
            due_at: milestone.due_at,
            review_window_secs: milestone.review_window_secs,
            claim_grace_secs: milestone.claim_grace_secs,
            submitted_at: (milestone.status == MilestoneStatus::Submitted)
                .then_some(milestone.submitted_at),
            terminal_at,
            outcome,
            review_ends_at,
            claimable_at,
            revision_count: milestone.revision_count,
            funder_actions,
            worker_actions,
            permissionless_actions,
            reason_codes,
            monitor,
        })
    }
}

fn monitor_decision(
    request: &StatusRequest,
    milestone: &MilestoneView,
    observed_at: i64,
    review_ends_at: Option<i64>,
    claimable_at: Option<i64>,
    reason_code: &'static str,
) -> MonitorDecision {
    let alert_before = i64::from(
        request
            .alert_before_secs
            .unwrap_or(DEFAULT_ALERT_BEFORE_SECS),
    );
    let poll_interval = request
        .poll_interval_secs
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);

    let (
        should_notify,
        continue_monitoring,
        event_code,
        severity,
        responsible_role,
        reminder_stage,
        seconds_to_boundary,
        recommended_next_check_secs,
    ) = match milestone.status {
        MilestoneStatus::Open => {
            let remaining = milestone.due_at.saturating_sub(observed_at);
            if remaining < 0 {
                let stage = sparse_reminder_stage(remaining.saturating_neg(), poll_interval);
                if stage.is_some() {
                    (
                        true,
                        true,
                        "WORKER_DELIVERY_DELAYED".to_string(),
                        "action".to_string(),
                        Some("worker".to_string()),
                        stage,
                        Some(remaining),
                        poll_interval,
                    )
                } else {
                    (
                        true,
                        true,
                        "WORKER_DELIVERY_DELAYED".to_string(),
                        "action".to_string(),
                        Some("worker".to_string()),
                        Some("state_entry".to_string()),
                        Some(remaining),
                        poll_interval,
                    )
                }
            } else if let Some(stage) =
                deadline_reminder_stage(remaining, alert_before, poll_interval)
            {
                (
                    true,
                    true,
                    if stage == "final" {
                        "DELIVERY_DEADLINE_FINAL".to_string()
                    } else {
                        "DELIVERY_DEADLINE_APPROACHING".to_string()
                    },
                    "warning".to_string(),
                    Some("worker".to_string()),
                    Some(stage),
                    Some(remaining),
                    poll_interval,
                )
            } else {
                (
                    false,
                    true,
                    "AWAITING_DELIVERY_QUIET".to_string(),
                    "quiet".to_string(),
                    Some("worker".to_string()),
                    None,
                    Some(remaining),
                    poll_interval,
                )
            }
        }
        MilestoneStatus::Submitted => {
            let review_end = review_ends_at.unwrap_or(milestone.submitted_at);
            let claimable = claimable_at.unwrap_or(review_end);
            if observed_at < review_end {
                let remaining = review_end.saturating_sub(observed_at);
                let elapsed = observed_at.saturating_sub(milestone.submitted_at);
                if let Some(stage) =
                    review_reminder_stage(elapsed, remaining, alert_before, poll_interval)
                {
                    let event_code = match stage.as_str() {
                        "state_entry" => "FUNDER_REVIEW_OPENED",
                        "state_entry_and_deadline" => "FUNDER_REVIEW_REQUIRED",
                        "final" => "FUNDER_REVIEW_DEADLINE",
                        _ => "FUNDER_REVIEW_APPROACHING",
                    };
                    (
                        true,
                        true,
                        event_code.to_string(),
                        if remaining <= alert_before {
                            "warning".to_string()
                        } else {
                            "info".to_string()
                        },
                        Some("funder".to_string()),
                        Some(stage),
                        Some(remaining),
                        poll_interval,
                    )
                } else {
                    (
                        true,
                        true,
                        "FUNDER_REVIEW_OPENED".to_string(),
                        "info".to_string(),
                        Some("funder".to_string()),
                        Some("state_entry".to_string()),
                        Some(remaining),
                        poll_interval,
                    )
                }
            } else {
                let funder_elapsed = observed_at.saturating_sub(review_end);
                let funder_stage = sparse_reminder_stage(funder_elapsed, poll_interval);
                if observed_at < claimable {
                    if let Some(stage) = funder_stage {
                        (
                            true,
                            true,
                            "FUNDER_REVIEW_DELAYED".to_string(),
                            "warning".to_string(),
                            Some("funder".to_string()),
                            Some(stage),
                            Some(review_end.saturating_sub(observed_at)),
                            poll_interval,
                        )
                    } else {
                        (
                            true,
                            true,
                            "FUNDER_REVIEW_DELAYED".to_string(),
                            "warning".to_string(),
                            Some("funder".to_string()),
                            Some("state_entry".to_string()),
                            Some(review_end.saturating_sub(observed_at)),
                            poll_interval,
                        )
                    }
                } else if milestone.protocol_version >= 1 {
                    let settlement_stage =
                        sparse_reminder_stage(observed_at.saturating_sub(claimable), poll_interval);
                    match (funder_stage, settlement_stage) {
                        (Some(funder), Some(settlement)) => (
                            true,
                            true,
                            "FUNDER_REVIEW_DELAYED_SETTLEMENT_READY".to_string(),
                            "action".to_string(),
                            Some("funder_and_permissionless_trigger".to_string()),
                            Some(format!("funder_{funder}+settlement_{settlement}")),
                            None,
                            poll_interval,
                        ),
                        (Some(stage), None) => (
                            true,
                            true,
                            "FUNDER_REVIEW_DELAYED".to_string(),
                            "warning".to_string(),
                            Some("funder".to_string()),
                            Some(stage),
                            Some(review_end.saturating_sub(observed_at)),
                            poll_interval,
                        ),
                        (None, Some(stage)) => (
                            true,
                            true,
                            "PERMISSIONLESS_SETTLEMENT_READY".to_string(),
                            "action".to_string(),
                            Some("permissionless_trigger".to_string()),
                            Some(stage),
                            Some(claimable.saturating_sub(observed_at)),
                            poll_interval,
                        ),
                        (None, None) => (
                            true,
                            true,
                            "FUNDER_REVIEW_DELAYED_SETTLEMENT_READY".to_string(),
                            "action".to_string(),
                            Some("funder_and_permissionless_trigger".to_string()),
                            Some("funder_state_entry+settlement_state_entry".to_string()),
                            None,
                            poll_interval,
                        ),
                    }
                } else {
                    let worker_stage =
                        sparse_reminder_stage(observed_at.saturating_sub(claimable), poll_interval);
                    if let Some(stage) = worker_stage {
                        (
                            true,
                            true,
                            "LEGACY_WORKER_CLAIM_READY".to_string(),
                            "action".to_string(),
                            Some("worker".to_string()),
                            Some(stage),
                            Some(claimable.saturating_sub(observed_at)),
                            poll_interval,
                        )
                    } else {
                        (
                            true,
                            true,
                            "LEGACY_WORKER_CLAIM_READY".to_string(),
                            "action".to_string(),
                            Some("worker".to_string()),
                            Some("state_entry".to_string()),
                            Some(claimable.saturating_sub(observed_at)),
                            poll_interval,
                        )
                    }
                }
            }
        }
        MilestoneStatus::Paid => {
            if milestone.protocol_version >= 1 {
                (
                    true,
                    true,
                    "SETTLEMENT_SUCCESS".to_string(),
                    "success".to_string(),
                    Some("both".to_string()),
                    Some("final".to_string()),
                    None,
                    poll_interval,
                )
            } else {
                (
                    false,
                    false,
                    "TERMINAL_PAID_QUIET".to_string(),
                    "quiet".to_string(),
                    None,
                    None,
                    None,
                    0,
                )
            }
        }
        MilestoneStatus::Refunded => {
            if milestone.protocol_version >= 1 {
                (
                    true,
                    true,
                    "MILESTONE_FAILED".to_string(),
                    "error".to_string(),
                    Some("both".to_string()),
                    Some("final".to_string()),
                    None,
                    poll_interval,
                )
            } else {
                (
                    false,
                    false,
                    "TERMINAL_REFUNDED_QUIET".to_string(),
                    "quiet".to_string(),
                    None,
                    None,
                    None,
                    0,
                )
            }
        }
    };

    let event_id = monitor_event_id(
        request,
        milestone,
        reason_code,
        &event_code,
        reminder_stage.as_deref(),
    );

    MonitorDecision {
        should_notify,
        continue_monitoring,
        event_code,
        severity,
        responsible_role,
        reminder_stage: reminder_stage.clone(),
        seconds_to_boundary,
        recommended_next_check_secs,
        event_id,
    }
}

fn deadline_reminder_stage(
    seconds_to_boundary: i64,
    alert_before: i64,
    poll_interval_secs: u32,
) -> Option<String> {
    let poll = i64::from(poll_interval_secs);
    if seconds_to_boundary > 0 && seconds_to_boundary <= poll {
        return Some("final".to_string());
    }
    if seconds_to_boundary <= alert_before
        && seconds_to_boundary > alert_before.saturating_sub(poll)
    {
        return Some("approaching".to_string());
    }
    None
}

fn review_reminder_stage(
    elapsed: i64,
    seconds_to_boundary: i64,
    alert_before: i64,
    poll_interval_secs: u32,
) -> Option<String> {
    let poll = i64::from(poll_interval_secs);
    if elapsed >= 0 && elapsed < poll {
        return Some(
            if seconds_to_boundary <= poll {
                "state_entry_and_deadline"
            } else {
                "state_entry"
            }
            .to_string(),
        );
    }
    deadline_reminder_stage(seconds_to_boundary, alert_before, poll_interval_secs)
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

fn monitor_event_id(
    request: &StatusRequest,
    milestone: &MilestoneView,
    reason_code: &str,
    event_code: &str,
    reminder_stage: Option<&str>,
) -> String {
    let material = format!(
        "{}|{}|{:?}|{}|{}|{}|{}|{}",
        request.milestone,
        milestone.protocol_version,
        milestone.status,
        milestone.revision_count,
        milestone.submitted_at,
        reason_code,
        event_code,
        reminder_stage.unwrap_or("none")
    );
    let digest = Sha256::digest(material.as_bytes());
    format!("prazopay:{}", hex(&digest[..16]))
}

fn decode_milestone(data: &[u8]) -> Result<MilestoneView, StatusError> {
    if data.len() != ACCOUNT_DATA_LEN {
        return Err(StatusError::AccountDataInvalid);
    }
    let expected_discriminator = account_discriminator();
    if data[..8] != expected_discriminator {
        return Err(StatusError::AccountDiscriminatorInvalid);
    }

    let mut offset = 8;
    let funder = bs58::encode(take::<32>(data, &mut offset)?).into_string();
    let worker = bs58::encode(take::<32>(data, &mut offset)?).into_string();
    let task_id_hex = hex(&take::<32>(data, &mut offset)?);
    let terms_hash_hex = hex(&take::<32>(data, &mut offset)?);
    let evidence_hash_hex = hex(&take::<32>(data, &mut offset)?);
    let feedback_hash_hex = hex(&take::<32>(data, &mut offset)?);
    let amount_lamports = u64::from_le_bytes(take::<8>(data, &mut offset)?);
    let due_at = i64::from_le_bytes(take::<8>(data, &mut offset)?);
    let review_window_secs = u32::from_le_bytes(take::<4>(data, &mut offset)?);
    let submitted_at = i64::from_le_bytes(take::<8>(data, &mut offset)?);
    let versioned_revision = take::<1>(data, &mut offset)?[0];
    let protocol_version = if versioned_revision & PROTOCOL_V1_FLAG == 0 {
        0
    } else if versioned_revision & PROTOCOL_V2_FLAG != 0 {
        2
    } else {
        1
    };
    let revision_count = versioned_revision & REVISION_COUNT_MASK;
    let claim_grace_secs = if protocol_version >= 1 {
        review_window_secs.min(MAX_CLAIM_GRACE_SECS)
    } else {
        0
    };
    let status = match take::<1>(data, &mut offset)?[0] {
        0 => MilestoneStatus::Open,
        1 => MilestoneStatus::Submitted,
        2 => MilestoneStatus::Paid,
        3 => MilestoneStatus::Refunded,
        _ => return Err(StatusError::AccountStatusInvalid),
    };
    let _bump = take::<1>(data, &mut offset)?[0];
    if offset != ACCOUNT_DATA_LEN {
        return Err(StatusError::AccountDataInvalid);
    }

    Ok(MilestoneView {
        funder,
        worker,
        task_id_hex,
        terms_hash_hex,
        evidence_hash_hex,
        feedback_hash_hex,
        amount_lamports,
        due_at,
        review_window_secs,
        submitted_at,
        revision_count,
        protocol_version,
        claim_grace_secs,
        status,
    })
}

fn account_discriminator() -> [u8; 8] {
    let digest = Sha256::digest(b"account:Milestone");
    let mut discriminator = [0; 8];
    discriminator.copy_from_slice(&digest[..8]);
    discriminator
}

fn take<const N: usize>(data: &[u8], offset: &mut usize) -> Result<[u8; N], StatusError> {
    let end = offset
        .checked_add(N)
        .ok_or(StatusError::AccountDataInvalid)?;
    let slice = data
        .get(*offset..end)
        .ok_or(StatusError::AccountDataInvalid)?;
    let array = slice
        .try_into()
        .map_err(|_| StatusError::AccountDataInvalid)?;
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

    fn request() -> StatusRequest {
        StatusRequest {
            cluster: "devnet".to_string(),
            milestone: bs58::encode([9; 32]).into_string(),
            alert_before_secs: None,
            poll_interval_secs: None,
        }
    }

    fn account_bytes_for_version(
        status: MilestoneStatus,
        submitted_at: i64,
        revisions: u8,
        protocol_v1: bool,
    ) -> Vec<u8> {
        account_bytes_for_protocol(status, submitted_at, revisions, u8::from(protocol_v1), 60)
    }

    fn account_bytes_for_review_window(
        status: MilestoneStatus,
        submitted_at: i64,
        revisions: u8,
        protocol_v1: bool,
        review_window_secs: u32,
    ) -> Vec<u8> {
        account_bytes_for_protocol(
            status,
            submitted_at,
            revisions,
            u8::from(protocol_v1),
            review_window_secs,
        )
    }

    fn account_bytes_for_protocol(
        status: MilestoneStatus,
        submitted_at: i64,
        revisions: u8,
        protocol_version: u8,
        review_window_secs: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ACCOUNT_DATA_LEN);
        bytes.extend_from_slice(&account_discriminator());
        bytes.extend_from_slice(&[1; 32]);
        bytes.extend_from_slice(&[2; 32]);
        bytes.extend_from_slice(&[3; 32]);
        bytes.extend_from_slice(&[4; 32]);
        bytes.extend_from_slice(&[5; 32]);
        bytes.extend_from_slice(&[6; 32]);
        bytes.extend_from_slice(&1_000_000_u64.to_le_bytes());
        bytes.extend_from_slice(&10_000_i64.to_le_bytes());
        bytes.extend_from_slice(&review_window_secs.to_le_bytes());
        bytes.extend_from_slice(&submitted_at.to_le_bytes());
        bytes.push(match protocol_version {
            2 => PROTOCOL_V1_FLAG | PROTOCOL_V2_FLAG | revisions,
            1 => PROTOCOL_V1_FLAG | revisions,
            _ => revisions,
        });
        bytes.push(match status {
            MilestoneStatus::Open => 0,
            MilestoneStatus::Submitted => 1,
            MilestoneStatus::Paid => 2,
            MilestoneStatus::Refunded => 3,
        });
        bytes.push(254);
        assert_eq!(bytes.len(), ACCOUNT_DATA_LEN);
        bytes
    }

    fn account_bytes(status: MilestoneStatus, submitted_at: i64, revisions: u8) -> Vec<u8> {
        account_bytes_for_version(status, submitted_at, revisions, true)
    }

    fn rpc_response(status: MilestoneStatus, submitted_at: i64, revisions: u8) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "context": {
                    "apiVersion": "3.0.0",
                    "slot": 456
                },
                "value": {
                    "data": [
                        BASE64.encode(account_bytes(status, submitted_at, revisions)),
                        "base64"
                    ],
                    "executable": false,
                    "lamports": 2_000_000,
                    "owner": PRAZOPAY_PROGRAM_ID,
                    "rentEpoch": 0,
                    "space": ACCOUNT_DATA_LEN
                }
            }
        }))
        .unwrap()
    }

    fn report(status: MilestoneStatus, submitted_at: i64, revisions: u8, now: i64) -> StatusReport {
        inspect_account_response(&rpc_response(status, submitted_at, revisions))
            .unwrap()
            .report(&request(), now)
            .unwrap()
    }

    fn report_with_review_window(
        status: MilestoneStatus,
        submitted_at: i64,
        revisions: u8,
        now: i64,
        review_window_secs: u32,
    ) -> StatusReport {
        let response = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "context": {
                    "apiVersion": "3.0.0",
                    "slot": 456
                },
                "value": {
                    "data": [
                        BASE64.encode(account_bytes_for_review_window(
                            status,
                            submitted_at,
                            revisions,
                            true,
                            review_window_secs,
                        )),
                        "base64"
                    ],
                    "executable": false,
                    "lamports": 2_000_000,
                    "owner": PRAZOPAY_PROGRAM_ID,
                    "rentEpoch": 0,
                    "space": ACCOUNT_DATA_LEN
                }
            }
        }))
        .unwrap();

        inspect_account_response(&response)
            .unwrap()
            .report(&request(), now)
            .unwrap()
    }

    #[test]
    fn open_before_deadline_waits_for_worker() {
        let report = report(MilestoneStatus::Open, 0, 0, 9_000);
        assert_eq!(report.worker_actions, vec!["submit_delivery"]);
        assert!(report.funder_actions.is_empty());
        assert!(report.permissionless_actions.is_empty());
        assert_eq!(report.reason_codes, vec!["AWAITING_DELIVERY"]);
        assert!(!report.monitor.should_notify);
        assert_eq!(report.monitor.event_code, "AWAITING_DELIVERY_QUIET");
        assert_eq!(report.monitor.responsible_role.as_deref(), Some("worker"));
    }

    #[test]
    fn open_near_deadline_notifies_worker() {
        let report = report(MilestoneStatus::Open, 0, 0, 9_750);
        assert!(report.monitor.should_notify);
        assert_eq!(report.monitor.event_code, "DELIVERY_DEADLINE_FINAL");
        assert_eq!(report.monitor.seconds_to_boundary, Some(250));
    }

    #[test]
    fn open_after_deadline_exposes_only_immutable_refund() {
        let immediate = report(MilestoneStatus::Open, 0, 0, 10_001);
        assert_eq!(immediate.permissionless_actions, vec!["refund_expired"]);
        assert!(immediate.worker_actions.is_empty());
        assert_eq!(
            immediate.reason_codes,
            vec!["DEADLINE_ELAPSED_WITHOUT_SUBMISSION"]
        );
        assert!(immediate.monitor.should_notify);
        assert_eq!(immediate.monitor.event_code, "WORKER_DELIVERY_DELAYED");
        assert_eq!(
            immediate.monitor.responsible_role.as_deref(),
            Some("worker")
        );

        let recovered = report(MilestoneStatus::Open, 0, 0, 10_400);
        assert!(recovered.monitor.should_notify);
        assert_eq!(recovered.monitor.event_code, "WORKER_DELIVERY_DELAYED");
        assert_eq!(recovered.monitor.event_id, immediate.monitor.event_id);

        let escalated = report(MilestoneStatus::Open, 0, 0, 11_800);
        assert!(escalated.monitor.should_notify);
        assert_eq!(escalated.monitor.reminder_stage.as_deref(), Some("30m"));
    }

    #[test]
    fn submitted_during_review_exposes_approve_and_bounded_revision() {
        let report = report(MilestoneStatus::Submitted, 2_000, 0, 2_059);
        assert_eq!(
            report.funder_actions,
            vec!["approve_milestone", "request_revision"]
        );
        assert!(report.worker_actions.is_empty());
        assert_eq!(report.review_ends_at, Some(2_060));
        assert_eq!(report.reason_codes, vec!["REVIEW_WINDOW_OPEN"]);
        assert!(report.monitor.should_notify);
        assert_eq!(report.monitor.event_code, "FUNDER_REVIEW_REQUIRED");
    }

    #[test]
    fn submitted_after_review_enters_claim_grace() {
        let report = report(MilestoneStatus::Submitted, 2_000, 0, 2_060);
        assert_eq!(report.funder_actions, vec!["approve_milestone"]);
        assert!(report.worker_actions.is_empty());
        assert_eq!(report.claimable_at, Some(2_120));
        assert_eq!(report.reason_codes, vec!["CLAIM_GRACE_ACTIVE"]);
        assert_eq!(report.monitor.event_code, "FUNDER_REVIEW_DELAYED");
        assert_eq!(report.monitor.responsible_role.as_deref(), Some("funder"));
        assert_eq!(
            report.monitor.reminder_stage.as_deref(),
            Some("state_entry")
        );
    }

    #[test]
    fn permissionless_settlement_is_available_only_after_grace() {
        let report = report(MilestoneStatus::Submitted, 2_000, 0, 2_120);
        assert_eq!(report.worker_actions, vec!["claim_after_review"]);
        assert_eq!(report.permissionless_actions, vec!["settle_after_review"]);
        assert_eq!(report.reason_codes, vec!["SILENCE_ACCEPTANCE_SETTLEABLE"]);
        assert_eq!(
            report.monitor.event_code,
            "FUNDER_REVIEW_DELAYED_SETTLEMENT_READY"
        );
        assert_eq!(
            report.monitor.responsible_role.as_deref(),
            Some("funder_and_permissionless_trigger")
        );
        assert_eq!(
            report.monitor.reminder_stage.as_deref(),
            Some("funder_state_entry+settlement_state_entry")
        );
    }

    #[test]
    fn actionable_state_is_recoverable_and_relay_deduplicable() {
        let immediate = report(MilestoneStatus::Submitted, 2_000, 0, 2_120);
        let recovered = report(MilestoneStatus::Submitted, 2_000, 0, 2_500);
        let thirty_minutes = report(MilestoneStatus::Submitted, 2_000, 0, 3_920);

        assert!(immediate.monitor.should_notify);
        assert!(recovered.monitor.should_notify);
        assert_eq!(
            recovered.monitor.event_code,
            "FUNDER_REVIEW_DELAYED_SETTLEMENT_READY"
        );
        assert_eq!(recovered.monitor.event_id, immediate.monitor.event_id);
        assert!(thirty_minutes.monitor.should_notify);
        assert_eq!(
            thirty_minutes.monitor.event_code,
            "FUNDER_REVIEW_DELAYED_SETTLEMENT_READY"
        );
        assert_eq!(
            thirty_minutes.monitor.reminder_stage.as_deref(),
            Some("funder_30m+settlement_30m")
        );
        assert_ne!(immediate.monitor.event_id, thirty_minutes.monitor.event_id);
    }

    #[test]
    fn delay_schedule_preserves_funder_and_adds_permissionless_settlement() {
        let submitted_at = 2_000;
        let review_window_secs = 3_600;
        let review_ends_at = submitted_at + i64::from(review_window_secs);
        let claimable_at = review_ends_at + i64::from(review_window_secs);

        let first_funder_delay = report_with_review_window(
            MilestoneStatus::Submitted,
            submitted_at,
            0,
            review_ends_at,
            review_window_secs,
        );
        assert_eq!(
            first_funder_delay.monitor.event_code,
            "FUNDER_REVIEW_DELAYED"
        );
        assert_eq!(
            first_funder_delay.monitor.responsible_role.as_deref(),
            Some("funder")
        );
        assert_eq!(
            first_funder_delay.monitor.reminder_stage.as_deref(),
            Some("state_entry")
        );

        let funder_thirty_minutes = report_with_review_window(
            MilestoneStatus::Submitted,
            submitted_at,
            0,
            review_ends_at + 30 * 60,
            review_window_secs,
        );
        assert_eq!(
            funder_thirty_minutes.monitor.event_code,
            "FUNDER_REVIEW_DELAYED"
        );
        assert_eq!(
            funder_thirty_minutes.monitor.reminder_stage.as_deref(),
            Some("30m")
        );

        let settlement_ready = report_with_review_window(
            MilestoneStatus::Submitted,
            submitted_at,
            0,
            claimable_at,
            review_window_secs,
        );
        assert_eq!(
            settlement_ready.monitor.event_code,
            "PERMISSIONLESS_SETTLEMENT_READY"
        );
        assert_eq!(
            settlement_ready.monitor.responsible_role.as_deref(),
            Some("permissionless_trigger")
        );
        assert_eq!(
            settlement_ready.monitor.reminder_stage.as_deref(),
            Some("state_entry")
        );

        let funder_two_hours = report_with_review_window(
            MilestoneStatus::Submitted,
            submitted_at,
            0,
            review_ends_at + 2 * 60 * 60,
            review_window_secs,
        );
        assert_eq!(funder_two_hours.monitor.event_code, "FUNDER_REVIEW_DELAYED");
        assert_eq!(
            funder_two_hours.monitor.responsible_role.as_deref(),
            Some("funder")
        );
        assert_eq!(
            funder_two_hours.monitor.reminder_stage.as_deref(),
            Some("2h")
        );

        let settlement_two_hours = report_with_review_window(
            MilestoneStatus::Submitted,
            submitted_at,
            0,
            claimable_at + 2 * 60 * 60,
            review_window_secs,
        );
        assert_eq!(
            settlement_two_hours.monitor.event_code,
            "PERMISSIONLESS_SETTLEMENT_READY"
        );
        assert_eq!(
            settlement_two_hours.monitor.responsible_role.as_deref(),
            Some("permissionless_trigger")
        );
        assert_eq!(
            settlement_two_hours.monitor.reminder_stage.as_deref(),
            Some("2h")
        );
        assert_ne!(
            first_funder_delay.monitor.event_id,
            settlement_ready.monitor.event_id
        );
    }

    #[test]
    fn maximum_revision_removes_revision_action() {
        let report = report(MilestoneStatus::Submitted, 2_000, MAX_REVISIONS, 2_010);
        assert_eq!(report.funder_actions, vec!["approve_milestone"]);
        assert_eq!(report.reason_codes, vec!["REVISION_UNAVAILABLE"]);
    }

    #[test]
    fn terminal_states_expose_no_actions() {
        let paid = report(MilestoneStatus::Paid, 2_000, 0, 2_001);
        assert!(paid.funder_actions.is_empty());
        assert!(paid.worker_actions.is_empty());
        assert!(paid.permissionless_actions.is_empty());
        assert_eq!(paid.reason_codes, vec!["TERMINAL_PAID"]);
        assert_eq!(paid.terminal_at, Some(2_000));
        assert_eq!(paid.outcome, Some("success"));
        assert!(paid.monitor.should_notify);
        assert!(paid.monitor.continue_monitoring);
        assert_eq!(paid.monitor.event_code, "SETTLEMENT_SUCCESS");
        assert_eq!(paid.monitor.reminder_stage.as_deref(), Some("final"));

        let paid_after_extended_outage = report(MilestoneStatus::Paid, 2_000, 0, 200_000);
        assert!(paid_after_extended_outage.monitor.should_notify);
        assert!(paid_after_extended_outage.monitor.continue_monitoring);
        assert_eq!(
            paid_after_extended_outage.monitor.event_code,
            "SETTLEMENT_SUCCESS"
        );
        assert_eq!(
            paid.monitor.event_id,
            paid_after_extended_outage.monitor.event_id
        );

        let refunded = report(MilestoneStatus::Refunded, 10_001, 0, 10_002);
        assert!(refunded.funder_actions.is_empty());
        assert!(refunded.worker_actions.is_empty());
        assert!(refunded.permissionless_actions.is_empty());
        assert_eq!(refunded.reason_codes, vec!["TERMINAL_REFUNDED"]);
        assert_eq!(refunded.terminal_at, Some(10_001));
        assert_eq!(refunded.outcome, Some("failed"));
        assert!(refunded.monitor.should_notify);
        assert!(refunded.monitor.continue_monitoring);
        assert_eq!(refunded.monitor.event_code, "MILESTONE_FAILED");

        let refunded_after_extended_outage = report(MilestoneStatus::Refunded, 10_001, 0, 200_000);
        assert!(refunded_after_extended_outage.monitor.should_notify);
        assert!(refunded_after_extended_outage.monitor.continue_monitoring);
        assert_eq!(
            refunded_after_extended_outage.monitor.event_code,
            "MILESTONE_FAILED"
        );
        assert_eq!(
            refunded.monitor.event_id,
            refunded_after_extended_outage.monitor.event_id
        );
    }

    #[test]
    fn alert_window_is_bounded() {
        let mut low = request();
        low.alert_before_secs = Some(MIN_ALERT_BEFORE_SECS - 1);
        assert!(matches!(
            validate_request(&low),
            Err(StatusError::AlertWindowInvalid)
        ));

        let mut high = request();
        high.alert_before_secs = Some(MAX_ALERT_BEFORE_SECS + 1);
        assert!(matches!(
            validate_request(&high),
            Err(StatusError::AlertWindowInvalid)
        ));

        let mut invalid_poll = request();
        invalid_poll.poll_interval_secs = Some(MIN_POLL_INTERVAL_SECS - 1);
        assert!(matches!(
            validate_request(&invalid_poll),
            Err(StatusError::PollIntervalInvalid)
        ));
    }

    #[test]
    fn monitor_event_id_is_stable_and_state_bound() {
        let first = report(MilestoneStatus::Submitted, 2_000, 0, 2_010);
        let same_state_later = report(MilestoneStatus::Submitted, 2_000, 0, 2_020);
        let revised = report(MilestoneStatus::Submitted, 2_000, 1, 2_020);
        assert_eq!(first.monitor.event_id, same_state_later.monitor.event_id);
        assert_ne!(first.monitor.event_id, revised.monitor.event_id);
    }

    #[test]
    fn legacy_accounts_keep_the_deployed_v0_claim_timing() {
        let bytes = account_bytes_for_version(MilestoneStatus::Submitted, 2_000, 0, false);
        let response = serde_json::to_vec(&json!({
            "result": {
                "context": {"slot": 456},
                "value": {
                    "data": [BASE64.encode(bytes), "base64"],
                    "executable": false,
                    "owner": PRAZOPAY_PROGRAM_ID
                }
            }
        }))
        .unwrap();
        let report = inspect_account_response(&response)
            .unwrap()
            .report(&request(), 2_060)
            .unwrap();

        assert_eq!(report.protocol_version, "v0_legacy");
        assert_eq!(report.claim_grace_secs, 0);
        assert_eq!(report.worker_actions, vec!["claim_after_review"]);
        assert_eq!(report.reason_codes, vec!["LEGACY_REVIEW_ELAPSED_CLAIMABLE"]);
    }

    #[test]
    fn v2_accounts_report_worker_signed_acceptance_and_keep_v1_settlement_rules() {
        let bytes = account_bytes_for_protocol(MilestoneStatus::Submitted, 2_000, 1, 2, 60);
        let response = serde_json::to_vec(&json!({
            "result": {
                "context": {"slot": 456},
                "value": {
                    "data": [BASE64.encode(bytes), "base64"],
                    "executable": false,
                    "owner": PRAZOPAY_PROGRAM_ID
                }
            }
        }))
        .unwrap();
        let report = inspect_account_response(&response)
            .unwrap()
            .report(&request(), 2_120)
            .unwrap();

        assert_eq!(report.protocol_version, "v2");
        assert_eq!(report.acceptance_policy, "worker_signed_silence_acceptance");
        assert_eq!(report.revision_count, 1);
        assert_eq!(report.claim_grace_secs, 60);
        assert_eq!(report.worker_actions, vec!["claim_after_review"]);
        assert_eq!(report.permissionless_actions, vec!["settle_after_review"]);
        assert_eq!(report.reason_codes, vec!["SILENCE_ACCEPTANCE_SETTLEABLE"]);
    }

    #[test]
    fn legacy_terminal_accounts_never_invent_an_outcome_timestamp() {
        let bytes = account_bytes_for_version(MilestoneStatus::Paid, 2_000, 0, false);
        let response = serde_json::to_vec(&json!({
            "result": {
                "context": {"slot": 456},
                "value": {
                    "data": [BASE64.encode(bytes), "base64"],
                    "executable": false,
                    "owner": PRAZOPAY_PROGRAM_ID
                }
            }
        }))
        .unwrap();
        let report = inspect_account_response(&response)
            .unwrap()
            .report(&request(), 2_001)
            .unwrap();

        assert_eq!(report.protocol_version, "v0_legacy");
        assert_eq!(report.terminal_at, None);
        assert_eq!(report.outcome, Some("success"));
        assert!(!report.monitor.should_notify);
        assert_eq!(report.monitor.event_code, "TERMINAL_PAID_QUIET");
    }

    #[test]
    fn wrong_owner_discriminator_and_encoding_fail_closed() {
        let mut wrong_owner: Value =
            serde_json::from_slice(&rpc_response(MilestoneStatus::Open, 0, 0)).unwrap();
        wrong_owner["result"]["value"]["owner"] = json!(bs58::encode([8; 32]).into_string());
        assert!(matches!(
            inspect_account_response(&serde_json::to_vec(&wrong_owner).unwrap()),
            Err(StatusError::AccountOwnerInvalid)
        ));

        let mut bad_data = account_bytes(MilestoneStatus::Open, 0, 0);
        bad_data[0] ^= 0xff;
        let response = json!({
            "result": {
                "context": {"slot": 456},
                "value": {
                    "data": [BASE64.encode(bad_data), "base64"],
                    "executable": false,
                    "owner": PRAZOPAY_PROGRAM_ID
                }
            }
        });
        assert!(matches!(
            inspect_account_response(&serde_json::to_vec(&response).unwrap()),
            Err(StatusError::AccountDiscriminatorInvalid)
        ));

        let mut wrong_encoding: Value =
            serde_json::from_slice(&rpc_response(MilestoneStatus::Open, 0, 0)).unwrap();
        wrong_encoding["result"]["value"]["data"][1] = json!("base64+zstd");
        assert!(matches!(
            inspect_account_response(&serde_json::to_vec(&wrong_encoding).unwrap()),
            Err(StatusError::AccountEncodingInvalid)
        ));
    }

    #[test]
    fn block_time_must_be_present_and_numeric() {
        assert_eq!(
            parse_block_time_response(br#"{"jsonrpc":"2.0","id":2,"result":1234}"#).unwrap(),
            1234
        );
        assert!(matches!(
            parse_block_time_response(br#"{"jsonrpc":"2.0","id":2,"result":null}"#),
            Err(StatusError::BlockTimeUnavailable)
        ));
    }

    #[test]
    fn request_validation_rejects_wrong_network_and_extra_fields() {
        let mut mainnet = request();
        mainnet.cluster = "mainnet-beta".to_string();
        assert!(matches!(
            validate_request(&mainnet),
            Err(StatusError::ClusterUnsupported)
        ));

        let mut invalid_milestone = request();
        invalid_milestone.milestone = "not-base58-0".to_string();
        assert!(matches!(
            validate_request(&invalid_milestone),
            Err(StatusError::MilestoneInvalid)
        ));

        let unknown_field = format!(
            r#"{{"cluster":"devnet","milestone":"{}","rpc_url":"https://example.com"}}"#,
            request().milestone
        );
        assert!(serde_json::from_str::<StatusRequest>(&unknown_field).is_err());
    }

    #[test]
    fn exact_deadline_boundary_changes_only_after_the_deadline() {
        let at_deadline = report(MilestoneStatus::Open, 0, 0, 10_000);
        assert_eq!(at_deadline.worker_actions, vec!["submit_delivery"]);
        assert!(at_deadline.permissionless_actions.is_empty());

        let after_deadline = report(MilestoneStatus::Open, 0, 0, 10_001);
        assert!(after_deadline.worker_actions.is_empty());
        assert_eq!(
            after_deadline.permissionless_actions,
            vec!["refund_expired"]
        );
    }

    #[test]
    fn daily_action_reminders_are_sparse_and_day_bound() {
        let day_one = report(MilestoneStatus::Submitted, 2_000, 0, 2_120 + 24 * 60 * 60);
        let day_one_quiet = report(
            MilestoneStatus::Submitted,
            2_000,
            0,
            2_120 + 24 * 60 * 60 + 400,
        );
        let day_two = report(
            MilestoneStatus::Submitted,
            2_000,
            0,
            2_120 + 2 * 24 * 60 * 60,
        );

        assert_eq!(
            day_one.monitor.event_code,
            "FUNDER_REVIEW_DELAYED_SETTLEMENT_READY"
        );
        assert_eq!(
            day_one.monitor.reminder_stage.as_deref(),
            Some("funder_day_1+settlement_day_1")
        );
        assert!(day_one.monitor.should_notify);
        assert!(day_one_quiet.monitor.should_notify);
        assert_eq!(
            day_one_quiet.monitor.reminder_stage.as_deref(),
            Some("funder_state_entry+settlement_state_entry")
        );
        assert_eq!(
            day_two.monitor.reminder_stage.as_deref(),
            Some("funder_day_2+settlement_day_2")
        );
        assert_ne!(day_one.monitor.event_id, day_two.monitor.event_id);
    }

    #[test]
    fn impossible_review_timestamp_fails_closed() {
        let snapshot =
            inspect_account_response(&rpc_response(MilestoneStatus::Submitted, i64::MAX, 0))
                .unwrap();
        assert!(matches!(
            snapshot.report(&request(), i64::MAX),
            Err(StatusError::TimeOverflow)
        ));
    }
}
