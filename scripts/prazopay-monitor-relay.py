#!/usr/bin/env python3
"""Durable local delivery relay for PrazoPay ZeroClaw heartbeat cards."""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import logging
import os
import re
import subprocess
import tempfile
import threading
import time
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Callable


EVENT_ID_PATTERN = re.compile(r"\bprazopay:[0-9a-f]{32}\b")
TERMINAL_EVENT_PATTERN = re.compile(
    r"\b(?:SETTLEMENT_SUCCESS|MILESTONE_FAILED|AGREEMENT_REJECTED|"
    r"AGREEMENT_EXPIRED|AGREEMENT_PROPOSAL_EXPIRED|"
    r"AGREEMENT_FUNDING_WINDOW_EXPIRED)\b"
)
FAILURE_PATTERN = re.compile(r"^NO_REPLY\[FAIL\]:\s*([A-Z0-9_]{3,64})$")
HEALTH_STAGE_PATTERN = re.compile(r"^(?:first|30m|2h|day_[1-9][0-9]*)$")
DISCORD_CHANNEL_PATTERN = re.compile(r"^[0-9]{17,20}$")
MAX_REQUEST_BYTES = 64 * 1024
AGREEMENT_ACCEPTANCE_GUARD = (
    "Worker acceptance requires an explicit accept_agreement transaction "
    "signed by the named Worker."
)
SILENCE_POLICY_GUARD = (
    "Silence acceptance applies only after delivery during the Funder review phase."
)
AGREEMENT_SCHEMA_GUARD = "Status schema: prazopay.agreement-status.v1"
AGREEMENT_PROTOCOL_GUARD = "On-chain Agreement protocol: v2"
MILESTONE_SCHEMA_GUARD = "Status schema: prazopay.status.v2"
MILESTONE_HANDOFF_SCHEMA_GUARD = "Milestone status schema: prazopay.status.v2"
MILESTONE_PROTOCOL_GUARD = "On-chain Milestone protocol: v2"
MILESTONE_ACCEPTANCE_POLICY_GUARD = (
    "Acceptance policy: worker_signed_silence_acceptance"
)
AMBIGUOUS_PROTOCOL_LABEL = "Protocol version:"
LEGACY_MILESTONE_PROTOCOL = "On-chain Milestone protocol: v1"
LEGACY_ACCEPTANCE_POLICY = "Acceptance policy: explicit_silence_acceptance"


class StateStore:
    def __init__(self, path: Path, milestone: str) -> None:
        self.path = path
        self.milestone = milestone
        self.lock = threading.Lock()
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)

    def load(self) -> dict:
        if not self.path.exists():
            return {
                "schema": "prazopay.delivery-state.v1",
                "milestone": self.milestone,
                "closed": False,
                "delivered_event_ids": [],
            }
        with self.path.open("r", encoding="utf-8") as handle:
            state = json.load(handle)
        if (
            state.get("schema") != "prazopay.delivery-state.v1"
            or state.get("milestone") != self.milestone
            or not isinstance(state.get("closed"), bool)
            or not isinstance(state.get("delivered_event_ids"), list)
        ):
            raise ValueError("delivery state is invalid or belongs to another milestone")
        event_ids = state["delivered_event_ids"]
        if (
            any(
                not isinstance(event_id, str)
                or EVENT_ID_PATTERN.fullmatch(event_id) is None
                for event_id in event_ids
            )
            or len(event_ids) != len(set(event_ids))
        ):
            raise ValueError("delivery state contains invalid event IDs")
        if state["closed"]:
            terminal_event_id = state.get("terminal_event_id")
            if (
                not isinstance(terminal_event_id, str)
                or EVENT_ID_PATTERN.fullmatch(terminal_event_id) is None
                or terminal_event_id not in event_ids
            ):
                raise ValueError("closed delivery state has no valid terminal event")
        health = state.get("monitor_health")
        if health is not None:
            if (
                not isinstance(health, dict)
                or health.get("status") != "degraded"
                or not isinstance(health.get("degraded_since"), int)
                or health["degraded_since"] < 0
                or not isinstance(health.get("failure_code"), str)
                or re.fullmatch(r"[A-Z0-9_]{3,64}", health["failure_code"])
                is None
                or not isinstance(health.get("delivered_stages"), list)
                or any(
                    not isinstance(stage, str)
                    or HEALTH_STAGE_PATTERN.fullmatch(stage) is None
                    for stage in health["delivered_stages"]
                )
                or len(health["delivered_stages"])
                != len(set(health["delivered_stages"]))
            ):
                raise ValueError("delivery state contains invalid monitor health")
        return state

    def save(self, state: dict) -> None:
        encoded = json.dumps(state, indent=2, sort_keys=True) + "\n"
        descriptor, temporary_name = tempfile.mkstemp(
            prefix=".delivery-state-",
            suffix=".tmp",
            dir=self.path.parent,
            text=True,
        )
        temporary_path = Path(temporary_name)
        try:
            os.chmod(temporary_path, 0o600)
            with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(encoded)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary_path, self.path)
            # Windows cannot open a directory with os.open(O_RDONLY). Atomic
            # replace plus the file fsync above is the strongest portable path.
            if os.name != "nt":
                directory_descriptor = os.open(self.path.parent, os.O_RDONLY)
                try:
                    os.fsync(directory_descriptor)
                finally:
                    os.close(directory_descriptor)
        finally:
            if temporary_path.exists():
                temporary_path.unlink()


