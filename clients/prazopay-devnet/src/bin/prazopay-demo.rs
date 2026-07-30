use {
    anchor_client::{Client, Cluster, Program},
    anchor_lang::{
        prelude::{system_program, Clock, Pubkey},
        solana_program::sysvar::SysvarId,
    },
    anyhow::{bail, Context, Result},
    prazopay::state::{Milestone, MilestoneStatus},
    serde::{Deserialize, Serialize},
    sha2::{Digest, Sha256},
    solana_keypair::{read_keypair_file, Keypair},
    solana_signer::Signer,
    std::{
        collections::BTreeMap,
        env, fs,
        path::Path,
        rc::Rc,
        str::FromStr,
        time::{SystemTime, UNIX_EPOCH},
    },
};

const AMOUNT_LAMPORTS: u64 = 1;
const REVIEW_WINDOW_SECS: u32 = 60;
const DEADLINE_OFFSET_SECS: i64 = 1_800;

#[derive(Debug, Deserialize, Serialize)]
struct DemoSession {
    schema: String,
    cluster: String,
    program_id: String,
    run_id: String,
    funder: String,
    worker: String,
    milestone: String,
    amount_lamports: u64,
    due_at: i64,
    review_window_secs: u32,
    task_id_hex: String,
    terms_hash_hex: String,
    evidence_hash_hex: Option<String>,
    signatures: BTreeMap<String, String>,
}

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
    Sha256::digest(format!("prazopay-demo:{run_id}:{label}").as_bytes()).into()
}

fn encode_hex(value: &[u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
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

fn wall_time() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_secs() as i64)
}

fn read_session(path: &Path) -> Result<DemoSession> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("could not read {}", path.display()))?,
    )
    .with_context(|| format!("could not decode {}", path.display()))
}

fn write_session(path: &Path, session: &DemoSession) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(session)?)
        .with_context(|| format!("could not write {}", path.display()))
}

fn parse_pubkey(value: &str, label: &str) -> Result<Pubkey> {
    Pubkey::from_str(value).with_context(|| format!("invalid {label} pubkey in session"))
}

fn state(program: &Program<Rc<Keypair>>, milestone: Pubkey) -> Result<Milestone> {
    program
        .account(milestone)
        .with_context(|| format!("could not read milestone {milestone}"))
}

fn status_name(status: MilestoneStatus) -> &'static str {
    match status {
        MilestoneStatus::Open => "OPEN",
        MilestoneStatus::Submitted => "SUBMITTED",
        MilestoneStatus::Paid => "PAID",
        MilestoneStatus::Refunded => "REFUNDED",
    }
}

fn print_public_result(
    action: &str,
    signer_role: &str,
    milestone: Pubkey,
    status: MilestoneStatus,
    signature: Option<&str>,
) {
    println!("ACTION={action}");
    println!("SIGNER_ROLE={signer_role}");
    println!("MILESTONE={milestone}");
    println!("STATE={}", status_name(status));
    if let Some(signature) = signature {
        println!("TX={signature}");
        println!("TX_EXPLORER=https://explorer.solana.com/tx/{signature}?cluster=devnet");
    }
    println!("ACCOUNT_EXPLORER=https://explorer.solana.com/address/{milestone}?cluster=devnet");
}

