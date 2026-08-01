#!/usr/bin/env python3
"""Independently verify the public PrazoPay v2 deployment and lifecycle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import struct
from pathlib import Path
from typing import Any

from verify_devnet_live import (
    DEFAULT_RPC_URL,
    PROGRAM_ID,
    UPGRADEABLE_LOADER_ID,
    RpcClient,
    VerificationError,
    account_keys,
    base58_encode,
    decode_milestone_account,
    decode_program_account,
    decode_programdata_account,
    display_rpc_url,
    require,
)


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE = ROOT / "fixtures" / "devnet-v2-lifecycle.json"
AGREEMENT_ACCOUNT_LEN = 215


def decode_agreement_account(data: bytes) -> dict[str, Any]:
    require(
        len(data) == AGREEMENT_ACCOUNT_LEN,
        f"Agreement account length is {len(data)}, expected {AGREEMENT_ACCOUNT_LEN}",
    )
    discriminator = hashlib.sha256(b"account:Agreement").digest()[:8]
    require(data[:8] == discriminator, "Agreement discriminator mismatch")
    offset = 8

    def take(length: int) -> bytes:
        nonlocal offset
        result = data[offset : offset + length]
        require(len(result) == length, "Agreement account is truncated")
        offset += length
        return result

    funder = base58_encode(take(32))
    worker = base58_encode(take(32))
    task_id = take(32).hex()
    terms_hash = take(32).hex()
    amount_lamports = struct.unpack("<Q", take(8))[0]
    delivery_window_secs = struct.unpack("<I", take(4))[0]
    review_window_secs = struct.unpack("<I", take(4))[0]
    funding_window_secs = struct.unpack("<I", take(4))[0]
    proposed_at = struct.unpack("<q", take(8))[0]
    proposal_expires_at = struct.unpack("<q", take(8))[0]
    accepted_at = struct.unpack("<q", take(8))[0]
    milestone_bytes = take(32)
    silence_acceptance = take(1)[0]
    status_value = take(1)[0]
    bump = take(1)[0]
    require(offset == AGREEMENT_ACCOUNT_LEN, "Agreement account has trailing data")
    require(silence_acceptance in (0, 1), "invalid silence-acceptance flag")
    statuses = {0: "proposed", 1: "accepted", 2: "funded", 3: "rejected"}
    require(status_value in statuses, f"invalid Agreement status {status_value}")
    return {
        "funder": funder,
        "worker": worker,
        "task_id_sha256": task_id,
        "terms_sha256": terms_hash,
        "amount_lamports": amount_lamports,
        "delivery_window_secs": delivery_window_secs,
        "review_window_secs": review_window_secs,
        "funding_window_secs": funding_window_secs,
        "proposed_at": proposed_at,
        "proposal_expires_at": proposal_expires_at,
        "accepted_at": accepted_at,
        "milestone": None if not any(milestone_bytes) else base58_encode(milestone_bytes),
        "silence_acceptance": bool(silence_acceptance),
        "status": statuses[status_value],
        "bump": bump,
    }


def signer_keys(transaction: dict[str, Any]) -> list[str]:
    keys = account_keys(transaction)
    required = transaction["transaction"]["message"]["header"][
        "numRequiredSignatures"
    ]
    return keys[:required]


def verify_live(client: RpcClient, fixture_path: Path) -> dict[str, Any]:
    fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
    require(fixture["cluster"] == "devnet", "fixture cluster is not devnet")
    program = fixture["program"]
    agreement_expected = fixture["agreement"]
    milestone_expected = fixture["milestone"]
    require(program["program_id"] == PROGRAM_ID, "fixture Program ID mismatch")

    context_slot, program_info, program_data = client.account(PROGRAM_ID)
    require(program_info["executable"] is True, "program account is not executable")
    require(program_info["owner"] == UPGRADEABLE_LOADER_ID, "program loader mismatch")
    programdata_address = decode_program_account(program_data)
    require(
        programdata_address == program["programdata_address"],
        "ProgramData address differs from fixture",
    )
    _, programdata_info, programdata_data = client.account(programdata_address)
    require(
        programdata_info["owner"] == UPGRADEABLE_LOADER_ID,
        "ProgramData loader mismatch",
    )
    decoded_programdata = decode_programdata_account(programdata_data)
    require(
        decoded_programdata["deployed_slot"] == program["deployed_slot"],
        "deployed slot differs from fixture",
    )
    require(
        decoded_programdata["upgrade_authority"] == program["upgrade_authority"],
        "upgrade authority differs from fixture",
    )
    local_sbf = (ROOT / "fixtures" / "prazopay-v2.so").read_bytes()
    require(len(local_sbf) == program["local_sbf_length"], "v2 SBF length mismatch")
    require(
        hashlib.sha256(local_sbf).hexdigest() == program["sbf_sha256"],
        "v2 SBF hash mismatch",
    )
    deployed_bytes = decoded_programdata["program_bytes"]
    require(
        len(deployed_bytes) == program["programdata_length"],
        "ProgramData length differs from fixture",
    )
    require(
        deployed_bytes[: len(local_sbf)] == local_sbf,
        "deployed program prefix differs byte-for-byte from v2 SBF",
    )
    padding = deployed_bytes[len(local_sbf) :]
    require(
        len(padding) == program["zero_padding_length"] and not any(padding),
        "ProgramData padding is not the disclosed all-zero suffix",
    )

    agreement_slot, agreement_info, agreement_data = client.account(
        agreement_expected["address"]
    )
    require(agreement_info["owner"] == PROGRAM_ID, "Agreement owner mismatch")
    agreement = decode_agreement_account(agreement_data)
    for field in (
        "funder",
        "worker",
        "task_id_sha256",
        "terms_sha256",
        "amount_lamports",
        "delivery_window_secs",
        "review_window_secs",
        "funding_window_secs",
        "proposal_expires_at",
        "milestone",
        "silence_acceptance",
        "status",
    ):
        require(
            agreement[field] == agreement_expected[field],
            f"Agreement {field} differs from fixture",
        )
    require(
        agreement["proposal_expires_at"] - agreement["proposed_at"]
        == agreement_expected["proposal_lifetime_secs"],
        "Agreement proposal lifetime differs from committed terms",
    )
    require(agreement["accepted_at"] >= agreement["proposed_at"], "invalid acceptance time")

    milestone_slot, milestone_info, milestone_data = client.account(
        milestone_expected["address"]
    )
    require(milestone_info["owner"] == PROGRAM_ID, "Milestone owner mismatch")
    milestone = decode_milestone_account(milestone_data)
    for field in (
        "funder",
        "worker",
        "task_id_sha256",
        "terms_sha256",
        "evidence_sha256",
        "amount_lamports",
        "due_at",
        "review_window_secs",
        "revision_count",
        "status",
        "protocol_version",
    ):
        require(
            milestone[field] == milestone_expected[field],
            f"Milestone {field} differs from fixture",
        )
    funded_at = milestone["due_at"] - agreement["delivery_window_secs"]
    require(funded_at >= agreement["accepted_at"], "Milestone predates acceptance")
    require(
        funded_at <= agreement["accepted_at"] + agreement["funding_window_secs"],
        "Milestone was funded after the accepted funding window",
    )

    signatures = {"upgrade": program["upgrade_signature"], **fixture["transactions"]}
    labels = list(signatures)
    status_values = client.call(
        "getSignatureStatuses",
        [list(signatures.values()), {"searchTransactionHistory": True}],
    )["value"]
    require(len(status_values) == len(labels), "signature status length mismatch")
    transactions: dict[str, dict[str, Any]] = {}
    slots: dict[str, int] = {}
    block_times: dict[str, int] = {}
    for label, signature, status in zip(labels, signatures.values(), status_values):
        require(status is not None, f"{label} transaction was not found")
        require(status["err"] is None, f"{label} transaction failed")
        require(status["confirmationStatus"] == "finalized", f"{label} is not finalized")
        transaction = client.call(
            "getTransaction",
            [
                signature,
                {
                    "commitment": "finalized",
                    "encoding": "json",
                    "maxSupportedTransactionVersion": 0,
                },
            ],
        )
        require(transaction is not None, f"{label} transaction body was not found")
        require(transaction["meta"]["err"] is None, f"{label} transaction meta failed")
        require(PROGRAM_ID in account_keys(transaction), f"{label} omits PrazoPay Program")
        require(transaction["blockTime"] is not None, f"{label} has no block time")
        transactions[label] = transaction
        slots[label] = transaction["slot"]
        block_times[label] = transaction["blockTime"]

    order = ["upgrade", "propose", "accept", "fund", "submit", "approve"]
    require(
        all(slots[left] <= slots[right] for left, right in zip(order, order[1:])),
        "transaction slots are not in lifecycle order",
    )
    agreement_address = agreement_expected["address"]
    milestone_address = milestone_expected["address"]
    for label in ("propose", "accept", "fund"):
        require(
            agreement_address in account_keys(transactions[label]),
            f"{label} does not reference the Agreement",
        )
    for label in ("fund", "submit", "approve"):
        require(
            milestone_address in account_keys(transactions[label]),
            f"{label} does not reference the Milestone",
        )
    funder = agreement_expected["funder"]
    worker = agreement_expected["worker"]
    require(funder in signer_keys(transactions["propose"]), "Funder did not sign proposal")
    require(worker not in signer_keys(transactions["propose"]), "Worker signed proposal")
    require(worker in signer_keys(transactions["accept"]), "Worker did not sign acceptance")
    require(funder in signer_keys(transactions["fund"]), "Funder did not sign funding")
    require(worker in signer_keys(transactions["submit"]), "Worker did not sign delivery")
    require(funder in signer_keys(transactions["approve"]), "Funder did not sign approval")

    approval = transactions["approve"]
    approval_keys = account_keys(approval)
    worker_index = approval_keys.index(worker)
    worker_delta = (
        approval["meta"]["postBalances"][worker_index]
        - approval["meta"]["preBalances"][worker_index]
    )
    require(
        worker_delta == fixture["worker_balance"]["delta_lamports"] == 1,
        "Worker approval-transaction balance delta is not exactly 1 lamport",
    )
    require(
        fixture["worker_balance"]["after_lamports"]
        - fixture["worker_balance"]["before_lamports"]
        == 1,
        "recorded Worker balance observations differ by more than 1 lamport",
    )

    return {
        "schema": "prazopay.live-verification.v2",
        "cluster": "devnet",
        "rpc_context_slot": context_slot,
        "program_id": PROGRAM_ID,
        "programdata_address": programdata_address,
        "deployed_slot": decoded_programdata["deployed_slot"],
        "sbf_sha256": program["sbf_sha256"],
        "agreement": agreement_expected["address"],
        "agreement_observed_slot": agreement_slot,
        "agreement_status": agreement["status"],
        "milestone": milestone_expected["address"],
        "milestone_observed_slot": milestone_slot,
        "milestone_status": milestone["status"],
        "transaction_slots": slots,
        "transaction_block_times": block_times,
        "worker_balance_delta_lamports": worker_delta,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify PrazoPay v2 against finalized Solana devnet state."
    )
    parser.add_argument(
        "--rpc-url",
        default=os.environ.get("SOLANA_RPC_URL", DEFAULT_RPC_URL),
    )
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--attempts", type=int, default=5)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        require(args.attempts >= 1, "--attempts must be at least 1")
        report = verify_live(RpcClient(args.rpc_url, args.timeout, args.attempts), args.fixture)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"RPC_ENDPOINT={display_rpc_url(args.rpc_url)}")
        print(f"PROGRAM_ID={report['program_id']}")
        print(f"DEPLOYED_SLOT={report['deployed_slot']}")
        print(f"ONCHAIN_SBF_SHA256={report['sbf_sha256']}")
        print(f"AGREEMENT_STATUS={report['agreement_status'].upper()}")
        print(f"MILESTONE_STATUS={report['milestone_status'].upper()}")
        print("FINALIZED_TRANSACTIONS=6")
        print(f"WORKER_BALANCE_DELTA={report['worker_balance_delta_lamports']}_LAMPORT")
        print("LIVE_DEVNET_V2_VERIFY=PASS")
        return 0
    except (VerificationError, KeyError, ValueError, TypeError, OSError) as error:
        print(f"LIVE_DEVNET_V2_VERIFY=FAIL: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
