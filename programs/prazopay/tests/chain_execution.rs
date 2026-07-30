use {
    anchor_lang::{
        prelude::{Clock, Pubkey},
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    prazopay::state::{Milestone, MilestoneStatus},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const INITIAL_BALANCE: u64 = 5_000_000_000;
const AMOUNT: u64 = 1_000_000;
const REVIEW_WINDOW_SECS: u32 = 60;
const TASK_ID: [u8; 32] = [7; 32];
const TERMS_HASH: [u8; 32] = [8; 32];
const EVIDENCE_HASH: [u8; 32] = [9; 32];

fn new_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let program = include_bytes!("../../../fixtures/prazopay-v1.so");
    svm.add_program(prazopay::id(), program).unwrap();
    svm
}

fn send(svm: &mut LiteSVM, instruction: Instruction, payer: &Keypair) {
    let message = Message::new_with_blockhash(
        &[instruction],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );
    let transaction =
        VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[payer]).unwrap();
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

fn create_milestone(svm: &mut LiteSVM, funder: &Keypair, worker: &Keypair, due_at: i64) -> Pubkey {
    let milestone = milestone_address(&funder.pubkey());
    let instruction = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::CreateMilestone {
            task_id: TASK_ID,
            terms_hash: TERMS_HASH,
            amount: AMOUNT,
            due_at,
            review_window_secs: REVIEW_WINDOW_SECS,
            silence_acceptance_acknowledged: true,
        }
        .data(),
        prazopay::accounts::CreateMilestone {
            funder: funder.pubkey(),
            worker: worker.pubkey(),
            milestone,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(svm, instruction, funder);
    milestone
}

fn submit_delivery(svm: &mut LiteSVM, worker: &Keypair, milestone: Pubkey) {
    let instruction = Instruction::new_with_bytes(
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
    send(svm, instruction, worker);
}

fn read_milestone(svm: &LiteSVM, address: &Pubkey) -> Milestone {
    let account = svm.get_account(address).unwrap();
    let mut data: &[u8] = &account.data;
    Milestone::try_deserialize(&mut data).unwrap()
}

#[test]
fn funder_approval_moves_the_exact_locked_amount_to_worker() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    svm.airdrop(&funder.pubkey(), INITIAL_BALANCE).unwrap();
    svm.airdrop(&worker.pubkey(), INITIAL_BALANCE).unwrap();

    let due_at = clock(&svm).unix_timestamp + 3_600;
    let milestone = create_milestone(&mut svm, &funder, &worker, due_at);
    submit_delivery(&mut svm, &worker, milestone);
    let worker_before = svm.get_balance(&worker.pubkey()).unwrap();
    let settled_at = clock(&svm).unix_timestamp;

    let instruction = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::ApproveMilestone {}.data(),
        prazopay::accounts::ApproveMilestone {
            milestone,
            funder: funder.pubkey(),
            worker: worker.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, instruction, &funder);

    assert_eq!(
        svm.get_balance(&worker.pubkey()).unwrap(),
        worker_before + AMOUNT
    );
    let paid = read_milestone(&svm, &milestone);
    assert_eq!(paid.status, MilestoneStatus::Paid);
    assert_eq!(paid.submitted_at, settled_at);
}

#[test]
fn anyone_can_settle_after_silence_but_only_the_worker_receives_payment() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    let trigger = Keypair::new();
    svm.airdrop(&funder.pubkey(), INITIAL_BALANCE).unwrap();
    svm.airdrop(&worker.pubkey(), INITIAL_BALANCE).unwrap();
    svm.airdrop(&trigger.pubkey(), INITIAL_BALANCE).unwrap();

    let due_at = clock(&svm).unix_timestamp + 3_600;
    let milestone = create_milestone(&mut svm, &funder, &worker, due_at);
    submit_delivery(&mut svm, &worker, milestone);
    let submitted_at = read_milestone(&svm, &milestone).submitted_at;
    let escrow_before = svm.get_balance(&milestone).unwrap();
    let worker_before = svm.get_balance(&worker.pubkey()).unwrap();

    set_time(&mut svm, submitted_at + i64::from(REVIEW_WINDOW_SECS) * 2);
    let instruction = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::SettleAfterReview {}.data(),
        prazopay::accounts::SettleAfterReview {
            milestone,
            worker: worker.pubkey(),
            trigger: trigger.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, instruction, &trigger);

    assert_eq!(escrow_before - svm.get_balance(&milestone).unwrap(), AMOUNT);
    assert_eq!(
        svm.get_balance(&worker.pubkey()).unwrap(),
        worker_before + AMOUNT
    );
    let paid = read_milestone(&svm, &milestone);
    assert_eq!(paid.status, MilestoneStatus::Paid);
    assert_eq!(
        paid.submitted_at,
        submitted_at + i64::from(REVIEW_WINDOW_SECS) * 2
    );
}

#[test]
fn permissionless_settlement_rejects_a_substituted_worker_recipient() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    let trigger = Keypair::new();
    let attacker = Keypair::new();
    for key in [&funder, &worker, &trigger, &attacker] {
        svm.airdrop(&key.pubkey(), INITIAL_BALANCE).unwrap();
    }

    let due_at = clock(&svm).unix_timestamp + 3_600;
    let milestone = create_milestone(&mut svm, &funder, &worker, due_at);
    submit_delivery(&mut svm, &worker, milestone);
    let submitted_at = read_milestone(&svm, &milestone).submitted_at;
    set_time(&mut svm, submitted_at + i64::from(REVIEW_WINDOW_SECS) * 2);
    let worker_before = svm.get_balance(&worker.pubkey()).unwrap();
    let attacker_before = svm.get_balance(&attacker.pubkey()).unwrap();

    let instruction = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::SettleAfterReview {}.data(),
        prazopay::accounts::SettleAfterReview {
            milestone,
            worker: attacker.pubkey(),
            trigger: trigger.pubkey(),
        }
        .to_account_metas(None),
    );
    let message = Message::new_with_blockhash(
        &[instruction],
        Some(&trigger.pubkey()),
        &svm.latest_blockhash(),
    );
    let transaction =
        VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[&trigger]).unwrap();
    assert!(svm.send_transaction(transaction).is_err());

    assert_eq!(
        read_milestone(&svm, &milestone).status,
        MilestoneStatus::Submitted
    );
    assert_eq!(svm.get_balance(&worker.pubkey()).unwrap(), worker_before);
    assert_eq!(
        svm.get_balance(&attacker.pubkey()).unwrap(),
        attacker_before
    );
}