fn create(funder_path: &Path, worker_path: &Path, session_path: &Path) -> Result<()> {
    let funder = load_keypair(funder_path)?;
    let worker = load_keypair(worker_path)?;
    let funder_key = funder.pubkey();
    let worker_key = worker.pubkey();
    if funder_key == worker_key {
        bail!("funder and worker must be distinct devnet identities");
    }

    let funder_program = program(funder)?;
    funder_program
        .rpc()
        .get_account(&prazopay::id())
        .context("PrazoPay is not deployed at the expected devnet Program ID")?;

    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_nanos()
        .to_string();
    let task_id = commitment(&run_id, "task");
    let terms_hash = commitment(&run_id, "terms");
    let milestone = milestone_address(&funder_key, &task_id);
    let due_at = chain_time(&funder_program)?
        .max(wall_time()?)
        .checked_add(DEADLINE_OFFSET_SECS)
        .context("deadline overflow")?;

    let signature = funder_program
        .request()
        .accounts(prazopay::accounts::CreateMilestone {
            funder: funder_key,
            worker: worker_key,
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

    let milestone_state = state(&funder_program, milestone)?;
    if milestone_state.status != MilestoneStatus::Open
        || milestone_state.amount != AMOUNT_LAMPORTS
        || milestone_state.funder != funder_key
        || milestone_state.worker != worker_key
    {
        bail!("created milestone state does not match the signed instruction");
    }

    let signature = signature.to_string();
    let mut signatures = BTreeMap::new();
    signatures.insert("create".to_owned(), signature.clone());
    let session = DemoSession {
        schema: "prazopay-live-demo-v1".to_owned(),
        cluster: "devnet".to_owned(),
        program_id: prazopay::id().to_string(),
        run_id,
        funder: funder_key.to_string(),
        worker: worker_key.to_string(),
        milestone: milestone.to_string(),
        amount_lamports: AMOUNT_LAMPORTS,
        due_at,
        review_window_secs: REVIEW_WINDOW_SECS,
        task_id_hex: encode_hex(&task_id),
        terms_hash_hex: encode_hex(&terms_hash),
        evidence_hash_hex: None,
        signatures,
    };
    write_session(session_path, &session)?;
    print_public_result(
        "CREATE",
        "funder",
        milestone,
        milestone_state.status,
        Some(&signature),
    );
    println!("AMOUNT_LAMPORTS={AMOUNT_LAMPORTS}");
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn submit(worker_path: &Path, session_path: &Path) -> Result<()> {
    let worker = load_keypair(worker_path)?;
    let worker_key = worker.pubkey();
    let mut session = read_session(session_path)?;
    if session.cluster != "devnet" || session.program_id != prazopay::id().to_string() {
        bail!("session is not for the expected PrazoPay devnet program");
    }
    if worker_key != parse_pubkey(&session.worker, "worker")? {
        bail!("the supplied keypair is not the immutable worker");
    }

    let milestone = parse_pubkey(&session.milestone, "milestone")?;
    let evidence_hash = commitment(&session.run_id, "delivery");
    let worker_program = program(worker)?;
    let signature = worker_program
        .request()
        .accounts(prazopay::accounts::SubmitDelivery {
            milestone,
            worker: worker_key,
        })
        .args(prazopay::instruction::SubmitDelivery { evidence_hash })
        .send()
        .context("submit_delivery failed")?
        .to_string();

    let milestone_state = state(&worker_program, milestone)?;
    if milestone_state.status != MilestoneStatus::Submitted
        || milestone_state.evidence_hash != evidence_hash
    {
        bail!("submitted milestone state does not match the signed instruction");
    }
    session.evidence_hash_hex = Some(encode_hex(&evidence_hash));
    session
        .signatures
        .insert("submit".to_owned(), signature.clone());
    write_session(session_path, &session)?;
    print_public_result(
        "SUBMIT",
        "worker",
        milestone,
        milestone_state.status,
        Some(&signature),
    );
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn approve(funder_path: &Path, session_path: &Path) -> Result<()> {
    let funder = load_keypair(funder_path)?;
    let funder_key = funder.pubkey();
    let mut session = read_session(session_path)?;
    if session.cluster != "devnet" || session.program_id != prazopay::id().to_string() {
        bail!("session is not for the expected PrazoPay devnet program");
    }
    if funder_key != parse_pubkey(&session.funder, "funder")? {
        bail!("the supplied keypair is not the immutable funder");
    }

    let worker_key = parse_pubkey(&session.worker, "worker")?;
    let milestone = parse_pubkey(&session.milestone, "milestone")?;
    let funder_program = program(funder)?;
    let worker_before = funder_program.rpc().get_balance(&worker_key)?;
    let signature = funder_program
        .request()
        .accounts(prazopay::accounts::ApproveMilestone {
            milestone,
            funder: funder_key,
            worker: worker_key,
        })
        .args(prazopay::instruction::ApproveMilestone {})
        .send()
        .context("approve_milestone failed")?
        .to_string();
    let worker_after = funder_program.rpc().get_balance(&worker_key)?;

    let milestone_state = state(&funder_program, milestone)?;
    if milestone_state.status != MilestoneStatus::Paid {
        bail!("approved milestone did not end PAID");
    }
    if worker_after != worker_before + session.amount_lamports {
        bail!("worker did not receive exactly the locked amount");
    }
    session
        .signatures
        .insert("approve".to_owned(), signature.clone());
    write_session(session_path, &session)?;
    print_public_result(
        "APPROVE",
        "funder",
        milestone,
        milestone_state.status,
        Some(&signature),
    );
    println!(
        "WORKER_GAIN_LAMPORTS={}",
        worker_after.saturating_sub(worker_before)
    );
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn settle(trigger_path: &Path, session_path: &Path) -> Result<()> {
    let trigger = load_keypair(trigger_path)?;
    let trigger_key = trigger.pubkey();
    let mut session = read_session(session_path)?;
    if session.cluster != "devnet" || session.program_id != prazopay::id().to_string() {
        bail!("session is not for the expected PrazoPay devnet program");
    }

    let worker_key = parse_pubkey(&session.worker, "worker")?;
    let milestone = parse_pubkey(&session.milestone, "milestone")?;
    let trigger_program = program(trigger)?;
    let worker_before = trigger_program.rpc().get_balance(&worker_key)?;
    let signature = trigger_program
        .request()
        .accounts(prazopay::accounts::SettleAfterReview {
            milestone,
            worker: worker_key,
            trigger: trigger_key,
        })
        .args(prazopay::instruction::SettleAfterReview {})
        .send()
        .context("settle_after_review failed")?
        .to_string();
    let worker_after = trigger_program.rpc().get_balance(&worker_key)?;

    let milestone_state = state(&trigger_program, milestone)?;
    if milestone_state.status != MilestoneStatus::Paid {
        bail!("permissionless silence settlement did not end PAID");
    }
    if worker_after != worker_before + session.amount_lamports {
        bail!("immutable worker did not receive exactly the locked amount");
    }
    session
        .signatures
        .insert("settle".to_owned(), signature.clone());
    write_session(session_path, &session)?;
    print_public_result(
        "SETTLE_AFTER_SILENCE",
        "permissionless-trigger",
        milestone,
        milestone_state.status,
        Some(&signature),
    );
    println!(
        "WORKER_GAIN_LAMPORTS={}",
        worker_after.saturating_sub(worker_before)
    );
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn refund(trigger_path: &Path, session_path: &Path) -> Result<()> {
    let trigger = load_keypair(trigger_path)?;
    let trigger_key = trigger.pubkey();
    let mut session = read_session(session_path)?;
    if session.cluster != "devnet" || session.program_id != prazopay::id().to_string() {
        bail!("session is not for the expected PrazoPay devnet program");
    }

    let funder_key = parse_pubkey(&session.funder, "funder")?;
    let milestone = parse_pubkey(&session.milestone, "milestone")?;
    let trigger_program = program(trigger)?;
    let funder_before = trigger_program.rpc().get_balance(&funder_key)?;
    let signature = trigger_program
        .request()
        .accounts(prazopay::accounts::RefundExpired {
            milestone,
            funder: funder_key,
            trigger: trigger_key,
        })
        .args(prazopay::instruction::RefundExpired {})
        .send()
        .context("refund_expired failed")?
        .to_string();
    let funder_after = trigger_program.rpc().get_balance(&funder_key)?;

    let milestone_state = state(&trigger_program, milestone)?;
    if milestone_state.status != MilestoneStatus::Refunded {
        bail!("expired milestone did not end REFUNDED");
    }
    if funder_after != funder_before + session.amount_lamports {
        bail!("funder did not receive exactly the locked amount");
    }
    session
        .signatures
        .insert("refund".to_owned(), signature.clone());
    write_session(session_path, &session)?;
    print_public_result(
        "REFUND",
        "permissionless-trigger",
        milestone,
        milestone_state.status,
        Some(&signature),
    );
    println!(
        "FUNDER_GAIN_LAMPORTS={}",
        funder_after.saturating_sub(funder_before)
    );
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn inspect(payer_path: &Path, session_path: &Path) -> Result<()> {
    let payer = load_keypair(payer_path)?;
    let session = read_session(session_path)?;
    if session.cluster != "devnet" || session.program_id != prazopay::id().to_string() {
        bail!("session is not for the expected PrazoPay devnet program");
    }
    let milestone = parse_pubkey(&session.milestone, "milestone")?;
    let payer_program = program(payer)?;
    let milestone_state = state(&payer_program, milestone)?;
    let now = chain_time(&payer_program)?;
    print_public_result(
        "STATUS",
        "read-only",
        milestone,
        milestone_state.status,
        None,
    );
    println!("AMOUNT_LAMPORTS={}", milestone_state.amount);
    println!("FUNDER={}", milestone_state.funder);
    println!("WORKER={}", milestone_state.worker);
    println!("CHAIN_TIME={now}");
    println!("DUE_AT={}", milestone_state.due_at);
    println!("STATE_TIME={}", milestone_state.submitted_at);
    println!(
        "PROTOCOL_VERSION={}",
        if milestone_state.is_protocol_v1() {
            "v1"
        } else {
            "v0"
        }
    );
    println!("REVISION_COUNT={}", milestone_state.revision_attempts());
    println!("CLAIM_GRACE_SECS={}", milestone_state.claim_grace_secs());
    Ok(())
}

fn usage() -> &'static str {
    "usage:\n  \
     prazopay-demo create <funder-keypair> <worker-keypair> <session-json>\n  \
     prazopay-demo submit <worker-keypair> <session-json>\n  \
     prazopay-demo approve <funder-keypair> <session-json>\n  \
     prazopay-demo settle <trigger-keypair> <session-json>\n  \
     prazopay-demo refund <trigger-keypair> <session-json>\n  \
     prazopay-demo status <payer-keypair> <session-json>"
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("create") if args.len() == 5 => create(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        ),
        Some("submit") if args.len() == 4 => submit(Path::new(&args[2]), Path::new(&args[3])),
        Some("approve") if args.len() == 4 => approve(Path::new(&args[2]), Path::new(&args[3])),
        Some("settle") if args.len() == 4 => settle(Path::new(&args[2]), Path::new(&args[3])),
        Some("refund") if args.len() == 4 => refund(Path::new(&args[2]), Path::new(&args[3])),
        Some("status") if args.len() == 4 => inspect(Path::new(&args[2]), Path::new(&args[3])),
        _ => bail!(usage()),
    }
}
