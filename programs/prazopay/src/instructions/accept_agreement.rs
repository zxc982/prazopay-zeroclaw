use anchor_lang::prelude::*;

use crate::{
    constants::AGREEMENT_SEED, error::ErrorCode, events::AgreementAccepted, state::Agreement,
};

#[derive(Accounts)]
pub struct AcceptAgreement<'info> {
    #[account(
        mut,
        has_one = worker @ ErrorCode::UnauthorizedWorker,
        seeds = [
            AGREEMENT_SEED,
            agreement.funder.as_ref(),
            agreement.task_id.as_ref()
        ],
        bump = agreement.bump
    )]
    pub agreement: Account<'info, Agreement>,
    pub worker: Signer<'info>,
}

pub fn handle_accept_agreement(ctx: Context<AcceptAgreement>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    ctx.accounts
        .agreement
        .accept(ctx.accounts.worker.key(), now)?;
    let funding_expires_at = ctx.accounts.agreement.funding_expires_at()?;

    emit!(AgreementAccepted {
        agreement: ctx.accounts.agreement.key(),
        worker: ctx.accounts.worker.key(),
        terms_hash: ctx.accounts.agreement.terms_hash,
        accepted_at: ctx.accounts.agreement.accepted_at,
        funding_expires_at,
    });
    Ok(())
}
