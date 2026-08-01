use anchor_lang::prelude::Pubkey;
use prazopay::{
    constants::{
        MAX_CLAIM_GRACE_SECS, MAX_FUNDING_WINDOW_SECS, MAX_MILESTONE_DURATION_SECS,
        MAX_PROPOSAL_LIFETIME_SECS, MAX_REVIEW_WINDOW_SECS, MAX_REVISIONS,
        MIN_DELIVERY_WINDOW_SECS, MIN_FUNDING_WINDOW_SECS, MIN_PROPOSAL_LIFETIME_SECS,
        MIN_REVIEW_WINDOW_SECS,
    },
    state::{Agreement, AgreementStatus, Milestone, MilestoneStatus},
};

const CREATED_AT: i64 = 1_000;
const DUE_AT: i64 = 10_000;
const AMOUNT: u64 = 1_000_000;

fn funder() -> Pubkey {
    Pubkey::new_from_array([1; 32])
}

fn worker() -> Pubkey {
    Pubkey::new_from_array([2; 32])
}

fn attacker() -> Pubkey {
    Pubkey::new_from_array([3; 32])
}

fn milestone() -> Milestone {
    Milestone::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        DUE_AT,
        MIN_REVIEW_WINDOW_SECS,
        true,
        254,
        CREATED_AT,
    )
    .unwrap()
}

fn agreement() -> Agreement {
    Agreement::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        600,
        600,
        true,
        253,
        CREATED_AT,
    )
    .unwrap()
}

#[test]
fn agreement_proposal_freezes_terms_before_funding() {
    let agreement = agreement();

    assert_eq!(Agreement::INIT_SPACE, 207);
    assert_eq!(agreement.funder, funder());
    assert_eq!(agreement.worker, worker());
    assert_eq!(agreement.task_id, [4; 32]);
    assert_eq!(agreement.terms_hash, [5; 32]);
    assert_eq!(agreement.amount, AMOUNT);
    assert_eq!(agreement.delivery_window_secs, 3_600);
    assert_eq!(agreement.review_window_secs, MIN_REVIEW_WINDOW_SECS);
    assert_eq!(agreement.funding_window_secs, 600);
    assert_eq!(agreement.proposed_at, CREATED_AT);
    assert_eq!(agreement.proposal_expires_at, CREATED_AT + 600);
    assert_eq!(agreement.accepted_at, 0);
    assert_eq!(agreement.milestone, Pubkey::default());
    assert!(agreement.silence_acceptance);
    assert_eq!(agreement.status, AgreementStatus::Proposed);
}

