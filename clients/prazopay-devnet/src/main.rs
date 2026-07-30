use {
    anchor_client::{Client, Cluster, Program},
    anchor_lang::{
        prelude::{system_program, Clock, Pubkey},
        solana_program::sysvar::SysvarId,
    },
    anyhow::{bail, Context, Result},
    prazopay::state::{Milestone, MilestoneStatus},
    serde_json::{json, Value},
    sha2::{Digest, Sha256},
    solana_keypair::{read_keypair_file, Keypair},
    solana_signer::Signer,
    std::{
        env, fs,
        path::{Path, PathBuf},
        rc::Rc,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    },
};

const AMOUNT_LAMPORTS: u64 = 1;
const REVIEW_WINDOW_SECS: u32 = 60;

fn load_keypair(path: &Path) -> Result<Rc<Keypair>> {
    let keypair = read_keypair_file(path)
        .map_err(|error| anyhow::anyhow!("could not read {}: {error}", path.display()))?;
    Ok(Rc::new(keypair))
}

fn program(payer: Rc<Keypair>) -> Result<Program<Rc<Keypair>>> {
    Client::new(Cluster::Devnet, payer)
        .program(prazopay::id())
        .context("could not create PrazoPay devnet client")
}

fn commitment(run_id: &str, label: &str) -> [u8; 32] {
    Sha256::digest(format!("prazopay:{run_id}:{label}").as_bytes()).into()
}

fn milestone_address(funder: &Pubkey, task_id: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[
            prazopay::constants::MILESTONE_SEED,
            funder.as_ref(),
            task_id.as_ref(),
        ],
        &prazopay::id(),
    )
    .0
}

fn chain_time(program: &Program<Rc<Keypair>>) -> Result<i64> {
    let account = program
        .rpc()
        .get_account(&Clock::id())
        .context("could not read devnet Clock sysvar")?;
    let clock: Clock =
        bincode::deserialize(&account.data).context("could not decode devnet Clock sysvar")?;
    Ok(clock.unix_timestamp)
}

fn wait_until(program: &Program<Rc<Keypair>>, target: i64, label: &str) -> Result<()> {
    loop {
        let now = chain_time(program)?;
        if now >= target {
            println!("{label}: chain time reached {now}");
            return Ok(());
        }
        let remaining = target - now;
        println!("{label}: waiting {remaining}s of chain time");
        thread::sleep(Duration::from_secs(remaining.min(5) as u64));
    }
}

fn create(
    funder_program: &Program<Rc<Keypair>>,
    funder: Pubkey,
    worker: Pubkey,
    task_id: [u8; 32],
    terms_hash: [u8; 32],
    due_at: i64,
) -> Result<(Pubkey, String)> {
    let milestone = milestone_address(&funder, &task_id);
    let signature = funder_program
        .request()
        .accounts(prazopay::accounts::CreateMilestone {
            funder,
            worker,
            milestone,
            system_program: system_program::ID,
        })
        .args(prazopay::instruction::CreateMilestone {
            task_id,
            terms_hash,
            amount: AMOUNT_LAMPORTS,
            due_at,
            review_window_secs: REVIEW_WINDOW_SECS,
            silence_acceptance_acknowledged: true,
        })
        .send()
        .context("create_milestone failed")?;
    Ok((milestone, signature.to_string()))
}

fn submit(
    worker_program: &Program<Rc<Keypair>>,
    worker: Pubkey,
    milestone: Pubkey,
    evidence_hash: [u8; 32],
) -> Result<String> {
    worker_program
        .request()
        .accounts(prazopay::accounts::SubmitDelivery { milestone, worker })
        .args(prazopay::instruction::SubmitDelivery { evidence_hash })
        .send()
        .map(|signature| signature.to_string())
        .context("submit_delivery failed")
}

fn request_revision(
    funder_program: &Program<Rc<Keypair>>,
    funder: Pubkey,
    milestone: Pubkey,
    feedback_hash: [u8; 32],
) -> Result<String> {
    funder_program
        .request()
        .accounts(prazopay::accounts::RequestRevision { milestone, funder })
        .args(prazopay::instruction::RequestRevision { feedback_hash })
        .send()
        .map(|signature| signature.to_string())
        .context("request_revision failed")
}

fn approve(
    funder_program: &Program<Rc<Keypair>>,
    funder: Pubkey,
    worker: Pubkey,
    milestone: Pubkey,
) -> Result<String> {
    funder_program
        .request()
        .accounts(prazopay::accounts::ApproveMilestone {
            milestone,
            funder,
            worker,
        })
        .args(prazopay::instruction::ApproveMilestone {})
        .send()
        .map(|signature| signature.to_string())
        .context("approve_milestone failed")
}

