pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm");

#[program]
pub mod prazopay {
    use super::*;

    pub fn create_milestone(
        ctx: Context<CreateMilestone>,
        task_id: [u8; 32],
        terms_hash: [u8; 32],
        amount: u64,
        due_at: i64,
        review_window_secs: u32,
        silence_acceptance_acknowledged: bool,
    ) -> Result<()> {
        instructions::create_milestone::handle_create_milestone(
            ctx,
            task_id,
            terms_hash,
            amount,
            due_at,
            review_window_secs,
            silence_acceptance_acknowledged,
        )
    }

    pub fn submit_delivery(ctx: Context<SubmitDelivery>, evidence_hash: [u8; 32]) -> Result<()> {
        instructions::submit_delivery::handle_submit_delivery(ctx, evidence_hash)
    }

    pub fn request_revision(ctx: Context<RequestRevision>, feedback_hash: [u8; 32]) -> Result<()> {
        instructions::request_revision::handle_request_revision(ctx, feedback_hash)
    }

    pub fn approve_milestone(ctx: Context<ApproveMilestone>) -> Result<()> {
        instructions::approve_milestone::handle_approve_milestone(ctx)
    }

    pub fn claim_after_review(ctx: Context<ClaimAfterReview>) -> Result<()> {
        instructions::claim_after_review::handle_claim_after_review(ctx)
    }

    pub fn settle_after_review(ctx: Context<SettleAfterReview>) -> Result<()> {
        instructions::settle_after_review::handle_settle_after_review(ctx)
    }

    pub fn refund_expired(ctx: Context<RefundExpired>) -> Result<()> {
        instructions::refund_expired::handle_refund_expired(ctx)
    }
}