#[test]
fn agreement_requires_valid_parties_commitments_windows_and_policy() {
    let valid = |funder_key,
                 worker_key,
                 task_id,
                 terms_hash,
                 amount,
                 delivery_window,
                 review_window,
                 funding_window,
                 proposal_lifetime,
                 silence_acceptance| {
        Agreement::new(
            funder_key,
            worker_key,
            task_id,
            terms_hash,
            amount,
            delivery_window,
            review_window,
            funding_window,
            proposal_lifetime,
            silence_acceptance,
            1,
            CREATED_AT,
        )
    };

    assert!(valid(
        funder(),
        funder(),
        [4; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS,
        600,
        true,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [0; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS,
        600,
        true,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        0,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS,
        600,
        true,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        MIN_DELIVERY_WINDOW_SECS - 1,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS,
        600,
        true,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS,
        MIN_PROPOSAL_LIFETIME_SECS - 1,
        true,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS,
        MAX_PROPOSAL_LIFETIME_SECS + 1,
        true,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS,
        600,
        false,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MIN_FUNDING_WINDOW_SECS - 1,
        600,
        true,
    )
    .is_err());
    assert!(valid(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        3_600,
        MIN_REVIEW_WINDOW_SECS,
        MAX_FUNDING_WINDOW_SECS + 1,
        600,
        true,
    )
    .is_err());
}

#[test]
fn only_worker_can_accept_or_reject_before_expiry() {
    let mut accepted = agreement();
    assert!(accepted.accept(attacker(), CREATED_AT + 1).is_err());
    accepted.accept(worker(), CREATED_AT + 1).unwrap();
    assert_eq!(accepted.status, AgreementStatus::Accepted);
    assert_eq!(accepted.accepted_at, CREATED_AT + 1);
    assert!(accepted.reject(worker(), CREATED_AT + 2).is_err());

    let mut rejected = agreement();
    rejected.reject(worker(), CREATED_AT + 1).unwrap();
    assert_eq!(rejected.status, AgreementStatus::Rejected);
    assert!(rejected.accept(worker(), CREATED_AT + 2).is_err());

    let mut expired = agreement();
    assert!(expired
        .accept(worker(), expired.proposal_expires_at + 1)
        .is_err());
    assert!(expired
        .reject(worker(), expired.proposal_expires_at + 1)
        .is_err());
}

#[test]
fn funding_requires_worker_acceptance_and_starts_a_full_delivery_window() {
    let mut accepted = agreement();
    let milestone = Pubkey::new_unique();
    assert!(accepted.fund(funder(), milestone, CREATED_AT + 1).is_err());
    accepted.accept(worker(), CREATED_AT + 2).unwrap();
    assert!(accepted
        .fund(funder(), Pubkey::default(), CREATED_AT + 3)
        .is_err());
    assert!(accepted
        .fund(attacker(), milestone, CREATED_AT + 3)
        .is_err());

    let funded_at = CREATED_AT + 4;
    let due_at = accepted.fund(funder(), milestone, funded_at).unwrap();
    assert_eq!(due_at, funded_at + i64::from(accepted.delivery_window_secs));
    assert_eq!(accepted.status, AgreementStatus::Funded);
    assert_eq!(accepted.milestone, milestone);
    assert!(accepted.fund(funder(), milestone, funded_at + 1).is_err());

    let mut expired = agreement();
    expired.accept(worker(), CREATED_AT + 2).unwrap();
    let funding_expires_at = expired.funding_expires_at().unwrap();
    assert!(expired
        .fund(funder(), milestone, funding_expires_at)
        .is_ok());

    let mut expired = agreement();
    expired.accept(worker(), CREATED_AT + 2).unwrap();
    let funding_expires_at = expired.funding_expires_at().unwrap();
    assert!(expired
        .fund(funder(), milestone, funding_expires_at + 1)
        .is_err());
}

#[test]
fn creation_freezes_parties_terms_amount_and_deadline() {
    let milestone = milestone();

    assert_eq!(Milestone::INIT_SPACE, 223);
    assert_eq!(milestone.funder, funder());
    assert_eq!(milestone.worker, worker());
    assert_eq!(milestone.task_id, [4; 32]);
    assert_eq!(milestone.terms_hash, [5; 32]);
    assert_eq!(milestone.amount, AMOUNT);
    assert_eq!(milestone.due_at, DUE_AT);
    assert_eq!(milestone.review_window_secs, MIN_REVIEW_WINDOW_SECS);
    assert!(milestone.is_protocol_v1());
    assert_eq!(milestone.revision_attempts(), 0);
    assert_eq!(milestone.claim_grace_secs(), MIN_REVIEW_WINDOW_SECS);
    assert_eq!(milestone.status, MilestoneStatus::Open);
}

#[test]
fn v2_milestones_reuse_the_legacy_layout_and_decode_version_bits() {
    let mut milestone = Milestone::new_v2(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        DUE_AT,
        MIN_REVIEW_WINDOW_SECS,
        252,
        CREATED_AT,
    )
    .unwrap();

    assert_eq!(Milestone::INIT_SPACE, 223);
    assert!(milestone.is_protocol_v1());
    assert!(milestone.is_protocol_v2());
    assert_eq!(milestone.protocol_version(), 2);
    assert_eq!(milestone.revision_attempts(), 0);

    milestone.submit(worker(), [6; 32], CREATED_AT + 1).unwrap();
    milestone
        .request_revision(funder(), [7; 32], CREATED_AT + 2)
        .unwrap();
    assert_eq!(milestone.protocol_version(), 2);
    assert_eq!(milestone.revision_attempts(), 1);
}

#[test]
fn upgraded_program_preserves_legacy_account_timing() {
    let mut legacy = milestone();
    legacy.revision_count = 0;
    assert!(!legacy.is_protocol_v1());
    assert_eq!(legacy.claim_grace_secs(), 0);

    legacy.submit(worker(), [6; 32], DUE_AT).unwrap();
    assert!(legacy.request_revision(funder(), [7; 32], DUE_AT).is_err());
    legacy
        .claim_after_review(worker(), DUE_AT + i64::from(MIN_REVIEW_WINDOW_SECS))
        .expect("legacy accounts retain the deployed zero-grace claim boundary");
    assert_eq!(legacy.status, MilestoneStatus::Paid);
}

#[test]
fn invalid_creation_fails_closed() {
    assert!(Milestone::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        0,
        DUE_AT,
        MIN_REVIEW_WINDOW_SECS,
        true,
        1,
        CREATED_AT,
    )
    .is_err());
    assert!(Milestone::new(
        funder(),
        funder(),
        [4; 32],
        [5; 32],
        AMOUNT,
        DUE_AT,
        MIN_REVIEW_WINDOW_SECS,
        true,
        1,
        CREATED_AT,
    )
    .is_err());
    assert!(Milestone::new(
        funder(),
        worker(),
        [0; 32],
        [5; 32],
        AMOUNT,
        DUE_AT,
        MIN_REVIEW_WINDOW_SECS,
        true,
        1,
        CREATED_AT,
    )
    .is_err());
    assert!(Milestone::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        CREATED_AT,
        MIN_REVIEW_WINDOW_SECS,
        true,
        1,
        CREATED_AT,
    )
    .is_err());
    assert!(Milestone::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        CREATED_AT + MAX_MILESTONE_DURATION_SECS + 1,
        MIN_REVIEW_WINDOW_SECS,
        true,
        1,
        CREATED_AT,
    )
    .is_err());
    assert!(Milestone::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        DUE_AT,
        MIN_REVIEW_WINDOW_SECS,
        false,
        1,
        CREATED_AT,
    )
    .is_err());
}

