use anchor_lang::prelude::*;

use crate::{
    constants::{AGREEMENT_SEED, MILESTONE_SEED},
    error::ErrorCode,
    events::{AgreementFunded, MilestoneCreated},
    state::{Agreement, Milestone},
};

#[derive(Accounts)]
pub struct FundAcceptedAgreement<'info> {
    #[account(
        mut,
        has_one = funder @ ErrorCode::UnauthorizedFunder,
        has_one = worker @ ErrorCode::UnauthorizedWorker,
        seeds = [
            AGREEMENT_SEED,
            agreement.funder.as_ref(),
            agreement.task_id.as_ref()
        ],
        bump = agreement.bump
    )]
    pub agreement: Account<'info, Agreement>,
    #[account(mut)]
    pub funder: Signer<'info>,
    pub worker: SystemAccount<'info>,
    #[account(
        init,
        payer = funder,
        space = 8 + Milestone::INIT_SPACE,
        seeds = [
            MILESTONE_SEED,
            agreement.funder.as_ref(),
            agreement.task_id.as_ref()
        ],
        bump
    )]
    pub milestone: Account<'info, Milestone>,
    pub system_program: Program<'info, System>,
}

pub fn handle_fund_accepted_agreement(ctx: Context<FundAcceptedAgreement>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let due_at = ctx.accounts.agreement.fund(
        ctx.accounts.funder.key(),
        ctx.accounts.milestone.key(),
        now,
    )?;

    let milestone = Milestone::new_v2(
        ctx.accounts.agreement.funder,
        ctx.accounts.agreement.worker,
        ctx.accounts.agreement.task_id,
        ctx.accounts.agreement.terms_hash,
        ctx.accounts.agreement.amount,
        due_at,
        ctx.accounts.agreement.review_window_secs,
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
        ctx.accounts.agreement.amount,
    )?;

    emit!(MilestoneCreated {
        milestone: ctx.accounts.milestone.key(),
        funder: ctx.accounts.agreement.funder,
        worker: ctx.accounts.agreement.worker,
        task_id: ctx.accounts.agreement.task_id,
        terms_hash: ctx.accounts.agreement.terms_hash,
        amount: ctx.accounts.agreement.amount,
        due_at,
        review_window_secs: ctx.accounts.agreement.review_window_secs,
        claim_grace_secs: ctx.accounts.milestone.claim_grace_secs(),
        silence_acceptance_acknowledged: ctx.accounts.agreement.silence_acceptance,
    });
    emit!(AgreementFunded {
        agreement: ctx.accounts.agreement.key(),
        milestone: ctx.accounts.milestone.key(),
        funder: ctx.accounts.agreement.funder,
        worker: ctx.accounts.agreement.worker,
        amount: ctx.accounts.agreement.amount,
        funded_at: now,
        due_at,
    });
    Ok(())
}
