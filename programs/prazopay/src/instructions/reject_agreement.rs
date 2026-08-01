use anchor_lang::prelude::*;

use crate::{
    constants::AGREEMENT_SEED, error::ErrorCode, events::AgreementRejected, state::Agreement,
};

#[derive(Accounts)]
pub struct RejectAgreement<'info> {
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

pub fn handle_reject_agreement(ctx: Context<RejectAgreement>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    ctx.accounts
        .agreement
        .reject(ctx.accounts.worker.key(), now)?;

    emit!(AgreementRejected {
        agreement: ctx.accounts.agreement.key(),
        worker: ctx.accounts.worker.key(),
        rejected_at: now,
    });
    Ok(())
}