fn settle(
    trigger_program: &Program<Rc<Keypair>>,
    worker: Pubkey,
    trigger: Pubkey,
    milestone: Pubkey,
) -> Result<String> {
    trigger_program
        .request()
        .accounts(prazopay::accounts::SettleAfterReview {
            milestone,
            worker,
            trigger,
        })
        .args(prazopay::instruction::SettleAfterReview {})
        .send()
        .map(|signature| signature.to_string())
        .context("settle_after_review failed")
}

fn refund(
    trigger_program: &Program<Rc<Keypair>>,
    funder: Pubkey,
    trigger: Pubkey,
    milestone: Pubkey,
) -> Result<String> {
    trigger_program
        .request()
        .accounts(prazopay::accounts::RefundExpired {
            milestone,
            funder,
            trigger,
        })
        .args(prazopay::instruction::RefundExpired {})
        .send()
        .map(|signature| signature.to_string())
        .context("refund_expired failed")
}

fn status_name(status: MilestoneStatus) -> &'static str {
    match status {
        MilestoneStatus::Open => "OPEN",
        MilestoneStatus::Submitted => "SUBMITTED",
        MilestoneStatus::Paid => "PAID",
        MilestoneStatus::Refunded => "REFUNDED",
    }
}

fn state(program: &Program<Rc<Keypair>>, milestone: Pubkey) -> Result<Milestone> {
    program
        .account(milestone)
        .with_context(|| format!("could not read milestone {milestone}"))
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() == 3 && args[1] == "--clock" {
        let payer = load_keypair(Path::new(&args[2]))?;
        let program = program(payer)?;
        let system_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system time is before Unix epoch")?
            .as_secs();
        println!("CLOCK_SYSVAR={}", chain_time(&program)?);
        println!("SYSTEM_TIME={system_time}");
        return Ok(());
    }
    if args.len() < 4 || args.len() > 5 {
        bail!(
            "usage: prazopay-devnet <funder-keypair> <worker-keypair> \
             <trigger-keypair> [output-json]"
        );
    }

    let funder = load_keypair(Path::new(&args[1]))?;
    let worker = load_keypair(Path::new(&args[2]))?;
    let trigger = load_keypair(Path::new(&args[3]))?;
    let output = args
        .get(4)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/devnet/lifecycle.json"));

    let funder_key = funder.pubkey();
    let worker_key = worker.pubkey();
    let trigger_key = trigger.pubkey();
    if funder_key == worker_key || funder_key == trigger_key || worker_key == trigger_key {
        bail!("funder, worker, and trigger must be distinct test identities");
    }

    let funder_program = program(funder)?;
    let worker_program = program(worker)?;
    let trigger_program = program(trigger)?;
    funder_program
        .rpc()
        .get_account(&prazopay::id())
        .context("PrazoPay is not deployed at the expected devnet Program ID")?;

    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_nanos()
        .to_string();
    let started_at = chain_time(&funder_program)?;
    let wall_started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_secs() as i64;
    let deadline_base = started_at.max(wall_started_at);
    println!(
        "timing: clock_sysvar={started_at} system_time={wall_started_at} \
         deadline_base={deadline_base}"
    );

    let task_refund = commitment(&run_id, "task-refund");
    let task_claim = commitment(&run_id, "task-claim");
    let task_approve = commitment(&run_id, "task-approve");
    let terms_refund = commitment(&run_id, "terms-refund");
    let terms_claim = commitment(&run_id, "terms-claim");
    let terms_approve = commitment(&run_id, "terms-approve");

    let refund_due_at = deadline_base + 100;
    let (refund_milestone, refund_create) = create(
        &funder_program,
        funder_key,
        worker_key,
        task_refund,
        terms_refund,
        refund_due_at,
    )?;
    println!("created expiry-refund milestone {refund_milestone}");

    let (claim_milestone, claim_create) = create(
        &funder_program,
        funder_key,
        worker_key,
        task_claim,
        terms_claim,
        deadline_base + 300,
    )?;
    let claim_submit = submit(
        &worker_program,
        worker_key,
        claim_milestone,
        commitment(&run_id, "evidence-claim"),
    )?;
    let claim_submitted_at = state(&funder_program, claim_milestone)?.submitted_at;
    println!("submitted silent-review milestone {claim_milestone}");

    let (approve_milestone, approve_create) = create(
        &funder_program,
        funder_key,
        worker_key,
        task_approve,
        terms_approve,
        deadline_base + 600,
    )?;
    let approve_submit_one = submit(
        &worker_program,
        worker_key,
        approve_milestone,
        commitment(&run_id, "evidence-approve-v1"),
    )?;
    let approve_revision = request_revision(
        &funder_program,
        funder_key,
        approve_milestone,
        commitment(&run_id, "feedback-approve-v1"),
    )?;
    let approve_submit_two = submit(
        &worker_program,
        worker_key,
        approve_milestone,
        commitment(&run_id, "evidence-approve-v2"),
    )?;
    let worker_before_approve = funder_program.rpc().get_balance(&worker_key)?;
    let approve_signature = approve(&funder_program, funder_key, worker_key, approve_milestone)?;
    let worker_after_approve = funder_program.rpc().get_balance(&worker_key)?;
    if worker_after_approve != worker_before_approve + AMOUNT_LAMPORTS {
        bail!("approval did not transfer exactly one locked lamport to worker");
    }
    let approve_state = state(&funder_program, approve_milestone)?;
    if approve_state.status != MilestoneStatus::Paid || approve_state.revision_attempts() != 1 {
        bail!("revision-and-approval milestone ended in an unexpected state");
    }
    println!("completed revision-and-approval milestone {approve_milestone}");

    let claim_at = claim_submitted_at + i64::from(REVIEW_WINDOW_SECS);
    wait_until(
        &funder_program,
        claim_at.max(refund_due_at + 1),
        "terminal-window",
    )?;

    let claim_escrow_before = funder_program.rpc().get_balance(&claim_milestone)?;
    let claim_signature = settle(&trigger_program, worker_key, trigger_key, claim_milestone)?;
    let claim_escrow_after = funder_program.rpc().get_balance(&claim_milestone)?;
    if claim_escrow_before - claim_escrow_after != AMOUNT_LAMPORTS {
        bail!("permissionless silence settlement did not release exactly one locked lamport");
    }
    let claim_state = state(&funder_program, claim_milestone)?;
    if claim_state.status != MilestoneStatus::Paid {
        bail!("silent-review milestone did not end PAID");
    }
    println!("completed permissionless silence settlement {claim_milestone}");

    let funder_before_refund = funder_program.rpc().get_balance(&funder_key)?;
    let refund_signature = refund(&trigger_program, funder_key, trigger_key, refund_milestone)?;
    let funder_after_refund = funder_program.rpc().get_balance(&funder_key)?;
    if funder_after_refund != funder_before_refund + AMOUNT_LAMPORTS {
        bail!("expiry refund did not return exactly one locked lamport to funder");
    }
    let refund_state = state(&funder_program, refund_milestone)?;
    if refund_state.status != MilestoneStatus::Refunded {
        bail!("expiry milestone did not end REFUNDED");
    }
    println!("completed third-party-triggered refund {refund_milestone}");

    let evidence: Value = json!({
        "schema": "prazopay-devnet-lifecycle-v1",
        "cluster": "devnet",
        "program_id": prazopay::id().to_string(),
        "run_id": run_id,
        "started_at": started_at,
        "wall_started_at": wall_started_at,
        "deadline_base": deadline_base,
        "completed_at": chain_time(&funder_program)?,
        "amount_lamports_per_milestone": AMOUNT_LAMPORTS,
        "review_window_secs": REVIEW_WINDOW_SECS,
        "identities": {
            "funder": funder_key.to_string(),
            "worker": worker_key.to_string(),
            "refund_trigger": trigger_key.to_string(),
        },
        "revision_then_approve": {
            "milestone": approve_milestone.to_string(),
            "status": status_name(approve_state.status),
            "revision_count": approve_state.revision_attempts(),
            "signatures": {
                "create": approve_create,
                "submit_v1": approve_submit_one,
                "request_revision": approve_revision,
                "submit_v2": approve_submit_two,
                "approve": approve_signature,
            }
        },
        "silent_review_settlement": {
            "milestone": claim_milestone.to_string(),
            "status": status_name(claim_state.status),
            "submitted_at": claim_submitted_at,
            "claim_at": claim_at,
            "signatures": {
                "create": claim_create,
                "submit": claim_submit,
                "settle": claim_signature,
            }
        },
        "expiry_refund": {
            "milestone": refund_milestone.to_string(),
            "status": status_name(refund_state.status),
            "due_at": refund_due_at,
            "signatures": {
                "create": refund_create,
                "refund": refund_signature,
            }
        }
    });

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("could not write {}", output.display()))?;
    println!("DEVNET_LIFECYCLE=PASS");
    println!("EVIDENCE={}", output.display());
    Ok(())
}
