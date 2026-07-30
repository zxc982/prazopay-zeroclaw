use anchor_lang::prelude::*;

use crate::{
    constants::MILESTONE_SEED,
    error::ErrorCode,
    events::{MilestonePaid, SettlementKind},
    state::Milestone,
};

#[derive(Accounts)]
pub struct ClaimAfterReview<'info> {
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
    #[account(mut)]
    pub worker: Signer<'info>,
}

pub fn handle_claim_after_review(ctx: Context<ClaimAfterReview>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let amount = ctx.accounts.milestone.amount;
    ctx.accounts
        .milestone
        .claim_after_review(ctx.accounts.worker.key(), now)?;
    ctx.accounts.milestone.sub_lamports(amount)?;
    ctx.accounts.worker.add_lamports(amount)?;

    emit!(MilestonePaid {
        milestone: ctx.accounts.milestone.key(),
        worker: ctx.accounts.worker.key(),
        amount,
        kind: SettlementKind::SilenceAcceptanceClaimed,
        settled_at: now,
    });
    Ok(())
}