#[test]
fn only_worker_can_submit_and_only_before_deadline() {
    let mut milestone = milestone();

    assert!(milestone
        .submit(attacker(), [6; 32], CREATED_AT + 1)
        .is_err());
    assert!(milestone.submit(worker(), [0; 32], CREATED_AT + 1).is_err());
    assert!(milestone.submit(worker(), [6; 32], DUE_AT + 1).is_err());

    milestone
        .submit(worker(), [6; 32], DUE_AT)
        .expect("submission at the exact deadline is valid");
    assert_eq!(milestone.status, MilestoneStatus::Submitted);
    assert_eq!(milestone.evidence_hash, [6; 32]);
    assert_eq!(milestone.submitted_at, DUE_AT);
}

#[test]
fn funder_approval_is_terminal() {
    let mut milestone = milestone();
    milestone.submit(worker(), [6; 32], CREATED_AT + 1).unwrap();

    let settled_at = CREATED_AT + 2;
    assert!(milestone.approve(attacker(), settled_at).is_err());
    milestone.approve(funder(), settled_at).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Paid);
    assert_eq!(milestone.submitted_at, settled_at);
    assert!(milestone.approve(funder(), settled_at + 1).is_err());
    assert!(milestone.refund_expired(DUE_AT + 1).is_err());
}

#[test]
fn silent_acceptance_requires_review_and_claim_grace_to_elapse() {
    let mut milestone = milestone();
    let submitted_at = CREATED_AT + 10;
    milestone.submit(worker(), [6; 32], submitted_at).unwrap();

    assert!(milestone
        .claim_after_review(
            worker(),
            submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS) - 1,
        )
        .is_err());
    assert!(milestone
        .claim_after_review(attacker(), submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS),)
        .is_err());
    assert!(milestone
        .claim_after_review(worker(), submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS))
        .is_err());
    milestone
        .claim_after_review(
            worker(),
            submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS) * 2,
        )
        .unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Paid);
    assert_eq!(
        milestone.submitted_at,
        submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS) * 2
    );
}

#[test]
fn permissionless_settlement_requires_an_acknowledged_protocol_and_claim_grace() {
    let submitted_at = CREATED_AT + 10;
    let claimable_at = submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS) * 2;
    let mut current = milestone();
    current.submit(worker(), [6; 32], submitted_at).unwrap();

    assert!(current.settle_after_review(claimable_at - 1).is_err());
    current.settle_after_review(claimable_at).unwrap();
    assert_eq!(current.status, MilestoneStatus::Paid);
    assert_eq!(current.submitted_at, claimable_at);

    let mut legacy = milestone();
    legacy.revision_count = 0;
    legacy.submit(worker(), [6; 32], submitted_at).unwrap();
    assert!(legacy
        .settle_after_review(submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS))
        .is_err());
    assert_eq!(legacy.status, MilestoneStatus::Submitted);
}

#[test]
fn revision_is_bounded_and_gets_a_fresh_delivery_window() {
    let mut milestone = milestone();
    let original_due_at = milestone.due_at;

    for revision in 1..=MAX_REVISIONS {
        let submitted_at = milestone.due_at;
        milestone
            .submit(worker(), [revision; 32], submitted_at)
            .unwrap();
        milestone
            .request_revision(funder(), [revision + 10; 32], submitted_at + 1)
            .unwrap();
        assert_eq!(milestone.status, MilestoneStatus::Open);
        assert_eq!(milestone.revision_attempts(), revision);
        assert_eq!(
            milestone.due_at,
            submitted_at + 1 + i64::from(MIN_REVIEW_WINDOW_SECS)
        );
        assert_eq!(milestone.evidence_hash, [0; 32]);
    }
    assert!(milestone.due_at > original_due_at);

    milestone
        .submit(worker(), [99; 32], milestone.due_at)
        .unwrap();
    assert!(milestone
        .request_revision(funder(), [98; 32], milestone.submitted_at + 1)
        .is_err());
}