class DeliveryProcessor:
    def __init__(
        self,
        state_store: StateStore,
        sender: Callable[[str, str], bool],
        disable_heartbeat: Callable[[], bool],
        expected_recipient: str,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.state_store = state_store
        self.sender = sender
        self.disable_heartbeat = disable_heartbeat
        self.expected_recipient = expected_recipient
        self.clock = clock

    def process(self, content: str, recipient: str) -> tuple[int, str]:
        stripped = content.strip()
        if recipient != self.expected_recipient:
            return HTTPStatus.BAD_REQUEST, "recipient does not match configured Discord channel"

        failure = FAILURE_PATTERN.fullmatch(stripped.upper())
        if failure is not None:
            with self.state_store.lock:
                return self._process_failure_locked(failure.group(1), recipient)
        if stripped.upper() == "NO_REPLY":
            with self.state_store.lock:
                return self._process_quiet_locked(recipient)
        if stripped.upper().startswith("NO_REPLY"):
            return HTTPStatus.UNPROCESSABLE_ENTITY, "quiet output format is invalid"

        event_match = EVENT_ID_PATTERN.search(content)
        if event_match is None:
            return HTTPStatus.UNPROCESSABLE_ENTITY, "PrazoPay event_id is missing"
        event_id = event_match.group(0)
        terminal = TERMINAL_EVENT_PATTERN.search(content) is not None

        agreement_headings = (
            "PrazoPay Agreement Proposal",
            "PrazoPay Agreement Accepted",
            "PrazoPay Agreement Closed",
            "PrazoPay Escrow Funded",
        )
        milestone_headings = (
            "PrazoPay Active Alert",
            "PrazoPay Delay Alert",
            "PrazoPay Final Outcome",
        )
        normalized = content.replace("`", "").replace("**", "")
        if AMBIGUOUS_PROTOCOL_LABEL.lower() in normalized.lower():
            return (
                HTTPStatus.UNPROCESSABLE_ENTITY,
                "ambiguous Protocol version label is forbidden",
            )
        if LEGACY_MILESTONE_PROTOCOL.lower() in normalized.lower():
            return (
                HTTPStatus.UNPROCESSABLE_ENTITY,
                "legacy Milestone protocol is forbidden",
            )
        if LEGACY_ACCEPTANCE_POLICY.lower() in normalized.lower():
            return (
                HTTPStatus.UNPROCESSABLE_ENTITY,
                "legacy acceptance policy is forbidden",
            )
        if any(heading in content for heading in agreement_headings):
            expected_explorer = (
                "https://explorer.solana.com/address/"
                f"{self.state_store.milestone}?cluster=devnet"
            )
            if expected_explorer not in content:
                return (
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                    "Agreement Explorer URL is missing or does not match the monitored address",
                )

        if any(
            heading in content
            for heading in (
                "PrazoPay Agreement Proposal",
                "PrazoPay Agreement Accepted",
                "PrazoPay Agreement Closed",
            )
        ):
            missing = self._missing_guards(
                normalized,
                (AGREEMENT_SCHEMA_GUARD, AGREEMENT_PROTOCOL_GUARD),
            )
            if missing:
                return (
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                    f"Agreement card is missing exact v2 provenance: {missing}",
                )

        if "PrazoPay Escrow Funded" in content:
            missing = self._missing_guards(
                normalized,
                (
                    AGREEMENT_SCHEMA_GUARD,
                    AGREEMENT_PROTOCOL_GUARD,
                    MILESTONE_HANDOFF_SCHEMA_GUARD,
                    MILESTONE_PROTOCOL_GUARD,
                    MILESTONE_ACCEPTANCE_POLICY_GUARD,
                ),
            )
            if missing:
                return (
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                    f"funded handoff card is missing exact v2 provenance: {missing}",
                )

        if any(heading in content for heading in milestone_headings):
            missing = self._missing_guards(
                normalized,
                (
                    MILESTONE_SCHEMA_GUARD,
                    MILESTONE_PROTOCOL_GUARD,
                    MILESTONE_ACCEPTANCE_POLICY_GUARD,
                ),
            )
            if missing:
                return (
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                    f"Milestone card is missing exact v2 provenance: {missing}",
                )

        if (
            "PrazoPay Agreement Proposal" in content
            or "PrazoPay Agreement Accepted" in content
        ):
            if AGREEMENT_ACCEPTANCE_GUARD not in normalized:
                return (
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                    "Agreement card does not require explicit Worker-signed acceptance",
                )
            if SILENCE_POLICY_GUARD not in normalized:
                return (
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                    "Agreement card does not constrain silence acceptance to Funder review",
                )

        with self.state_store.lock:
            state = self.state_store.load()
            delivered = set(state["delivered_event_ids"])
            if state["closed"]:
                if not self.disable_heartbeat():
                    logging.warning(
                        "monitor is closed, but heartbeat config still could not be disabled"
                    )
                return HTTPStatus.OK, "monitor already closed"
            if event_id in delivered:
                if state.get("monitor_health") is not None:
                    recovery = self._recovery_card(state, recipient)
                    if recovery is not None:
                        return recovery
                return HTTPStatus.OK, "duplicate event suppressed"
            if not self.sender(content, recipient):
                return HTTPStatus.SERVICE_UNAVAILABLE, "Discord delivery failed; retry required"

            state["delivered_event_ids"].append(event_id)
            if terminal:
                state["closed"] = True
                state["terminal_event_id"] = event_id
            state.pop("monitor_health", None)
            self.state_store.save(state)

            if terminal and not self.disable_heartbeat():
                logging.warning(
                    "terminal event %s was delivered, but heartbeat config could not be disabled",
                    event_id,
                )

        return HTTPStatus.OK, "event delivered and committed"

    @staticmethod
    def _missing_guards(content: str, required: tuple[str, ...]) -> str:
        return ", ".join(guard for guard in required if guard not in content)

    def _process_failure_locked(self, failure_code: str, recipient: str) -> tuple[int, str]:
        state = self.state_store.load()
        if state["closed"]:
            if not self.disable_heartbeat():
                logging.warning("monitor is closed, but heartbeat config still could not be disabled")
            return HTTPStatus.OK, "monitor already closed"

        now = max(0, int(self.clock()))
        health = state.get("monitor_health")
        if health is None:
            health = {
                "status": "degraded",
                "degraded_since": now,
                "failure_code": failure_code,
                "delivered_stages": [],
            }
            state["monitor_health"] = health
            self.state_store.save(state)
        elif health["failure_code"] != failure_code:
            health["failure_code"] = failure_code

        stage = self._health_stage(now - health["degraded_since"], health["delivered_stages"])
        if stage is None:
            self.state_store.save(state)
            return HTTPStatus.OK, "degraded output suppressed between sparse stages"

        event_id = self._health_event_id(health["degraded_since"], stage, failure_code)
        if failure_code == "PRAZOPAY_PROTOCOL_MISMATCH":
            heading = "PrazoPay Monitor Integrity Block"
            event_code = "MONITOR_INTEGRITY_DEGRADED"
            explanation = (
                "- Protocol state: blocked; tool provenance did not match the "
                "required v2 tuple\n"
            )
            next_action = (
                "Next action: verify the installed read-only WASM hash and restart "
                "ZeroClaw. Do not sign or act on a workflow card until a fresh v2 "
                "read succeeds."
            )
        else:
            heading = "PrazoPay Monitor Degraded"
            event_code = "MONITOR_RPC_DEGRADED"
            explanation = (
                "- Protocol state: unknown; no transaction decision was inferred\n"
            )
            next_action = (
                "Next action: ZeroClaw will retry on the next heartbeat. "
                "Do not sign based on this infrastructure alert."
            )
        content = (
            f"{heading}\n"
            f"- Event: {event_code}\n"
            f"- Failure code: {failure_code}\n"
            f"- Reminder stage: {stage}\n"
            f"{explanation}"
            f"Event ID: {event_id}\n\n"
            f"{next_action}"
        )
        if not self.sender(content, recipient):
            return HTTPStatus.SERVICE_UNAVAILABLE, "Discord delivery failed; retry required"
        if event_id not in state["delivered_event_ids"]:
            state["delivered_event_ids"].append(event_id)
        health["delivered_stages"].append(stage)
        self.state_store.save(state)
        return HTTPStatus.OK, "degraded monitor alert delivered and committed"

    def _process_quiet_locked(self, recipient: str) -> tuple[int, str]:
        state = self.state_store.load()
        if state["closed"]:
            if not self.disable_heartbeat():
                logging.warning("monitor is closed, but heartbeat config still could not be disabled")
            return HTTPStatus.OK, "monitor already closed"
        recovery = self._recovery_card(state, recipient)
        if recovery is not None:
            return recovery
        return HTTPStatus.OK, "quiet output suppressed"

    def _recovery_card(self, state: dict, recipient: str) -> tuple[int, str] | None:
        health = state.get("monitor_health")
        if health is None:
            return None
        event_id = self._health_event_id(
            health["degraded_since"], "recovered", health["failure_code"]
        )
        recovery_event = (
            "MONITOR_INTEGRITY_RECOVERED"
            if health["failure_code"] == "PRAZOPAY_PROTOCOL_MISMATCH"
            else "MONITOR_RPC_RECOVERED"
        )
        content = (
            "PrazoPay Monitor Recovered\n"
            f"- Event: {recovery_event}\n"
            f"- Previous failure: {health['failure_code']}\n"
            "- Protocol state: read succeeded; normal monitoring resumed\n"
            f"Event ID: {event_id}\n\n"
            "Next action: Follow the next on-chain actionable-state card."
        )
        if not self.sender(content, recipient):
            return HTTPStatus.SERVICE_UNAVAILABLE, "Discord delivery failed; retry required"
        if event_id not in state["delivered_event_ids"]:
            state["delivered_event_ids"].append(event_id)
        state.pop("monitor_health", None)
        self.state_store.save(state)
        return HTTPStatus.OK, "monitor recovery delivered and committed"

    @staticmethod
    def _health_stage(elapsed: int, delivered_stages: list[str]) -> str | None:
        delivered = set(delivered_stages)
        if "first" not in delivered:
            return "first"
        if elapsed >= 30 * 60 and "30m" not in delivered:
            return "30m"
        if elapsed >= 2 * 60 * 60 and "2h" not in delivered:
            return "2h"
        if elapsed >= 24 * 60 * 60:
            stage = f"day_{elapsed // (24 * 60 * 60)}"
            if stage not in delivered:
                return stage
        return None

    def _health_event_id(self, degraded_since: int, stage: str, failure_code: str) -> str:
        material = f"{self.state_store.milestone}|{degraded_since}|{stage}|{failure_code}"
        return f"prazopay:{hashlib.sha256(material.encode('utf-8')).hexdigest()[:32]}"


class ZeroClawCommands:
    def __init__(self, binary: str, config_dir: str) -> None:
        self.binary = binary
        self.config_dir = config_dir

    def send_discord(self, content: str, recipient: str) -> bool:
        command = [
            self.binary,
            "channel",
            "send",
            content,
            "--channel-id",
            "discord.main",
            "--recipient",
            recipient,
            "--config-dir",
            self.config_dir,
        ]
        try:
            result = subprocess.run(
                command,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
                timeout=45,
            )
        except (OSError, subprocess.TimeoutExpired):
            return False
        return result.returncode == 0

    def disable_heartbeat(self) -> bool:
        command = [
            self.binary,
            "config",
            "set",
            "heartbeat.enabled",
            "false",
            "--no-interactive",
            "--config-dir",
            self.config_dir,
        ]
        try:
            result = subprocess.run(
                command,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
                timeout=20,
            )
        except (OSError, subprocess.TimeoutExpired):
            return False
        return result.returncode == 0


def build_handler(
    processor: DeliveryProcessor, expected_authorization: str
) -> type[BaseHTTPRequestHandler]:
    class RelayHandler(BaseHTTPRequestHandler):
        server_version = "PrazoPayRelay/1"

        def do_GET(self) -> None:
            if self.path != "/health":
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            try:
                state = processor.state_store.load()
                self._json_response(
                    HTTPStatus.OK,
                    {
                        "status": "ok",
                        "closed": state["closed"],
                        "delivered_events": len(state["delivered_event_ids"]),
                    },
                )
            except (OSError, ValueError, json.JSONDecodeError):
                self._json_response(
                    HTTPStatus.INTERNAL_SERVER_ERROR, {"status": "state_error"}
                )

        def do_POST(self) -> None:
            if self.path != "/heartbeat":
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            authorization = self.headers.get("Authorization", "")
            if not hmac.compare_digest(authorization, expected_authorization):
                self._json_response(HTTPStatus.UNAUTHORIZED, {"status": "unauthorized"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
            except ValueError:
                self._json_response(
                    HTTPStatus.BAD_REQUEST, {"status": "invalid_content_length"}
                )
                return
            if length <= 0 or length > MAX_REQUEST_BYTES:
                self._json_response(
                    HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
                    {"status": "request_size_invalid"},
                )
                return
            try:
                payload = json.loads(self.rfile.read(length))
                content = payload["content"]
                recipient = payload.get("recipient", "")
                if not isinstance(content, str) or not isinstance(recipient, str):
                    raise TypeError
            except (json.JSONDecodeError, KeyError, TypeError):
                self._json_response(
                    HTTPStatus.BAD_REQUEST, {"status": "payload_invalid"}
                )
                return

            try:
                status, detail = processor.process(content, recipient)
            except (OSError, ValueError, json.JSONDecodeError):
                logging.exception("delivery state failure")
                status, detail = HTTPStatus.INTERNAL_SERVER_ERROR, "state failure"
            self._json_response(status, {"status": detail})

        def log_message(self, message_format: str, *args: object) -> None:
            logging.info("%s - %s", self.address_string(), message_format % args)

        def _json_response(self, status: int, payload: dict) -> None:
            body = json.dumps(payload, separators=(",", ":")).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    return RelayHandler


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen-host", default="127.0.0.1")
    parser.add_argument("--listen-port", type=int, default=42620)
    parser.add_argument("--state-dir", required=True)
    parser.add_argument("--milestone", required=True)
    parser.add_argument("--discord-channel", required=True)
    parser.add_argument("--auth-token-file", required=True)
    parser.add_argument("--zeroclaw-bin", required=True)
    parser.add_argument("--config-dir", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.listen_host not in {"127.0.0.1", "::1"}:
        raise SystemExit("relay must bind to a loopback address")
    if not 1 <= args.listen_port <= 65535:
        raise SystemExit("listen port is invalid")
    if not DISCORD_CHANNEL_PATTERN.fullmatch(args.discord_channel):
        raise SystemExit("Discord channel must be a 17-20 digit snowflake")

    token = Path(args.auth_token_file).read_text(encoding="utf-8").strip()
    if len(token) < 32:
        raise SystemExit("relay authorization token is missing or too short")

    state_store = StateStore(Path(args.state_dir) / "delivery-state.json", args.milestone)
    commands = ZeroClawCommands(args.zeroclaw_bin, args.config_dir)
    processor = DeliveryProcessor(
        state_store,
        commands.send_discord,
        commands.disable_heartbeat,
        args.discord_channel,
    )
    handler = build_handler(processor, f"Bearer {token}")

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
    )
    server = ThreadingHTTPServer((args.listen_host, args.listen_port), handler)
    logging.info(
        "PrazoPay delivery relay listening on %s:%d", args.listen_host, args.listen_port
    )
    try:
        server.serve_forever(poll_interval=0.5)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
