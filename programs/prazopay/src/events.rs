use anchor_lang::prelude::*;

#[event]
pub struct MilestoneCreated {
    pub milestone: Pubkey,
    pub funder: Pubkey,
    pub worker: Pubkey,
    pub task_id: [u8; 32],
    pub terms_hash: [u8; 32],
    pub amount: u64,
    pub due_at: i64,
    pub review_window_secs: u32,
    pub claim_grace_secs: u32,
    pub silence_acceptance_acknowledged: bool,
}

#[event]
pub struct DeliverySubmitted {
    pub milestone: Pubkey,
    pub worker: Pubkey,
    pub evidence_hash: [u8; 32],
    pub submitted_at: i64,
    pub review_ends_at: i64,
    pub claimable_at: i64,
    pub revision_count: u8,
}

#[event]
pub struct RevisionRequested {
    pub milestone: Pubkey,
    pub funder: Pubkey,
    pub feedback_hash: [u8; 32],
    pub revision_count: u8,
    pub revision_due_at: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementKind {
    FunderApproved,
    SilenceAcceptanceClaimed,
    SilenceAcceptanceSettled,
}

#[event]
pub struct MilestonePaid {
    pub milestone: Pubkey,
    pub worker: Pubkey,
    pub amount: u64,
    pub kind: SettlementKind,
    pub settled_at: i64,
}

#[event]
pub struct MilestoneRefunded {
    pub milestone: Pubkey,
    pub funder: Pubkey,
    pub amount: u64,
    pub settled_at: i64,
}
