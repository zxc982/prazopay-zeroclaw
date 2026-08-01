use {
    anchor_client::{Client, Cluster, Program},
    anchor_lang::{
        prelude::{system_program, Clock, Pubkey},
        solana_program::sysvar::SysvarId,
    },
    anyhow::{bail, Context, Result},
    prazopay::state::{Agreement, AgreementStatus, Milestone, MilestoneStatus},
    serde::{Deserialize, Serialize},
    serde_json::Value,
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

const TERMS_SCHEMA: &str = "prazopay.agreement-terms.v1";
// Legacy v1 demo defaults are retained only for backward-compatible replay.
// New v2 runs must use a canonical terms document through `propose`.
const AMOUNT_LAMPORTS: u64 = 1;
const REVIEW_WINDOW_SECS: u32 = 60;
const DEADLINE_OFFSET_SECS: i64 = 1_800;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TermsDocument {
    schema: String,
    funder: String,
    worker: String,
    amount_lamports: u64,
    delivery_window_secs: u32,
    review_window_secs: u32,
    revision_delivery_window_secs: u32,
    funding_window_secs: u32,
    proposal_lifetime_secs: u32,
    silence_acceptance: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct DemoSession {
    schema: String,
    cluster: String,
    program_id: String,
    run_id: String,
    funder: String,
    worker: String,
    #[serde(default)]
    agreement: Option<String>,
    milestone: String,
    amount_lamports: u64,
    due_at: i64,
    #[serde(default)]
    delivery_window_secs: Option<u32>,
    #[serde(default)]
    funding_window_secs: Option<u32>,
    #[serde(default)]
    revision_delivery_window_secs: Option<u32>,
    #[serde(default)]
    proposal_expires_at: Option<i64>,
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

fn decode_hex_32(value: &str, label: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} must contain exactly 64 hexadecimal characters");
    }
    let mut decoded = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        decoded[index] = u8::from_str_radix(
            std::str::from_utf8(chunk).context("hex input is not UTF-8")?,
            16,
        )?;
    }
    Ok(decoded)
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

fn agreement_address(funder: &Pubkey, task_id: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[
            prazopay::constants::AGREEMENT_SEED,
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

fn validate_v2_session(session: &DemoSession) -> Result<()> {
    if session.schema != "prazopay-live-demo-v2"
        || session.cluster != "devnet"
        || session.program_id != prazopay::id().to_string()
    {
        bail!("session is not a PrazoPay v2 devnet session for the compiled Program ID");
    }
    let funder = parse_pubkey(&session.funder, "funder")?;
    let task_id = decode_hex_32(&session.task_id_hex, "session task_id_hex")?;
    let expected_agreement = agreement_address(&funder, &task_id);
    let agreement = parse_pubkey(
        session
            .agreement
            .as_deref()
            .context("session has no v2 agreement")?,
        "agreement",
    )?;
    let milestone = parse_pubkey(&session.milestone, "milestone")?;
    if agreement != expected_agreement || milestone != milestone_address(&funder, &task_id) {
        bail!("session Agreement or Milestone PDA is not derived from its Funder and task ID");
    }
    Ok(())
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

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>> {
    fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<()> {
        match value {
            Value::Null => output.extend_from_slice(b"null"),
            Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
            Value::Number(value) => {
                if value.is_f64() {
                    bail!("canonical terms must not contain floating-point numbers");
                }
                output.extend_from_slice(value.to_string().as_bytes());
            }
            Value::String(value) => serde_json::to_writer(output, value)?,
            Value::Array(values) => {
                output.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    write_value(value, output)?;
                }
                output.push(b']');
            }
            Value::Object(values) => {
                output.push(b'{');
                let mut keys: Vec<_> = values.keys().collect();
                keys.sort_unstable();
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    serde_json::to_writer(&mut *output, key)?;
                    output.push(b':');
                    write_value(&values[key], output)?;
                }
                output.push(b'}');
            }
        }
        Ok(())
    }

    let mut output = Vec::new();
    write_value(value, &mut output)?;
    Ok(output)
}

fn load_terms(path: &Path) -> Result<(TermsDocument, [u8; 32])> {
    let raw = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    let document: TermsDocument = serde_json::from_slice(&raw).with_context(|| {
        format!(
            "{} is not an exact agreement-terms document",
            path.display()
        )
    })?;
    let value: Value = serde_json::from_slice(&raw)
        .with_context(|| format!("could not decode {} as JSON", path.display()))?;
    if !value.is_object() {
        bail!("canonical terms must be a JSON object");
    }
    if document.schema != TERMS_SCHEMA {
        bail!("terms schema must be {TERMS_SCHEMA}");
    }
    if document.revision_delivery_window_secs != document.review_window_secs {
        bail!("revision_delivery_window_secs must equal review_window_secs for protocol v2");
    }
    if !document.silence_acceptance {
        bail!("terms must explicitly set silence_acceptance to true");
    }
    let canonical = canonical_json_bytes(&value)?;
    Ok((document, Sha256::digest(canonical).into()))
}

fn verify_terms_parties(document: &TermsDocument, funder: Pubkey, worker: Pubkey) -> Result<()> {
    if parse_pubkey(&document.funder, "terms funder")? != funder {
        bail!("terms funder does not match the signing Funder");
    }
    if parse_pubkey(&document.worker, "terms worker")? != worker {
        bail!("terms worker does not match the proposed Worker");
    }
    if funder == worker {
        bail!("funder and worker must be distinct devnet identities");
    }
    Ok(())
}

fn verify_agreement_matches_session_and_terms(
    agreement: &Agreement,
    agreement_key: Pubkey,
    session: &DemoSession,
    document: &TermsDocument,
    terms_hash: [u8; 32],
) -> Result<()> {
    validate_v2_session(session)?;
    let funder = parse_pubkey(&session.funder, "funder")?;
    let worker = parse_pubkey(&session.worker, "worker")?;
    verify_terms_parties(document, funder, worker)?;
    let task_id = decode_hex_32(&session.task_id_hex, "session task_id_hex")?;
    if agreement_key != agreement_address(&funder, &task_id)
        || agreement.funder != funder
        || agreement.worker != worker
        || agreement.task_id != task_id
        || agreement.terms_hash != terms_hash
        || session.terms_hash_hex != encode_hex(&terms_hash)
        || session.amount_lamports != document.amount_lamports
        || session.delivery_window_secs != Some(document.delivery_window_secs)
        || session.review_window_secs != document.review_window_secs
        || session.revision_delivery_window_secs != Some(document.revision_delivery_window_secs)
        || session.funding_window_secs != Some(document.funding_window_secs)
        || agreement.amount != document.amount_lamports
        || agreement.delivery_window_secs != document.delivery_window_secs
        || agreement.review_window_secs != document.review_window_secs
        || agreement.funding_window_secs != document.funding_window_secs
        || agreement.proposal_expires_at != session.proposal_expires_at.unwrap_or_default()
        || agreement
            .proposal_expires_at
            .saturating_sub(agreement.proposed_at)
            != i64::from(document.proposal_lifetime_secs)
        || agreement.silence_acceptance != document.silence_acceptance
    {
        bail!("chain Agreement, session, and canonical terms do not match exactly");
    }
    Ok(())
}

fn state(program: &Program<Rc<Keypair>>, milestone: Pubkey) -> Result<Milestone> {
    program
        .account(milestone)
        .with_context(|| format!("could not read milestone {milestone}"))
}

fn agreement_state(program: &Program<Rc<Keypair>>, agreement: Pubkey) -> Result<Agreement> {
    program
        .account(agreement)
        .with_context(|| format!("could not read agreement {agreement}"))
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

fn propose(
    funder_path: &Path,
    worker_pubkey: &str,
    terms_path: &Path,
    session_path: &Path,
) -> Result<()> {
    let funder = load_keypair(funder_path)?;
    let funder_key = funder.pubkey();
    let worker_key = Pubkey::from_str(worker_pubkey).context("worker pubkey is invalid")?;
    let (terms, terms_hash) = load_terms(terms_path)?;
    verify_terms_parties(&terms, funder_key, worker_key)?;

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
    let agreement = agreement_address(&funder_key, &task_id);
    let milestone = milestone_address(&funder_key, &task_id);
    let signature = funder_program
        .request()
        .accounts(prazopay::accounts::ProposeAgreement {
            funder: funder_key,
            worker: worker_key,
            agreement,
            system_program: system_program::ID,
        })
        .args(prazopay::instruction::ProposeAgreement {
            task_id,
            terms_hash,
            amount: terms.amount_lamports,
            delivery_window_secs: terms.delivery_window_secs,
            review_window_secs: terms.review_window_secs,
            funding_window_secs: terms.funding_window_secs,
            proposal_lifetime_secs: terms.proposal_lifetime_secs,
            silence_acceptance: terms.silence_acceptance,
        })
        .send()
        .context("propose_agreement failed")?
        .to_string();

    let agreement_state = agreement_state(&funder_program, agreement)?;
    if agreement_state.status != AgreementStatus::Proposed
        || agreement_state.funder != funder_key
        || agreement_state.worker != worker_key
        || agreement_state.terms_hash != terms_hash
        || agreement_state.amount != terms.amount_lamports
        || agreement_state.delivery_window_secs != terms.delivery_window_secs
        || agreement_state.review_window_secs != terms.review_window_secs
        || agreement_state.funding_window_secs != terms.funding_window_secs
    {
        bail!("proposed agreement state does not match the signed instruction");
    }

    let mut signatures = BTreeMap::new();
    signatures.insert("propose".to_owned(), signature.clone());
    let session = DemoSession {
        schema: "prazopay-live-demo-v2".to_owned(),
        cluster: "devnet".to_owned(),
        program_id: prazopay::id().to_string(),
        run_id,
        funder: funder_key.to_string(),
        worker: worker_key.to_string(),
        agreement: Some(agreement.to_string()),
        milestone: milestone.to_string(),
        amount_lamports: terms.amount_lamports,
        due_at: 0,
        delivery_window_secs: Some(terms.delivery_window_secs),
        funding_window_secs: Some(terms.funding_window_secs),
        revision_delivery_window_secs: Some(terms.revision_delivery_window_secs),
        proposal_expires_at: Some(agreement_state.proposal_expires_at),
        review_window_secs: terms.review_window_secs,
        task_id_hex: encode_hex(&task_id),
        terms_hash_hex: encode_hex(&terms_hash),
        evidence_hash_hex: None,
        signatures,
    };
    write_session(session_path, &session)?;

    println!("ACTION=PROPOSE_AGREEMENT");
    println!("SIGNER_ROLE=funder");
    println!("AGREEMENT={agreement}");
    println!("STATE=PROPOSED");
    println!("FUNDS_LOCKED=false");
    println!("TERMS_SHA256={}", encode_hex(&terms_hash));
    println!("FUNDING_WINDOW_SECS={}", terms.funding_window_secs);
    println!(
        "REVISION_DELIVERY_WINDOW_SECS={}",
        terms.revision_delivery_window_secs
    );
    println!("TX={signature}");
    println!("TX_EXPLORER=https://explorer.solana.com/tx/{signature}?cluster=devnet");
    println!("ACCOUNT_EXPLORER=https://explorer.solana.com/address/{agreement}?cluster=devnet");
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn accept(worker_path: &Path, terms_path: &Path, session_path: &Path) -> Result<()> {
    let worker = load_keypair(worker_path)?;
    let worker_key = worker.pubkey();
    let mut session = read_session(session_path)?;
    validate_v2_session(&session)?;
    if worker_key != parse_pubkey(&session.worker, "worker")? {
        bail!("the supplied keypair is not the proposed worker");
    }
    let agreement = parse_pubkey(
        session
            .agreement
            .as_deref()
            .context("session has no v2 agreement")?,
        "agreement",
    )?;
    let worker_program = program(worker)?;
    let (terms, terms_hash) = load_terms(terms_path)?;
    let before = agreement_state(&worker_program, agreement)?;
    verify_agreement_matches_session_and_terms(&before, agreement, &session, &terms, terms_hash)?;
    if before.status != AgreementStatus::Proposed || before.accepted_at != 0 {
        bail!("agreement is not an unaccepted proposal");
    }
    let signature = worker_program
        .request()
        .accounts(prazopay::accounts::AcceptAgreement {
            agreement,
            worker: worker_key,
        })
        .args(prazopay::instruction::AcceptAgreement {})
        .send()
        .context("accept_agreement failed")?
        .to_string();
    let accepted = agreement_state(&worker_program, agreement)?;
    if accepted.status != AgreementStatus::Accepted || accepted.accepted_at <= 0 {
        bail!("agreement did not enter ACCEPTED");
    }
    session
        .signatures
        .insert("accept".to_owned(), signature.clone());
    write_session(session_path, &session)?;

    println!("ACTION=ACCEPT_AGREEMENT");
    println!("SIGNER_ROLE=worker");
    println!("AGREEMENT={agreement}");
    println!("STATE=ACCEPTED");
    println!("TERMS_SHA256={}", session.terms_hash_hex);
    println!("FUNDING_EXPIRES_AT={}", accepted.funding_expires_at()?);
    println!("TX={signature}");
    println!("TX_EXPLORER=https://explorer.solana.com/tx/{signature}?cluster=devnet");
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn reject(worker_path: &Path, session_path: &Path) -> Result<()> {
    let worker = load_keypair(worker_path)?;
    let worker_key = worker.pubkey();
    let mut session = read_session(session_path)?;
    validate_v2_session(&session)?;
    if worker_key != parse_pubkey(&session.worker, "worker")? {
        bail!("the supplied keypair is not the proposed worker");
    }
    let agreement = parse_pubkey(
        session
            .agreement
            .as_deref()
            .context("session has no v2 agreement")?,
        "agreement",
    )?;
    let worker_program = program(worker)?;
    let signature = worker_program
        .request()
        .accounts(prazopay::accounts::RejectAgreement {
            agreement,
            worker: worker_key,
        })
        .args(prazopay::instruction::RejectAgreement {})
        .send()
        .context("reject_agreement failed")?
        .to_string();
    if agreement_state(&worker_program, agreement)?.status != AgreementStatus::Rejected {
        bail!("agreement did not enter REJECTED");
    }
    session
        .signatures
        .insert("reject".to_owned(), signature.clone());
    write_session(session_path, &session)?;

    println!("ACTION=REJECT_AGREEMENT");
    println!("SIGNER_ROLE=worker");
    println!("AGREEMENT={agreement}");
    println!("STATE=REJECTED");
    println!("FUNDS_LOCKED=false");
    println!("TX={signature}");
    println!("TX_EXPLORER=https://explorer.solana.com/tx/{signature}?cluster=devnet");
    println!("SESSION={}", session_path.display());
    Ok(())
}

fn fund(funder_path: &Path, session_path: &Path) -> Result<()> {
    let funder = load_keypair(funder_path)?;
    let funder_key = funder.pubkey();
    let mut session = read_session(session_path)?;
    validate_v2_session(&session)?;
    if funder_key != parse_pubkey(&session.funder, "funder")? {
        bail!("the supplied keypair is not the agreement funder");
    }
    let worker_key = parse_pubkey(&session.worker, "worker")?;
    let agreement = parse_pubkey(
        session
            .agreement
            .as_deref()
            .context("session has no v2 agreement")?,
        "agreement",
    )?;
    let milestone = parse_pubkey(&session.milestone, "milestone")?;
    let funder_program = program(funder)?;
    let agreement_before = agreement_state(&funder_program, agreement)?;
    let chain_now = chain_time(&funder_program)?;
    if agreement_before.status != AgreementStatus::Accepted
        || agreement_before.funder != funder_key
        || agreement_before.worker != worker_key
        || agreement_before.milestone != Pubkey::default()
        || agreement_before.amount != session.amount_lamports
        || agreement_before.delivery_window_secs != session.delivery_window_secs.unwrap_or_default()
        || agreement_before.review_window_secs != session.review_window_secs
        || agreement_before.funding_window_secs != session.funding_window_secs.unwrap_or_default()
        || agreement_before.proposal_expires_at != session.proposal_expires_at.unwrap_or_default()
        || agreement_before.terms_hash
            != decode_hex_32(&session.terms_hash_hex, "session terms_hash_hex")?
        || chain_now > agreement_before.funding_expires_at()?
    {
        bail!("accepted Agreement does not match the Funder session");
    }
    let funded_at = chain_now;
    let signature = funder_program
        .request()
        .accounts(prazopay::accounts::FundAcceptedAgreement {
            agreement,
            funder: funder_key,
            worker: worker_key,
            milestone,
            system_program: system_program::ID,
        })
        .args(prazopay::instruction::FundAcceptedAgreement {})
        .send()
        .context("fund_accepted_agreement failed")?
        .to_string();

    let agreement_after = agreement_state(&funder_program, agreement)?;
    if agreement_after.status != AgreementStatus::Funded || agreement_after.milestone != milestone {
        bail!("agreement did not enter FUNDED");
    }
    let milestone_state = state(&funder_program, milestone)?;
    if milestone_state.status != MilestoneStatus::Open
        || milestone_state.protocol_version() != 2
        || milestone_state.amount != session.amount_lamports
        || milestone_state.funder != funder_key
        || milestone_state.worker != worker_key
        || milestone_state.task_id != decode_hex_32(&session.task_id_hex, "session task_id_hex")?
        || milestone_state.terms_hash
            != decode_hex_32(&session.terms_hash_hex, "session terms_hash_hex")?
        || milestone_state.review_window_secs != session.review_window_secs
    {
        bail!("funded v2 milestone does not match the accepted agreement");
    }
    let delivery_window_secs = i64::from(
        session
            .delivery_window_secs
            .context("session has no v2 delivery window")?,
    );
    if milestone_state.due_at < funded_at
        || milestone_state.due_at.saturating_sub(funded_at) > delivery_window_secs + 30
    {
        bail!("v2 milestone did not receive the agreed delivery window");
    }

    session.due_at = milestone_state.due_at;
    session
        .signatures
        .insert("fund".to_owned(), signature.clone());
    write_session(session_path, &session)?;
    print_public_result(
        "FUND_ACCEPTED_AGREEMENT",
        "funder",
        milestone,
        milestone_state.status,
        Some(&signature),
    );
    println!("AGREEMENT={agreement}");
    println!("PROTOCOL_VERSION=v2");
    println!("DUE_AT={}", milestone_state.due_at);
    println!("SESSION={}", session_path.display());
    Ok(())
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
        agreement: None,
        milestone: milestone.to_string(),
        amount_lamports: AMOUNT_LAMPORTS,
        due_at,
        delivery_window_secs: None,
        funding_window_secs: None,
        revision_delivery_window_secs: None,
        proposal_expires_at: None,
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
        match milestone_state.protocol_version() {
            2 => "v2",
            1 => "v1",
            _ => "v0",
        }
    );
    println!("REVISION_COUNT={}", milestone_state.revision_attempts());
    println!("CLAIM_GRACE_SECS={}", milestone_state.claim_grace_secs());
    Ok(())
}

fn usage() -> &'static str {
    "usage:\n  \
     prazopay-demo propose <funder-keypair> <worker-pubkey> <terms-json> <session-json>\n  \
     prazopay-demo accept <worker-keypair> <terms-json> <session-json>\n  \
     prazopay-demo reject <worker-keypair> <session-json>\n  \
     prazopay-demo fund <funder-keypair> <session-json>\n  \
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
        Some("propose") if args.len() == 6 => propose(
            Path::new(&args[2]),
            &args[3],
            Path::new(&args[4]),
            Path::new(&args[5]),
        ),
        Some("accept") if args.len() == 5 => accept(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        ),
        Some("reject") if args.len() == 4 => reject(Path::new(&args[2]), Path::new(&args[3])),
        Some("fund") if args.len() == 4 => fund(Path::new(&args[2]), Path::new(&args[3])),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn terms(funder: Pubkey, worker: Pubkey) -> TermsDocument {
        TermsDocument {
            schema: TERMS_SCHEMA.to_owned(),
            funder: funder.to_string(),
            worker: worker.to_string(),
            amount_lamports: 1_000,
            delivery_window_secs: 3_600,
            review_window_secs: 600,
            revision_delivery_window_secs: 600,
            funding_window_secs: 900,
            proposal_lifetime_secs: 1_800,
            silence_acceptance: true,
        }
    }

    fn v2_fixture() -> (TermsDocument, Agreement, Pubkey, DemoSession, [u8; 32]) {
        let funder = Pubkey::new_unique();
        let worker = Pubkey::new_unique();
        let task_id = [9; 32];
        let terms_hash = [7; 32];
        let agreement_key = agreement_address(&funder, &task_id);
        let milestone = milestone_address(&funder, &task_id);
        let document = terms(funder, worker);
        let agreement = Agreement {
            funder,
            worker,
            task_id,
            terms_hash,
            amount: document.amount_lamports,
            delivery_window_secs: document.delivery_window_secs,
            review_window_secs: document.review_window_secs,
            funding_window_secs: document.funding_window_secs,
            proposed_at: 100,
            proposal_expires_at: 100 + i64::from(document.proposal_lifetime_secs),
            accepted_at: 0,
            milestone: Pubkey::default(),
            silence_acceptance: true,
            status: AgreementStatus::Proposed,
            bump: 1,
        };
        let session = DemoSession {
            schema: "prazopay-live-demo-v2".to_owned(),
            cluster: "devnet".to_owned(),
            program_id: prazopay::id().to_string(),
            run_id: "test".to_owned(),
            funder: funder.to_string(),
            worker: worker.to_string(),
            agreement: Some(agreement_key.to_string()),
            milestone: milestone.to_string(),
            amount_lamports: document.amount_lamports,
            due_at: 0,
            delivery_window_secs: Some(document.delivery_window_secs),
            funding_window_secs: Some(document.funding_window_secs),
            revision_delivery_window_secs: Some(document.revision_delivery_window_secs),
            proposal_expires_at: Some(agreement.proposal_expires_at),
            review_window_secs: document.review_window_secs,
            task_id_hex: encode_hex(&task_id),
            terms_hash_hex: encode_hex(&terms_hash),
            evidence_hash_hex: None,
            signatures: BTreeMap::new(),
        };
        (document, agreement, agreement_key, session, terms_hash)
    }

    #[test]
    fn canonical_terms_hash_is_independent_of_object_key_order() {
        let left = json!({"schema":"x","amount":1,"nested":{"b":2,"a":1}});
        let right = json!({"nested":{"a":1,"b":2},"amount":1,"schema":"x"});
        assert_eq!(
            canonical_json_bytes(&left).unwrap(),
            canonical_json_bytes(&right).unwrap()
        );
        assert!(canonical_json_bytes(&json!({"amount": 1.5})).is_err());
    }

    #[test]
    fn acceptance_preflight_fails_closed_on_session_or_terms_tampering() {
        let (document, agreement, agreement_key, mut session, terms_hash) = v2_fixture();
        verify_agreement_matches_session_and_terms(
            &agreement,
            agreement_key,
            &session,
            &document,
            terms_hash,
        )
        .unwrap();

        session.cluster = "mainnet-beta".to_owned();
        assert!(verify_agreement_matches_session_and_terms(
            &agreement,
            agreement_key,
            &session,
            &document,
            terms_hash,
        )
        .is_err());
        session.cluster = "devnet".to_owned();
        session.terms_hash_hex = encode_hex(&[8; 32]);
        assert!(verify_agreement_matches_session_and_terms(
            &agreement,
            agreement_key,
            &session,
            &document,
            terms_hash,
        )
        .is_err());
    }

    #[test]
    fn exact_terms_parser_rejects_unknown_or_duplicate_fields() {
        let (document, _, _, _, _) = v2_fixture();
        let base = format!(
            r#"{{"schema":"{}","funder":"{}","worker":"{}","amount_lamports":{},"delivery_window_secs":{},"review_window_secs":{},"revision_delivery_window_secs":{},"funding_window_secs":{},"proposal_lifetime_secs":{},"silence_acceptance":true}}"#,
            document.schema,
            document.funder,
            document.worker,
            document.amount_lamports,
            document.delivery_window_secs,
            document.review_window_secs,
            document.revision_delivery_window_secs,
            document.funding_window_secs,
            document.proposal_lifetime_secs,
        );
        let path = std::env::temp_dir().join(format!(
            "prazopay-terms-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, &base).unwrap();
        assert!(load_terms(&path).is_ok());

        fs::write(&path, base.replacen("}", ",\"unexpected\":1}", 1)).unwrap();
        assert!(load_terms(&path).is_err());
        fs::write(
            &path,
            base.replacen(
                "\"amount_lamports\":1000",
                "\"amount_lamports\":1000,\"amount_lamports\":1001",
                1,
            ),
        )
        .unwrap();
        assert!(load_terms(&path).is_err());
        let _ = fs::remove_file(path);
    }
}
