use anchor_lang::prelude::*;

use crate::{
    constants::{
        MAX_CLAIM_GRACE_SECS, MAX_FUNDING_WINDOW_SECS, MAX_MILESTONE_DURATION_SECS,
        MAX_PROPOSAL_LIFETIME_SECS, MAX_REVIEW_WINDOW_SECS, MAX_REVISIONS,
        MIN_DELIVERY_WINDOW_SECS, MIN_FUNDING_WINDOW_SECS, MIN_PROPOSAL_LIFETIME_SECS,
        MIN_REVIEW_WINDOW_SECS, PROTOCOL_V1_FLAG, PROTOCOL_V2_FLAG, REVISION_COUNT_MASK, ZERO_HASH,
    },
    error::ErrorCode,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgreementStatus {
    Proposed,
    Accepted,
    Funded,
    Rejected,
}

#[account]
#[derive(Debug)]
pub struct Agreement {
    pub funder: Pubkey,
    pub worker: Pubkey,
    pub task_id: [u8; 32],
    pub terms_hash: [u8; 32],
    pub amount: u64,
    pub delivery_window_secs: u32,
    pub review_window_secs: u32,
    pub funding_window_secs: u32,
    pub proposed_at: i64,
    pub proposal_expires_at: i64,
    pub accepted_at: i64,
    pub milestone: Pubkey,
    pub silence_acceptance: bool,
    pub status: AgreementStatus,
    pub bump: u8,
}

impl Agreement {
    pub const INIT_SPACE: usize = (32 * 5) + 8 + 4 + 4 + 4 + 8 + 8 + 8 + 1 + 1 + 1;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        funder: Pubkey,
        worker: Pubkey,
        task_id: [u8; 32],
        terms_hash: [u8; 32],
        amount: u64,
        delivery_window_secs: u32,
        review_window_secs: u32,
        funding_window_secs: u32,
        proposal_lifetime_secs: u32,
        silence_acceptance: bool,
        bump: u8,
        now: i64,
    ) -> Result<Self> {
        require!(amount > 0, ErrorCode::InvalidAmount);
        require!(funder != worker, ErrorCode::PartiesMustDiffer);
        require!(task_id != ZERO_HASH, ErrorCode::EmptyCommitment);
        require!(terms_hash != ZERO_HASH, ErrorCode::EmptyCommitment);
        require!(
            (MIN_DELIVERY_WINDOW_SECS..=MAX_MILESTONE_DURATION_SECS as u32)
                .contains(&delivery_window_secs),
            ErrorCode::InvalidDeliveryWindow
        );
        require!(
            (MIN_REVIEW_WINDOW_SECS..=MAX_REVIEW_WINDOW_SECS).contains(&review_window_secs),
            ErrorCode::InvalidReviewWindow
        );
        require!(
            (MIN_FUNDING_WINDOW_SECS..=MAX_FUNDING_WINDOW_SECS).contains(&funding_window_secs),
            ErrorCode::InvalidFundingWindow
        );
        require!(
            (MIN_PROPOSAL_LIFETIME_SECS..=MAX_PROPOSAL_LIFETIME_SECS)
                .contains(&proposal_lifetime_secs),
            ErrorCode::InvalidProposalLifetime
        );
        let proposal_expires_at = now
            .checked_add(i64::from(proposal_lifetime_secs))
            .ok_or(ErrorCode::TimeOverflow)?;
        require!(
            silence_acceptance,
            ErrorCode::SilenceAcceptanceNotAcknowledged
        );

        Ok(Self {
            funder,
            worker,
            task_id,
            terms_hash,
            amount,
            delivery_window_secs,
            review_window_secs,
            funding_window_secs,
            proposed_at: now,
            proposal_expires_at,
            accepted_at: 0,
            milestone: Pubkey::default(),
            silence_acceptance,
            status: AgreementStatus::Proposed,
            bump,
        })
    }

    pub fn accept(&mut self, actor: Pubkey, now: i64) -> Result<()> {
        require_keys_eq!(actor, self.worker, ErrorCode::UnauthorizedWorker);
        require!(
            self.status == AgreementStatus::Proposed,
            ErrorCode::InvalidAgreementStatus
        );
        require!(now <= self.proposal_expires_at, ErrorCode::AgreementExpired);
        self.accepted_at = now;
        self.status = AgreementStatus::Accepted;
        Ok(())
    }

    pub fn reject(&mut self, actor: Pubkey, now: i64) -> Result<()> {
        require_keys_eq!(actor, self.worker, ErrorCode::UnauthorizedWorker);
        require!(
            self.status == AgreementStatus::Proposed,
            ErrorCode::InvalidAgreementStatus
        );
        require!(now <= self.proposal_expires_at, ErrorCode::AgreementExpired);
        self.status = AgreementStatus::Rejected;
        Ok(())
    }

    pub fn funding_expires_at(&self) -> Result<i64> {
        require!(
            matches!(
                self.status,
                AgreementStatus::Accepted | AgreementStatus::Funded
            ),
            ErrorCode::InvalidAgreementStatus
        );
        self.accepted_at
            .checked_add(i64::from(self.funding_window_secs))
            .ok_or_else(|| ErrorCode::TimeOverflow.into())
    }

    pub fn fund(&mut self, actor: Pubkey, milestone: Pubkey, now: i64) -> Result<i64> {
        require_keys_eq!(actor, self.funder, ErrorCode::UnauthorizedFunder);
        require!(
            self.status == AgreementStatus::Accepted,
            ErrorCode::InvalidAgreementStatus
        );
        require!(
            now <= self.funding_expires_at()?,
            ErrorCode::FundingWindowExpired
        );
        require!(
            milestone != Pubkey::default(),
            ErrorCode::InvalidMilestoneAddress
        );
        let due_at = now
            .checked_add(i64::from(self.delivery_window_secs))
            .ok_or(ErrorCode::TimeOverflow)?;
        self.status = AgreementStatus::Funded;
        self.milestone = milestone;
        Ok(due_at)
    }
}

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

        Self::new_versioned(
            funder,
            worker,
            task_id,
            terms_hash,
            amount,
            due_at,
            review_window_secs,
            bump,
            now,
            PROTOCOL_V1_FLAG,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_v2(
        funder: Pubkey,
        worker: Pubkey,
        task_id: [u8; 32],
        terms_hash: [u8; 32],
        amount: u64,
        due_at: i64,
        review_window_secs: u32,
        bump: u8,
        now: i64,
    ) -> Result<Self> {
        Self::new_versioned(
            funder,
            worker,
            task_id,
            terms_hash,
            amount,
            due_at,
            review_window_secs,
            bump,
            now,
            PROTOCOL_V1_FLAG | PROTOCOL_V2_FLAG,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_versioned(
        funder: Pubkey,
        worker: Pubkey,
        task_id: [u8; 32],
        terms_hash: [u8; 32],
        amount: u64,
        due_at: i64,
        review_window_secs: u32,
        bump: u8,
        now: i64,
        version_bits: u8,
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
            revision_count: version_bits,
            status: MilestoneStatus::Open,
            bump,
        })
    }

    pub fn is_protocol_v1(&self) -> bool {
        self.revision_count & PROTOCOL_V1_FLAG != 0
    }

    pub fn is_protocol_v2(&self) -> bool {
        self.revision_count & (PROTOCOL_V1_FLAG | PROTOCOL_V2_FLAG)
            == PROTOCOL_V1_FLAG | PROTOCOL_V2_FLAG
    }

    pub fn protocol_version(&self) -> u8 {
        if self.is_protocol_v2() {
            2
        } else if self.is_protocol_v1() {
            1
        } else {
            0
        }
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
        let version_bits = self.revision_count & !REVISION_COUNT_MASK;
        self.revision_count = version_bits | next_revision;
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