#[test]
fn anyone_can_trigger_expiry_but_only_the_funder_receives_the_refund() {
    let mut svm = new_svm();
    let funder = Keypair::new();
    let worker = Keypair::new();
    let trigger = Keypair::new();
    svm.airdrop(&funder.pubkey(), INITIAL_BALANCE).unwrap();
    svm.airdrop(&trigger.pubkey(), INITIAL_BALANCE).unwrap();

    let due_at = clock(&svm).unix_timestamp + 120;
    let milestone = create_milestone(&mut svm, &funder, &worker, due_at);
    let funder_before = svm.get_balance(&funder.pubkey()).unwrap();
    set_time(&mut svm, due_at + 1);

    let instruction = Instruction::new_with_bytes(
        prazopay::id(),
        &prazopay::instruction::RefundExpired {}.data(),
        prazopay::accounts::RefundExpired {
            milestone,
            funder: funder.pubkey(),
            trigger: trigger.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, instruction, &trigger);

    assert_eq!(
        svm.get_balance(&funder.pubkey()).unwrap(),
        funder_before + AMOUNT
    );
    let refunded = read_milestone(&svm, &milestone);
    assert_eq!(refunded.status, MilestoneStatus::Refunded);
    assert_eq!(refunded.submitted_at, due_at + 1);
}
