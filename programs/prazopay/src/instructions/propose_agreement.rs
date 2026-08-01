use anchor_lang::prelude::*;

use crate::{constants::AGREEMENT_SEED, events::AgreementProposed, state::Agreement};

#[derive(Accounts)]
#[instruction(task_id: [u8; 32])]
pub struct ProposeAgreement<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,
    pub worker: SystemAccount<'info>,
    #[account(
        init,
        payer = funder,
        space = 8 + Agreement::INIT_SPACE,
        seeds = [AGREEMENT_SEED, funder.key().as_ref(), task_id.as_ref()],
        bump
    )]
    pub agreement: Account<'info, Agreement>,
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handle_propose_agreement(
    ctx: Context<ProposeAgreement>,
    task_id: [u8; 32],
    terms_hash: [u8; 32],
    amount: u64,
    delivery_window_secs: u32,
    review_window_secs: u32,
    funding_window_secs: u32,
    proposal_lifetime_secs: u32,
    silence_acceptance: bool,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let agreement = Agreement::new(
        ctx.accounts.funder.key(),
        ctx.accounts.worker.key(),
        task_id,
        terms_hash,
        amount,
        delivery_window_secs,
        review_window_secs,
        funding_window_secs,
        proposal_lifetime_secs,
        silence_acceptance,
        ctx.bumps.agreement,
        now,
    )?;
    ctx.accounts.agreement.set_inner(agreement);

    emit!(AgreementProposed {
        agreement: ctx.accounts.agreement.key(),
        funder: ctx.accounts.funder.key(),
        worker: ctx.accounts.worker.key(),
        task_id,
        terms_hash,
        amount,
        delivery_window_secs,
        review_window_secs,
        funding_window_secs,
        proposed_at: now,
        proposal_expires_at: ctx.accounts.agreement.proposal_expires_at,
        silence_acceptance,
    });
    Ok(())
}
