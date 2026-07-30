use anchor_lang::prelude::*;

use crate::{constants::MILESTONE_SEED, events::MilestoneCreated, state::Milestone};

#[derive(Accounts)]
#[instruction(task_id: [u8; 32])]
pub struct CreateMilestone<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,
    pub worker: SystemAccount<'info>,
    #[account(
        init,
        payer = funder,
        space = 8 + Milestone::INIT_SPACE,
        seeds = [MILESTONE_SEED, funder.key().as_ref(), task_id.as_ref()],
        bump
    )]
    pub milestone: Account<'info, Milestone>,
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handle_create_milestone(
    ctx: Context<CreateMilestone>,
    task_id: [u8; 32],
    terms_hash: [u8; 32],
    amount: u64,
    due_at: i64,
    review_window_secs: u32,
    silence_acceptance_acknowledged: bool,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let milestone = Milestone::new(
        ctx.accounts.funder.key(),
        ctx.accounts.worker.key(),
        task_id,
        terms_hash,
        amount,
        due_at,
        review_window_secs,
        silence_acceptance_acknowledged,
        ctx.bumps.milestone,
        now,
    )?;
    ctx.accounts.milestone.set_inner(milestone);

    let transfer_accounts = anchor_lang::system_program::Transfer {
        from: ctx.accounts.funder.to_account_info(),
        to: ctx.accounts.milestone.to_account_info(),
    };
    anchor_lang::system_program::transfer(
        CpiContext::new(anchor_lang::system_program::ID, transfer_accounts),
        amount,
    )?;

    emit!(MilestoneCreated {
        milestone: ctx.accounts.milestone.key(),
        funder: ctx.accounts.funder.key(),
        worker: ctx.accounts.worker.key(),
        task_id,
        terms_hash,
        amount,
        due_at,
        review_window_secs,
        claim_grace_secs: ctx.accounts.milestone.claim_grace_secs(),
        silence_acceptance_acknowledged,
    });
    Ok(())
}
