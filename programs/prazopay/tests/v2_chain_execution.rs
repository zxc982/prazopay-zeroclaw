use {
    anchor_lang::{
        prelude::{Clock, Pubkey},
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    prazopay::state::{Agreement, AgreementStatus, Milestone, MilestoneStatus},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    std::{env, fs},
};

const INITIAL_BALANCE: u64 = 5_000_000_000;
const AMOUNT: u64 = 1_000_000;
const DELIVERY_WINDOW_SECS: u32 = 3_600;
const REVIEW_WINDOW_SECS: u32 = 60;
const FUNDING_WINDOW_SECS: u32 = 600;
const TASK_ID: [u8; 32] = [21; 32];
const TERMS_HASH: [u8; 32] = [22; 32];
const EVIDENCE_HASH: [u8; 32] = [23; 32];

fn new_svm() -> LiteSVM {
    let program_path =
        env::var("PRAZOPAY_V2_SBF").expect("PRAZOPAY_V2_SBF must name the candidate SBF");
    let program = fs::read(program_path).expect("candidate SBF must be readable");
    let mut svm = LiteSVM::new();
    svm.add_program(prazopay::id(), &program).unwrap();
    svm
}

fn transaction(svm: &LiteSVM, instruction: Instruction, payer: &Keypair) -> VersionedTransaction {
    let message = Message::new_with_blockhash(
        &[instruction],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );
    VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[payer]).unwrap()
}

fn send(svm: &mut LiteSVM, instruction: Instruction, payer: &Keypair) {
    let transaction = transaction(svm, instruction, payer);
    svm.send_transaction(transaction).unwrap();
}

fn clock(svm: &LiteSVM) -> Clock {
    svm.get_sysvar()
}

fn set_time(svm: &mut LiteSVM, unix_timestamp: i64) {
    let mut clock = clock(svm);
    clock.unix_timestamp = unix_timestamp;
    svm.set_sysvar(&clock);
}

fn agreement_address(funder: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            prazopay::constants::AGREEMENT_SEED,
            funder.as_ref(),
            TASK_ID.as_ref(),
        ],
        &prazopay::id(),
    )
    .0
}

fn milestone_address(funder: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            prazopay::constants::MILESTONE_SEED,
            funder.as_ref(),
            TASK_ID.as_ref(),
        ],
        &prazopay::id(),
    )
    .0
}

fn read_agreement(svm: &LiteSVM, address: &Pubkey) -> Agreement {
    let account = svm.get_account(address).unwrap();
    let mut data: &[u8] = &account.data;
    Agreement::try_deserialize(&mut data).unwrap()
}

fn read_milestone(svm: &LiteSVM, address: &Pubkey) -> Milestone {
    let account = svm.get_account(address).unwrap();
    let mut data: &[u8] = &account.data;
    Milestone::try_deserialize(&mut data).unwrap()
}

