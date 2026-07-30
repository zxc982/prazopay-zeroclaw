use anchor_lang::prelude::*;

use crate::{
    constants::MILESTONE_SEED,
    error::ErrorCode,
    events::{MilestonePaid, SettlementKind},
    state::Milestone,
};

#[derive(Accounts)]
pub struct SettleAfterReview<'info> {
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
    pub worker: SystemAccount<'info>,
    pub trigger: Signer<'info>,
}

pub fn handle_settle_after_review(ctx: Context<SettleAfterReview>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let amount = ctx.accounts.milestone.amount;
    ctx.accounts.milestone.settle_after_review(now)?;
    ctx.accounts.milestone.sub_lamports(amount)?;
    ctx.accounts.worker.add_lamports(amount)?;

    emit!(MilestonePaid {
        milestone: ctx.accounts.milestone.key(),
        worker: ctx.accounts.worker.key(),
        amount,
        kind: SettlementKind::SilenceAcceptanceSettled,
        settled_at: now,
    });
    Ok(())
}
