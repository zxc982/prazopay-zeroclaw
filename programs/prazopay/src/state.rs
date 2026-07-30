use anchor_lang::prelude::*;

use crate::{
    constants::{
        MAX_CLAIM_GRACE_SECS, MAX_MILESTONE_DURATION_SECS, MAX_REVIEW_WINDOW_SECS, MAX_REVISIONS,
        MIN_REVIEW_WINDOW_SECS, PROTOCOL_V1_FLAG, REVISION_COUNT_MASK, ZERO_HASH,
    },
    error::ErrorCode,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MilestoneStatus {
    Open,
    Submitted,
    Paid,
    Refunded,
}

#[account]
#[derive(Debug)]
pub struct Milestone {
    pub funder: Pubkey,
    pub worker: Pubkey,
    pub task_id: [u8; 32],
    pub terms_hash: [u8; 32],
    pub evidence_hash: [u8; 32],
    pub feedback_hash: [u8; 32],
    pub amount: u64,
    pub due_at: i64,
    pub review_window_secs: u32,
    pub submitted_at: i64,
    pub revision_count: u8,
    pub status: MilestoneStatus,
    pub bump: u8,
}

impl Milestone {
    pub const INIT_SPACE: usize = (32 * 6) + 8 + 8 + 4 + 8 + 1 + 1 + 1;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        funder: Pubkey,
        worker: Pubkey,
        task_id: [u8; 32],
        terms_hash: [u8; 32],
        amount: u64,
        due_at: i64,
        review_window_secs: u32,
        silence_acceptance_acknowledged: bool,
        bump: u8,
        now: i64,
    ) -> Result<Self> {
        require!(amount > 0, ErrorCode::InvalidAmount);
        require!(funder != worker, ErrorCode::PartiesMustDiffer);
        require!(task_id != ZERO_HASH, ErrorCode::EmptyCommitment);
        require!(terms_hash != ZERO_HASH, ErrorCode::EmptyCommitment);

        let max_due_at = now
            .checked_add(MAX_MILESTONE_DURATION_SECS)
            .ok_or(ErrorCode::TimeOverflow)?;
        require!(
            due_at > now && due_at <= max_due_at,
            ErrorCode::InvalidDeadline
        );
        require!(
            (MIN_REVIEW_WINDOW_SECS..=MAX_REVIEW_WINDOW_SECS).contains(&review_window_secs),
            ErrorCode::InvalidReviewWindow
        );
        require!(
            silence_acceptance_acknowledged,
            ErrorCode::SilenceAcceptanceNotAcknowledged
        );

        Ok(Self {
            funder,
            worker,
            task_id,
            terms_hash,
            evidence_hash: ZERO_HASH,
            feedback_hash: ZERO_HASH,
            amount,
            due_at,
            review_window_secs,
            submitted_at: 0,
            revision_count: PROTOCOL_V1_FLAG,
            status: MilestoneStatus::Open,
            bump,
        })
    }

    pub fn is_protocol_v1(&self) -> bool {
        self.revision_count & PROTOCOL_V1_FLAG != 0
    }

    pub fn revision_attempts(&self) -> u8 {
        self.revision_count & REVISION_COUNT_MASK
    }

    pub fn claim_grace_secs(&self) -> u32 {
        if self.is_protocol_v1() {
            self.review_window_secs.min(MAX_CLAIM_GRACE_SECS)
        } else {
            0
        }
    }

    pub fn review_ends_at(&self) -> Result<i64> {
        self.submitted_at
            .checked_add(i64::from(self.review_window_secs))
            .ok_or_else(|| ErrorCode::TimeOverflow.into())
    }

    pub fn claimable_at(&self) -> Result<i64> {
        self.review_ends_at()?
            .checked_add(i64::from(self.claim_grace_secs()))
            .ok_or_else(|| ErrorCode::TimeOverflow.into())
    }

    pub fn submit(&mut self, actor: Pubkey, evidence_hash: [u8; 32], now: i64) -> Result<()> {
        require_keys_eq!(actor, self.worker, ErrorCode::UnauthorizedWorker);
        require!(
            self.status == MilestoneStatus::Open,
            ErrorCode::InvalidStatus
        );
        require!(now <= self.due_at, ErrorCode::SubmissionDeadlinePassed);
        require!(evidence_hash != ZERO_HASH, ErrorCode::EmptyCommitment);

        self.evidence_hash = evidence_hash;
        self.feedback_hash = ZERO_HASH;
        self.submitted_at = now;
        self.status = MilestoneStatus::Submitted;
        Ok(())
    }

    pub fn request_revision(
        &mut self,
        actor: Pubkey,
        feedback_hash: [u8; 32],
        now: i64,
    ) -> Result<()> {
        require_keys_eq!(actor, self.funder, ErrorCode::UnauthorizedFunder);
        require!(
            self.status == MilestoneStatus::Submitted,
            ErrorCode::InvalidStatus
        );
        require!(
            self.revision_attempts() < MAX_REVISIONS,
            ErrorCode::MaxRevisionsReached
        );
        require!(feedback_hash != ZERO_HASH, ErrorCode::EmptyCommitment);
        require!(now < self.review_ends_at()?, ErrorCode::ReviewWindowElapsed);
        if self.is_protocol_v1() {
            // A revision request starts a fresh delivery window of the same
            // duration as the review window. A submission at the original
            // deadline therefore never truncates the funder's review right.
            self.due_at = now
                .checked_add(i64::from(self.review_window_secs))
                .ok_or(ErrorCode::TimeOverflow)?;
        } else {
            // Preserve the deployed v0 behavior for legacy accounts.
            require!(now < self.due_at, ErrorCode::SubmissionDeadlinePassed);
        }

        self.feedback_hash = feedback_hash;
        self.evidence_hash = ZERO_HASH;
        self.submitted_at = 0;
        let next_revision = self
            .revision_attempts()
            .checked_add(1)
            .ok_or(ErrorCode::MaxRevisionsReached)?;
        self.revision_count = if self.is_protocol_v1() {
            PROTOCOL_V1_FLAG | next_revision
        } else {
            next_revision
        };
        self.status = MilestoneStatus::Open;
        Ok(())
    }

    pub fn approve(&mut self, actor: Pubkey, now: i64) -> Result<()> {
        require_keys_eq!(actor, self.funder, ErrorCode::UnauthorizedFunder);
        require!(
            self.status == MilestoneStatus::Submitted,
            ErrorCode::InvalidStatus
        );
        if self.is_protocol_v1() {
            // In terminal v1 states this otherwise inactive field stores the
            // outcome time. Reuse preserves the deployed account size and
            // leaves legacy v0 account semantics unchanged.
            self.submitted_at = now;
        }
        self.status = MilestoneStatus::Paid;
        Ok(())
    }

    pub fn claim_after_review(&mut self, actor: Pubkey, now: i64) -> Result<()> {
        require_keys_eq!(actor, self.worker, ErrorCode::UnauthorizedWorker);
        self.complete_silence_settlement(now)
    }

    pub fn settle_after_review(&mut self, now: i64) -> Result<()> {
        require!(
            self.is_protocol_v1(),
            ErrorCode::SilenceAcceptanceNotAcknowledged
        );
        self.complete_silence_settlement(now)
    }

    fn complete_silence_settlement(&mut self, now: i64) -> Result<()> {
        require!(
            self.status == MilestoneStatus::Submitted,
            ErrorCode::InvalidStatus
        );
        let review_ends_at = self.review_ends_at()?;
        let claimable_at = self.claimable_at()?;
        require!(now >= review_ends_at, ErrorCode::ReviewWindowStillOpen);
        require!(now >= claimable_at, ErrorCode::ClaimGraceStillOpen);
        if self.is_protocol_v1() {
            self.submitted_at = now;
        }
        self.status = MilestoneStatus::Paid;
        Ok(())
    }

    pub fn refund_expired(&mut self, now: i64) -> Result<()> {
        require!(
            self.status == MilestoneStatus::Open,
            ErrorCode::InvalidStatus
        );
        require!(now > self.due_at, ErrorCode::DeadlineStillOpen);
        if self.is_protocol_v1() {
            self.submitted_at = now;
        }
        self.status = MilestoneStatus::Refunded;
        Ok(())
    }
}
