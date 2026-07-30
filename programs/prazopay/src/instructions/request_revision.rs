use anchor_lang::prelude::*;

use crate::{
    constants::MILESTONE_SEED, error::ErrorCode, events::RevisionRequested, state::Milestone,
};

#[derive(Accounts)]
pub struct RequestRevision<'info> {
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
    pub funder: Signer<'info>,
}

pub fn handle_request_revision(
    ctx: Context<RequestRevision>,
    feedback_hash: [u8; 32],
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    ctx.accounts
        .milestone
        .request_revision(ctx.accounts.funder.key(), feedback_hash, now)?;

    emit!(RevisionRequested {
        milestone: ctx.accounts.milestone.key(),
        funder: ctx.accounts.funder.key(),
        feedback_hash,
        revision_count: ctx.accounts.milestone.revision_attempts(),
        revision_due_at: ctx.accounts.milestone.due_at,
    });
    Ok(())
}