#[test]
#[ignore = "requires a locally built candidate SBF via PRAZOPAY_V2_SBF"]
fn worker_acceptance_is_required_before_funding_and_payment() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    let trigger = Keypair::new();
    for key in [&funder, &worker, &trigger] {
        svm.airdrop(&key.pubkey(), INITIAL_BALANCE).unwrap();
    }

    let agreement = agreement_address(&funder.pubkey());
    let milestone = milestone_address(&funder.pubkey());
    let propose = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::ProposeAgreement {
            task_id: TASK_ID,
            terms_hash: TERMS_HASH,
            amount: AMOUNT,
            delivery_window_secs: DELIVERY_WINDOW_SECS,
            review_window_secs: REVIEW_WINDOW_SECS,
            funding_window_secs: FUNDING_WINDOW_SECS,
            proposal_lifetime_secs: 600,
            silence_acceptance: true,
        }
        .data(),
        prazopay::accounts::ProposeAgreement {
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            agreement,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, propose, &funder);
    assert_eq!(
        read_agreement(&svm, &agreement).status,
        AgreementStatus::Proposed
    );
    assert!(svm.get_account(&milestone).is_none());

    let fund = || {
        Instruction::new_with_bytes(
            prazopay::id(),
            &prazopay::instruction::FundAcceptedAgreement {}.data(),
            prazopay::accounts::FundAcceptedAgreement {
                agreement,
                funder: funder.pubkey(),
                worker: worker.pubkey(),
                milestone,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    };
    let premature_funding = transaction(&svm, fund(), &funder);
    assert!(svm.send_transaction(premature_funding).is_err());
    assert!(svm.get_account(&milestone).is_none());
    svm.expire_blockhash();

    let accept = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::AcceptAgreement {}.data(),
        prazopay::accounts::AcceptAgreement {
            agreement,
            worker: worker.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, accept, &worker);
    assert_eq!(
        read_agreement(&svm, &agreement).status,
        AgreementStatus::Accepted
    );

    let funded_at = clock(&svm).unix_timestamp;
    send(&mut svm, fund(), &funder);
    let funded_agreement = read_agreement(&svm, &agreement);
    let open = read_milestone(&svm, &milestone);
    assert_eq!(funded_agreement.status, AgreementStatus::Funded);
    assert_eq!(funded_agreement.milestone, milestone);
    assert_eq!(open.status, MilestoneStatus::Open);
    assert_eq!(open.protocol_version(), 2);
    assert_eq!(open.due_at, funded_at + i64::from(DELIVERY_WINDOW_SECS));
    assert_eq!(open.terms_hash, TERMS_HASH);

    let submit = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::SubmitDelivery {
            evidence_hash: EVIDENCE_HASH,
        }
        .data(),
        prazopay::accounts::SubmitDelivery {
            milestone,
            worker: worker.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, submit, &worker);
    let submitted = read_milestone(&svm, &milestone);
    assert_eq!(submitted.status, MilestoneStatus::Submitted);
    assert_eq!(submitted.evidence_hash, EVIDENCE_HASH);

    let worker_before = svm.get_balance(&worker.pubkey()).unwrap();
    set_time(
        &mut svm,
        submitted.submitted_at + i64::from(REVIEW_WINDOW_SECS) * 2,
    );
    let settle = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::SettleAfterReview {}.data(),
        prazopay::accounts::SettleAfterReview {
            milestone,
            worker: worker.pubkey(),
            trigger: trigger.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, settle, &trigger);

    assert_eq!(
        read_milestone(&svm, &milestone).status,
        MilestoneStatus::Paid
    );
    assert_eq!(
        svm.get_balance(&worker.pubkey()).unwrap(),
        worker_before + AMOUNT
    );
}

#[test]
#[ignore = "requires a locally built candidate SBF via PRAZOPAY_V2_SBF"]
fn substituted_worker_and_post_rejection_funding_fail_closed() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    let attacker = Keypair::new();
    for key in [&funder, &worker, &attacker] {
        svm.airdrop(&key.pubkey(), INITIAL_BALANCE).unwrap();
    }
    let agreement = agreement_address(&funder.pubkey());
    let milestone = milestone_address(&funder.pubkey());
    let propose = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::ProposeAgreement {
            task_id: TASK_ID,
            terms_hash: TERMS_HASH,
            amount: AMOUNT,
            delivery_window_secs: DELIVERY_WINDOW_SECS,
            review_window_secs: REVIEW_WINDOW_SECS,
            funding_window_secs: FUNDING_WINDOW_SECS,
            proposal_lifetime_secs: 600,
            silence_acceptance: true,
        }
        .data(),
        prazopay::accounts::ProposeAgreement {
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            agreement,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, propose, &funder);

    let substituted_accept = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::AcceptAgreement {}.data(),
        prazopay::accounts::AcceptAgreement {
            agreement,
            worker: attacker.pubkey(),
        }
        .to_account_metas(None),
    );
    let attempt = transaction(&svm, substituted_accept, &attacker);
    assert!(svm.send_transaction(attempt).is_err());
    assert_eq!(
        read_agreement(&svm, &agreement).status,
        AgreementStatus::Proposed
    );
    svm.expire_blockhash();

    let reject = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::RejectAgreement {}.data(),
        prazopay::accounts::RejectAgreement {
            agreement,
            worker: worker.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, reject, &worker);
    assert_eq!(
        read_agreement(&svm, &agreement).status,
        AgreementStatus::Rejected
    );

    let fund = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::FundAcceptedAgreement {}.data(),
        prazopay::accounts::FundAcceptedAgreement {
            agreement,
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            milestone,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let attempt = transaction(&svm, fund, &funder);
    assert!(svm.send_transaction(attempt).is_err());
    assert_eq!(
        read_agreement(&svm, &agreement).status,
        AgreementStatus::Rejected
    );
    assert!(svm.get_account(&milestone).is_none());
}

