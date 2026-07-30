#!/usr/bin/env python3
"""Durable local delivery relay for PrazoPay ZeroClaw heartbeat cards."""

from __future__ import annotations

import argparse
import hmac
import json
import logging
import os
import re
import subprocess
import tempfile
import threading
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Callable


EVENT_ID_PATTERN = re.compile(r"\bprazopay:[0-9a-f]{32}\b")
TERMINAL_EVENT_PATTERN = re.compile(r"\b(?:SETTLEMENT_SUCCESS|MILESTONE_FAILED)\b")
DISCORD_CHANNEL_PATTERN = re.compile(r"^[0-9]{17,20}$")
MAX_REQUEST_BYTES = 64 * 1024


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
    ) -> None:
        self.state_store = state_store
        self.sender = sender
        self.disable_heartbeat = disable_heartbeat
        self.expected_recipient = expected_recipient

    def process(self, content: str, recipient: str) -> tuple[int, str]:
        stripped = content.strip()
        if stripped.upper().startswith("NO_REPLY"):
            return HTTPStatus.OK, "quiet output suppressed"
        if recipient != self.expected_recipient:
            return HTTPStatus.BAD_REQUEST, "recipient does not match configured Discord channel"

        event_match = EVENT_ID_PATTERN.search(content)
        if event_match is None:
            return HTTPStatus.UNPROCESSABLE_ENTITY, "PrazoPay event_id is missing"
        event_id = event_match.group(0)
        terminal = TERMINAL_EVENT_PATTERN.search(content) is not None

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
                return HTTPStatus.OK, "duplicate event suppressed"
            if not self.sender(content, recipient):
                return HTTPStatus.SERVICE_UNAVAILABLE, "Discord delivery failed; retry required"

            state["delivered_event_ids"].append(event_id)
            if terminal:
                state["closed"] = True
                state["terminal_event_id"] = event_id
            self.state_store.save(state)

            if terminal and not self.disable_heartbeat():
                logging.warning(
                    "terminal event %s was delivered, but heartbeat config could not be disabled",
                    event_id,
                )

        return HTTPStatus.OK, "event delivered and committed"


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
            "discord",
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