#[test]
fn review_window_is_complete_even_for_submission_at_delivery_deadline() {
    let mut review_elapsed = milestone();
    let submitted_at = CREATED_AT + 10;
    review_elapsed
        .submit(worker(), [6; 32], submitted_at)
        .unwrap();
    assert!(review_elapsed
        .request_revision(
            funder(),
            [7; 32],
            submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS),
        )
        .is_err());

    let mut late_submission = milestone();
    late_submission.submit(worker(), [6; 32], DUE_AT).unwrap();
    late_submission
        .request_revision(
            funder(),
            [7; 32],
            DUE_AT + i64::from(MIN_REVIEW_WINDOW_SECS) - 1,
        )
        .expect("the original delivery deadline must not truncate review");
    assert_eq!(
        late_submission.due_at,
        DUE_AT + i64::from(MIN_REVIEW_WINDOW_SECS) * 2 - 1
    );
}

#[test]
fn expiry_refund_requires_open_state_and_time_after_deadline() {
    let mut open = milestone();
    assert!(open.refund_expired(DUE_AT).is_err());
    open.refund_expired(DUE_AT + 1).unwrap();
    assert_eq!(open.status, MilestoneStatus::Refunded);
    assert_eq!(open.submitted_at, DUE_AT + 1);
    assert!(open.refund_expired(DUE_AT + 2).is_err());

    let mut submitted = milestone();
    submitted.submit(worker(), [6; 32], DUE_AT).unwrap();
    assert!(submitted.refund_expired(DUE_AT + 1).is_err());
    assert_eq!(submitted.status, MilestoneStatus::Submitted);
}

#[test]
fn maximum_review_uses_a_capped_claim_grace() {
    let milestone = Milestone::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        CREATED_AT + MAX_MILESTONE_DURATION_SECS,
        MAX_REVIEW_WINDOW_SECS,
        true,
        1,
        CREATED_AT,
    )
    .unwrap();

    assert_eq!(milestone.review_window_secs, MAX_REVIEW_WINDOW_SECS);
    assert_eq!(milestone.claim_grace_secs(), MAX_CLAIM_GRACE_SECS);
}

#[test]
fn approve_and_permissionless_settlement_races_have_only_one_winner() {
    let submitted_at = CREATED_AT + 10;
    let claimable_at = submitted_at + i64::from(MIN_REVIEW_WINDOW_SECS) * 2;

    let mut settlement_first = milestone();
    settlement_first
        .submit(worker(), [6; 32], submitted_at)
        .unwrap();
    settlement_first.settle_after_review(claimable_at).unwrap();
    assert!(settlement_first
        .approve(funder(), claimable_at + 1)
        .is_err());

    let mut approve_first = milestone();
    approve_first
        .submit(worker(), [6; 32], submitted_at)
        .unwrap();
    approve_first.approve(funder(), submitted_at + 1).unwrap();
    assert!(approve_first.settle_after_review(claimable_at).is_err());
}

#[test]
fn revision_deadline_replaces_the_expired_original_deadline() {
    let mut milestone = milestone();
    milestone.submit(worker(), [6; 32], DUE_AT).unwrap();
    milestone
        .request_revision(funder(), [7; 32], DUE_AT + 1)
        .unwrap();
    let revision_due_at = milestone.due_at;

    assert!(revision_due_at > DUE_AT);
    assert!(milestone.refund_expired(DUE_AT + 2).is_err());
    assert!(milestone.refund_expired(revision_due_at).is_err());
    milestone.refund_expired(revision_due_at + 1).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Refunded);
}

#[test]
fn checked_time_overflow_fails_closed() {
    assert!(Milestone::new(
        funder(),
        worker(),
        [4; 32],
        [5; 32],
        AMOUNT,
        i64::MAX,
        MIN_REVIEW_WINDOW_SECS,
        true,
        1,
        i64::MAX - 1,
    )
    .is_err());

    let mut milestone = milestone();
    milestone.status = MilestoneStatus::Submitted;
    milestone.submitted_at = i64::MAX;
    assert!(milestone.review_ends_at().is_err());
    assert!(milestone.claimable_at().is_err());
}
