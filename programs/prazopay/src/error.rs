use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The milestone amount must be greater than zero")]
    InvalidAmount,
    #[msg("The deadline must be in the future and within the supported horizon")]
    InvalidDeadline,
    #[msg("The review window is outside the supported range")]
    InvalidReviewWindow,
    #[msg("The funder must explicitly acknowledge silence-based acceptance")]
    SilenceAcceptanceNotAcknowledged,
    #[msg("The funder and worker must be different accounts")]
    PartiesMustDiffer,
    #[msg("A required commitment hash is all zeroes")]
    EmptyCommitment,
    #[msg("Only the immutable funder may perform this action")]
    UnauthorizedFunder,
    #[msg("Only the immutable worker may perform this action")]
    UnauthorizedWorker,
    #[msg("The milestone is not in the required state")]
    InvalidStatus,
    #[msg("The delivery deadline has passed")]
    SubmissionDeadlinePassed,
    #[msg("The review window has already elapsed")]
    ReviewWindowElapsed,
    #[msg("The review window is still open")]
    ReviewWindowStillOpen,
    #[msg("The post-review claim grace period is still open")]
    ClaimGraceStillOpen,
    #[msg("The milestone deadline has not passed")]
    DeadlineStillOpen,
    #[msg("The maximum number of revisions has been reached")]
    MaxRevisionsReached,
    #[msg("The delivery window is outside the supported range")]
    InvalidDeliveryWindow,
    #[msg("The agreement proposal lifetime is outside the supported range")]
    InvalidProposalLifetime,
    #[msg("The agreement proposal has expired")]
    AgreementExpired,
    #[msg("The funding window is outside the supported range")]
    InvalidFundingWindow,
    #[msg("The accepted agreement funding window has expired")]
    FundingWindowExpired,
    #[msg("The funded milestone address is invalid")]
    InvalidMilestoneAddress,
    #[msg("The agreement is not in the required state")]
    InvalidAgreementStatus,
    #[msg("Checked time arithmetic overflowed")]
    TimeOverflow,
}
