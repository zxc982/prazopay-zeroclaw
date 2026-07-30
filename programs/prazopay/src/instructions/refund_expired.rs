use anchor_lang::prelude::*;

use crate::{
    constants::MILESTONE_SEED, error::ErrorCode, events::MilestoneRefunded, state::Milestone,
};

#[derive(Accounts)]
pub struct RefundExpired<'info> {
    #[account(
        mut,
        has_one = funder @ ErrorCode::UnauthorizedFunder,
        seeds = [
            MILESTONE_SEED,
            milestone.funder.as_ref(),
            milestone.task_id.as_ref()
        ],
        bump = milestone.bump
    )]
    pub milestone: Account<'info, Milestone>,
    #[account(mut)]
    pub funder: SystemAccount<'info>,
    pub trigger: Signer<'info>,
}

pub fn handle_refund_expired(ctx: Context<RefundExpired>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let amount = ctx.accounts.milestone.amount;
    ctx.accounts.milestone.refund_expired(now)?;
    ctx.accounts.milestone.sub_lamports(amount)?;
    ctx.accounts.funder.add_lamports(amount)?;

    emit!(MilestoneRefunded {
        milestone: ctx.accounts.milestone.key(),
        funder: ctx.accounts.funder.key(),
        amount,
        settled_at: now,
    });
    Ok(())
}
