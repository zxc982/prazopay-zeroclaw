#!/usr/bin/env python3
"""Read-only, live verification of PrazoPay's public Solana devnet evidence."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import struct
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE = ROOT / "fixtures" / "devnet-fair-lifecycle.json"
DEFAULT_RPC_URL = "https://api.devnet.solana.com"
PROGRAM_ID = "DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm"
UPGRADEABLE_LOADER_ID = "BPFLoaderUpgradeab1e11111111111111111111111"
ACCOUNT_DATA_LEN = 231
PROTOCOL_V1_FLAG = 0x80
PROTOCOL_V2_FLAG = 0x40
BASE58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


class VerificationError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise VerificationError(message)


def base58_encode(data: bytes) -> str:
    leading_zeroes = len(data) - len(data.lstrip(b"\0"))
    number = int.from_bytes(data, "big")
    encoded = ""
    while number:
        number, remainder = divmod(number, 58)
        encoded = BASE58_ALPHABET[remainder] + encoded
    return ("1" * leading_zeroes) + encoded


def decode_program_account(data: bytes) -> str:
    require(len(data) == 36, f"program account length is {len(data)}, expected 36")
    require(
        struct.unpack_from("<I", data, 0)[0] == 2,
        "program account is not UpgradeableLoaderState::Program",
    )
    return base58_encode(data[4:36])


def decode_programdata_account(data: bytes) -> dict[str, Any]:
    require(len(data) >= 13, "ProgramData account is truncated")
    require(
        struct.unpack_from("<I", data, 0)[0] == 3,
        "account is not UpgradeableLoaderState::ProgramData",
    )
    deployed_slot = struct.unpack_from("<Q", data, 4)[0]
    authority_option = data[12]
    if authority_option == 0:
        header_length = 13
        authority = None
    elif authority_option == 1:
        require(len(data) >= 45, "ProgramData authority is truncated")
        header_length = 45
        authority = base58_encode(data[13:45])
    else:
        raise VerificationError(f"invalid ProgramData authority option {authority_option}")
    return {
        "deployed_slot": deployed_slot,
        "upgrade_authority": authority,
        "program_bytes": data[header_length:],
    }


def decode_milestone_account(data: bytes) -> dict[str, Any]:
    require(
        len(data) == ACCOUNT_DATA_LEN,
        f"milestone account length is {len(data)}, expected {ACCOUNT_DATA_LEN}",
    )
    discriminator = hashlib.sha256(b"account:Milestone").digest()[:8]
    require(data[:8] == discriminator, "milestone discriminator mismatch")

    offset = 8

    def take(length: int) -> bytes:
        nonlocal offset
        result = data[offset : offset + length]
        require(len(result) == length, "milestone account is truncated")
        offset += length
        return result

    funder = base58_encode(take(32))
    worker = base58_encode(take(32))
    task_id = take(32).hex()
    terms_hash = take(32).hex()
    evidence_hash = take(32).hex()
    feedback_hash = take(32).hex()
    amount_lamports = struct.unpack("<Q", take(8))[0]
    due_at = struct.unpack("<q", take(8))[0]
    review_window_secs = struct.unpack("<I", take(4))[0]
    submitted_at = struct.unpack("<q", take(8))[0]
    versioned_revision = take(1)[0]
    status_value = take(1)[0]
    bump = take(1)[0]
    require(offset == ACCOUNT_DATA_LEN, "milestone account has trailing data")
    statuses = {0: "open", 1: "submitted", 2: "paid", 3: "refunded"}
    require(status_value in statuses, f"invalid milestone status {status_value}")
    return {
        "funder": funder,
        "worker": worker,
        "task_id_sha256": task_id,
        "terms_sha256": terms_hash,
        "evidence_sha256": evidence_hash,
        "feedback_sha256": feedback_hash,
        "amount_lamports": amount_lamports,
        "due_at": due_at,
        "review_window_secs": review_window_secs,
        "submitted_at": submitted_at,
        "protocol_version": (
            2
            if versioned_revision & PROTOCOL_V2_FLAG
            else 1
            if versioned_revision & PROTOCOL_V1_FLAG
            else 0
        ),
        "revision_count": versioned_revision & 0x3F,
        "status": statuses[status_value],
        "bump": bump,
    }


class RpcClient:
    def __init__(self, url: str, timeout: float, attempts: int) -> None:
        self.url = url
        self.timeout = timeout
        self.attempts = attempts
        self.request_id = 0

    def call(self, method: str, params: list[Any]) -> Any:
        self.request_id += 1
        payload = json.dumps(
            {
                "jsonrpc": "2.0",
                "id": self.request_id,
                "method": method,
                "params": params,
            }
        ).encode()
        request = urllib.request.Request(
            self.url,
            data=payload,
            headers={
                "Content-Type": "application/json",
                "User-Agent": "prazopay-live-verifier/1",
            },
            method="POST",
        )
        last_error: Exception | None = None
        for attempt in range(self.attempts):
            try:
                with urllib.request.urlopen(request, timeout=self.timeout) as response:
                    body = json.loads(response.read().decode())
                if "error" in body:
                    raise VerificationError(
                        f"{method} RPC error {json.dumps(body['error'], sort_keys=True)}"
                    )
                require("result" in body, f"{method} response has no result")
                return body["result"]
            except (
                OSError,
                TimeoutError,
                urllib.error.HTTPError,
                urllib.error.URLError,
                json.JSONDecodeError,
            ) as error:
                last_error = error
                if attempt + 1 < self.attempts:
                    time.sleep(2**attempt)
        error_type = type(last_error).__name__ if last_error else "UnknownError"
        raise VerificationError(
            f"{method} transport failed after {self.attempts} attempts ({error_type})"
        )

    def account(self, address: str) -> tuple[int, dict[str, Any], bytes]:
        response = self.call(
            "getAccountInfo",
            [address, {"commitment": "finalized", "encoding": "base64"}],
        )
        require(response["value"] is not None, f"account {address} does not exist")
        value = response["value"]
        encoded, encoding = value["data"]
        require(encoding == "base64", f"unexpected account encoding {encoding}")
        return response["context"]["slot"], value, base64.b64decode(encoded, validate=True)


def account_keys(transaction: dict[str, Any]) -> list[str]:
    keys = transaction["transaction"]["message"]["accountKeys"]
    return [key if isinstance(key, str) else key["pubkey"] for key in keys]


def verify_live(client: RpcClient, fixture_path: Path) -> dict[str, Any]:
    fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
    require(fixture["cluster"] == "devnet", "fixture cluster is not devnet")
    program = fixture["program"]
    lifecycle = fixture["lifecycle"]
    require(program["program_id"] == PROGRAM_ID, "fixture Program ID mismatch")

    context_slot, program_info, program_data = client.account(PROGRAM_ID)
    require(program_info["executable"] is True, "program account is not executable")
    require(program_info["owner"] == UPGRADEABLE_LOADER_ID, "program loader mismatch")
    programdata_address = decode_program_account(program_data)
    require(
        programdata_address == program["programdata_address"],
        "ProgramData address differs from the fixture",
    )

    _, programdata_info, programdata_data = client.account(programdata_address)
    require(
        programdata_info["owner"] == UPGRADEABLE_LOADER_ID,
        "ProgramData loader mismatch",
    )
    decoded_programdata = decode_programdata_account(programdata_data)
    require(
        decoded_programdata["deployed_slot"] == program["deployed_slot"],
        "deployed slot differs from the fixture",
    )
    require(
        decoded_programdata["upgrade_authority"] == program["upgrade_authority"],
        "upgrade authority differs from the fixture",
    )

    local_sbf = (ROOT / "fixtures" / "prazopay-v1.so").read_bytes()
    expected_hash = program["local_and_deployed_prefix_sha256"]
    require(
        hashlib.sha256(local_sbf).hexdigest() == expected_hash,
        "local SBF hash differs from the fixture",
    )
    deployed_bytes = decoded_programdata["program_bytes"]
    require(
        len(deployed_bytes) == program["programdata_length"],
        "on-chain ProgramData length differs from the fixture",
    )
    require(
        deployed_bytes[: len(local_sbf)] == local_sbf,
        "on-chain program prefix differs byte-for-byte from the committed SBF",
    )
    padding = deployed_bytes[len(local_sbf) :]
    require(
        len(padding) == program["zero_padding_length"] and not any(padding),
        "on-chain ProgramData padding differs from the fixture",
    )

    milestone_address = lifecycle["milestone"]
    milestone_slot, milestone_info, milestone_data = client.account(milestone_address)
    require(milestone_info["owner"] == PROGRAM_ID, "milestone owner mismatch")
    require(milestone_info["executable"] is False, "milestone is unexpectedly executable")
    milestone = decode_milestone_account(milestone_data)
    for field in ("funder", "worker", "amount_lamports", "review_window_secs"):
        require(
            milestone[field] == lifecycle[field],
            f"milestone {field} differs from the fixture",
        )
    require(milestone["protocol_version"] == 1, "milestone is not protocol v1")
    require(milestone["status"] == lifecycle["final_status"], "terminal status mismatch")
    require(
        milestone["submitted_at"] == lifecycle["terminal_at"],
        "terminal timestamp mismatch",
    )

    transaction_map = lifecycle["transactions"]
    labels = list(transaction_map)
    signatures = [transaction_map[label] for label in labels]
    statuses = client.call(
        "getSignatureStatuses",
        [signatures, {"searchTransactionHistory": True}],
    )["value"]
    require(len(statuses) == len(signatures), "signature-status result length mismatch")

    transaction_slots: dict[str, int] = {}
    transactions: dict[str, dict[str, Any]] = {}
    for label, signature, status in zip(labels, signatures, statuses):
        require(status is not None, f"{label} transaction was not found")
        require(status["err"] is None, f"{label} transaction failed")
        require(
            status["confirmationStatus"] == "finalized",
            f"{label} transaction is not finalized",
        )
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
        keys = account_keys(transaction)
        require(PROGRAM_ID in keys, f"{label} transaction does not reference PrazoPay")
        require(
            milestone_address in keys,
            f"{label} transaction does not reference the recorded milestone",
        )
        transaction_slots[label] = transaction["slot"]
        transactions[label] = transaction

    require(
        transaction_slots["create"]
        <= transaction_slots["submit"]
        <= transaction_slots["permissionless_settle"],
        "transaction slots are out of lifecycle order",
    )
    settlement = transactions["permissionless_settle"]
    settlement_keys = account_keys(settlement)
    require(lifecycle["worker"] in settlement_keys, "settlement omits the Worker")
    worker_index = settlement_keys.index(lifecycle["worker"])
    worker_delta = (
        settlement["meta"]["postBalances"][worker_index]
        - settlement["meta"]["preBalances"][worker_index]
    )
    require(
        worker_delta == lifecycle["worker_balance_delta_lamports"] == 1,
        "Worker balance delta is not exactly 1 lamport",
    )

    return {
        "schema": "prazopay.live-verification.v1",
        "cluster": "devnet",
        "rpc_context_slot": context_slot,
        "program_id": PROGRAM_ID,
        "programdata_address": programdata_address,
        "deployed_slot": decoded_programdata["deployed_slot"],
        "sbf_sha256": expected_hash,
        "milestone": milestone_address,
        "milestone_observed_slot": milestone_slot,
        "milestone_status": milestone["status"],
        "transaction_slots": transaction_slots,
        "worker_balance_delta_lamports": worker_delta,
    }


def display_rpc_url(url: str) -> str:
    parsed = urllib.parse.urlsplit(url)
    hostname = parsed.hostname or "<invalid-host>"
    port = f":{parsed.port}" if parsed.port else ""
    return f"{parsed.scheme}://{hostname}{port}"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify PrazoPay's recorded evidence against live Solana devnet."
    )
    parser.add_argument(
        "--rpc-url",
        default=os.environ.get("SOLANA_RPC_URL", DEFAULT_RPC_URL),
        help="Solana devnet JSON-RPC endpoint (default: public devnet endpoint)",
    )
    parser.add_argument(
        "--fixture",
        type=Path,
        default=DEFAULT_FIXTURE,
        help="fair lifecycle fixture",
    )
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--attempts", type=int, default=3)
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
        print(f"PROGRAMDATA={report['programdata_address']}")
        print(f"ONCHAIN_SBF_SHA256={report['sbf_sha256']}")
        print(f"MILESTONE_STATUS={report['milestone_status'].upper()}")
        print("FINALIZED_TRANSACTIONS=3")
        print(f"WORKER_BALANCE_DELTA={report['worker_balance_delta_lamports']}_LAMPORT")
        print("LIVE_DEVNET_VERIFY=PASS")
        return 0
    except (VerificationError, KeyError, ValueError, TypeError, OSError) as error:
        print(f"LIVE_DEVNET_VERIFY=FAIL: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
