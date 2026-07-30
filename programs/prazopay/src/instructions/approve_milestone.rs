use anchor_lang::prelude::*;

use crate::{
    constants::MILESTONE_SEED,
    error::ErrorCode,
    events::{MilestonePaid, SettlementKind},
    state::Milestone,
};

#[derive(Accounts)]
pub struct ApproveMilestone<'info> {
    #[account(
        mut,
        has_one = funder @ ErrorCode::UnauthorizedFunder,
        has_one = worker @ ErrorCode::UnauthorizedWorker,
        seeds = [
            MILESTONE_SEED,
            milestone.funder.as_ref(),
            milestone.task_id.as_ref()
        ],
        bump = milestone.bump
    )]
    pub milestone: Account<'info, Milestone>,
    pub funder: Signer<'info>,
    #[account(mut)]
    pub worker: SystemAccount<'info>,
}

pub fn handle_approve_milestone(ctx: Context<ApproveMilestone>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let amount = ctx.accounts.milestone.amount;
    ctx.accounts
        .milestone
        .approve(ctx.accounts.funder.key(), now)?;
    ctx.accounts.milestone.sub_lamports(amount)?;
    ctx.accounts.worker.add_lamports(amount)?;

    emit!(MilestonePaid {
        milestone: ctx.accounts.milestone.key(),
        worker: ctx.accounts.worker.key(),
        amount,
        kind: SettlementKind::FunderApproved,
        settled_at: now,
    });
    Ok(())
}