#[test]
#[ignore = "requires a locally built candidate SBF via PRAZOPAY_V2_SBF"]
fn last_second_acceptance_gets_full_funding_window_but_late_funding_fails() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    for key in [&funder, &worker] {
        svm.airdrop(&key.pubkey(), INITIAL_BALANCE).unwrap();
    }
    let agreement = agreement_address(&funder.pubkey());
    let milestone = milestone_address(&funder.pubkey());
    let propose = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::ProposeAgreement {
            task_id: TASK_ID,
            terms_hash: TERMS_HASH,
            amount: AMOUNT,
            delivery_window_secs: DELIVERY_WINDOW_SECS,
            review_window_secs: REVIEW_WINDOW_SECS,
            funding_window_secs: FUNDING_WINDOW_SECS,
            proposal_lifetime_secs: 600,
            silence_acceptance: true,
        }
        .data(),
        prazopay::accounts::ProposeAgreement {
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            agreement,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, propose, &funder);
    let proposal_expires_at = read_agreement(&svm, &agreement).proposal_expires_at;
    set_time(&mut svm, proposal_expires_at);

    let accept = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::AcceptAgreement {}.data(),
        prazopay::accounts::AcceptAgreement {
            agreement,
            worker: worker.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, accept, &worker);
    let accepted = read_agreement(&svm, &agreement);
    assert_eq!(accepted.accepted_at, proposal_expires_at);
    assert_eq!(
        accepted.funding_expires_at().unwrap(),
        proposal_expires_at + i64::from(FUNDING_WINDOW_SECS)
    );

    set_time(&mut svm, accepted.funding_expires_at().unwrap() + 1);
    let fund = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::FundAcceptedAgreement {}.data(),
        prazopay::accounts::FundAcceptedAgreement {
            agreement,
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            milestone,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let attempt = transaction(&svm, fund, &funder);
    assert!(svm.send_transaction(attempt).is_err());
    assert_eq!(
        read_agreement(&svm, &agreement).status,
        AgreementStatus::Accepted
    );
    assert!(svm.get_account(&milestone).is_none());
}

#[test]
#[ignore = "requires a locally built candidate SBF via PRAZOPAY_V2_SBF"]
fn failed_transfer_rolls_back_agreement_and_milestone_creation() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    for key in [&funder, &worker] {
        svm.airdrop(&key.pubkey(), INITIAL_BALANCE).unwrap();
    }
    let agreement = agreement_address(&funder.pubkey());
    let milestone = milestone_address(&funder.pubkey());
    let impossible_amount = INITIAL_BALANCE * 2;
    let propose = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::ProposeAgreement {
            task_id: TASK_ID,
            terms_hash: TERMS_HASH,
            amount: impossible_amount,
            delivery_window_secs: DELIVERY_WINDOW_SECS,
            review_window_secs: REVIEW_WINDOW_SECS,
            funding_window_secs: FUNDING_WINDOW_SECS,
            proposal_lifetime_secs: 600,
            silence_acceptance: true,
        }
        .data(),
        prazopay::accounts::ProposeAgreement {
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            agreement,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, propose, &funder);
    let accept = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::AcceptAgreement {}.data(),
        prazopay::accounts::AcceptAgreement {
            agreement,
            worker: worker.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, accept, &worker);
    let accepted_before = read_agreement(&svm, &agreement);

    let fund = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::FundAcceptedAgreement {}.data(),
        prazopay::accounts::FundAcceptedAgreement {
            agreement,
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            milestone,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let attempt = transaction(&svm, fund, &funder);
    assert!(svm.send_transaction(attempt).is_err());
    let agreement_after = read_agreement(&svm, &agreement);
    assert_eq!(agreement_after.status, AgreementStatus::Accepted);
    assert_eq!(agreement_after.milestone, Pubkey::default());
    assert_eq!(agreement_after.accepted_at, accepted_before.accepted_at);
    assert!(svm.get_account(&milestone).is_none());
}
