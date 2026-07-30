use anchor_lang::prelude::*;

#[constant]
pub const MILESTONE_SEED: &[u8] = b"milestone";

#[constant]
pub const MIN_REVIEW_WINDOW_SECS: u32 = 60;

#[constant]
pub const MAX_REVIEW_WINDOW_SECS: u32 = 7 * 24 * 60 * 60;

#[constant]
pub const MAX_MILESTONE_DURATION_SECS: i64 = 90 * 24 * 60 * 60;

#[constant]
pub const MAX_REVISIONS: u8 = 3;

#[constant]
pub const MAX_CLAIM_GRACE_SECS: u32 = 60 * 60;

// Versioning is packed into the otherwise-unused high bit of revision_count so
// upgraded programs can continue to deserialize and settle legacy accounts.
pub const PROTOCOL_V1_FLAG: u8 = 0b1000_0000;
pub const REVISION_COUNT_MASK: u8 = 0b0111_1111;

pub const ZERO_HASH: [u8; 32] = [0; 32];
