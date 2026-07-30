use anchor_lang::prelude::*;

use crate::{
    constants::MILESTONE_SEED, error::ErrorCode, events::DeliverySubmitted, state::Milestone,
};

#[derive(Accounts)]
pub struct SubmitDelivery<'info> {
    #[account(
        mut,
        has_one = worker @ ErrorCode::UnauthorizedWorker,
        seeds = [
            MILESTONE_SEED,
            milestone.funder.as_ref(),
            milestone.task_id.as_ref()
        ],
        bump = milestone.bump
    )]
    pub milestone: Account<'info, Milestone>,
    pub worker: Signer<'info>,
}

pub fn handle_submit_delivery(ctx: Context<SubmitDelivery>, evidence_hash: [u8; 32]) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    ctx.accounts
        .milestone
        .submit(ctx.accounts.worker.key(), evidence_hash, now)?;
    let review_ends_at = ctx.accounts.milestone.review_ends_at()?;
    let claimable_at = ctx.accounts.milestone.claimable_at()?;

    emit!(DeliverySubmitted {
        milestone: ctx.accounts.milestone.key(),
        worker: ctx.accounts.worker.key(),
        evidence_hash,
        submitted_at: now,
        review_ends_at,
        claimable_at,
        revision_count: ctx.accounts.milestone.revision_attempts(),
    });
    Ok(())
}
