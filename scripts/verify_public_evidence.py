#!/usr/bin/env python3
"""Validate the internally consistent, public-only PrazoPay evidence bundle."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "fixtures"
PROGRAM_ID = "DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm"
SBF_SHA256 = "b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2"
SBF_V2_SHA256 = "a54c676c98f526425ba77b54cfdb64a6ddddab2cf218d12f732dfa95bb4d8294"


def load_json(name: str) -> dict:
    with (FIXTURES / name).open(encoding="utf-8") as handle:
        return json.load(handle)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"PUBLIC_EVIDENCE=FAIL: {message}")


def main() -> None:
    deployment = load_json("devnet-deployment.json")
    lifecycle = load_json("devnet-lifecycle.json")
    fair = load_json("devnet-fair-lifecycle.json")
    monitor = load_json("devnet-active-monitor.json")
    v2 = load_json("devnet-v2-lifecycle.json")

    require(deployment["cluster"] == "devnet", "baseline deployment cluster")
    require(lifecycle["cluster"] == "devnet", "lifecycle cluster")
    require(fair["cluster"] == "devnet", "fair lifecycle cluster")
    require(monitor["cluster"] == "devnet", "monitor cluster")
    require(v2["cluster"] == "devnet", "v2 lifecycle cluster")
    require(deployment["program_id"] == PROGRAM_ID, "baseline deployment program ID")
    require(lifecycle["program_id"] == PROGRAM_ID, "lifecycle program ID")
    require(fair["program"]["program_id"] == PROGRAM_ID, "fair lifecycle program ID")
    require(monitor["program_id"] == PROGRAM_ID, "monitor program ID")
    require(v2["program"]["program_id"] == PROGRAM_ID, "v2 program ID")
    require(
        deployment["local_sbf_sha256"] == deployment["deployed_sbf_sha256"],
        "baseline deployment hash match",
    )
    require(
        deployment["confirmation_status"] == "finalized",
        "baseline deployment finality",
    )

    sbf = (FIXTURES / "prazopay-v1.so").read_bytes()
    require(hashlib.sha256(sbf).hexdigest() == SBF_SHA256, "SBF hash")
    require(
        fair["program"]["local_and_deployed_prefix_sha256"] == SBF_SHA256,
        "deployed-prefix hash",
    )
    require(fair["program"]["local_sbf_length"] == len(sbf), "SBF length")

    sbf_v2 = (FIXTURES / "prazopay-v2.so").read_bytes()
    require(hashlib.sha256(sbf_v2).hexdigest() == SBF_V2_SHA256, "v2 SBF hash")
    require(v2["program"]["sbf_sha256"] == SBF_V2_SHA256, "v2 deployed hash")
    require(v2["program"]["local_sbf_length"] == len(sbf_v2), "v2 SBF length")
    require(v2["agreement"]["status"] == "funded", "v2 Agreement status")
    require(v2["milestone"]["protocol_version"] == 2, "v2 protocol marker")
    require(v2["milestone"]["status"] == "paid", "v2 terminal state")
    require(v2["worker_balance"]["delta_lamports"] == 1, "v2 worker delta")
    require(
        set(v2["transactions"]) == {"propose", "accept", "fund", "submit", "approve"},
        "v2 lifecycle transaction set",
    )

    current = fair["lifecycle"]
    require(current["amount_lamports"] == 1, "current milestone amount")
    require(current["final_status"] == "paid", "current milestone terminal state")
    require(current["worker_balance_delta_lamports"] == 1, "worker balance delta")
    require(
        set(current["transactions"]) == {"create", "submit", "permissionless_settle"},
        "current lifecycle transaction set",
    )

    require(lifecycle["amount_lamports_per_milestone"] == 1, "legacy fixture amount")
    require(lifecycle["revision_then_approve"]["status"] == "PAID", "approval path")
    require(lifecycle["silent_review_claim"]["status"] == "PAID", "claim path")
    require(lifecycle["expiry_refund"]["status"] == "REFUNDED", "refund path")
    require(
        monitor["heartbeat"]["allowed_tools"] == ["prazopay_status"],
        "monitor capability boundary",
    )

    trace_count = 0
    with (FIXTURES / "zeroclaw-trace.jsonl").open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if line.strip():
                try:
                    json.loads(line)
                except json.JSONDecodeError as exc:
                    raise SystemExit(
                        f"PUBLIC_EVIDENCE=FAIL: trace line {line_number}: {exc}"
                    ) from exc
                trace_count += 1
    require(trace_count > 0, "ZeroClaw trace is empty")

    print("PUBLIC_EVIDENCE=PASS")


if __name__ == "__main__":
    main()
