import importlib.util
import json
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer
from pathlib import Path


MODULE_PATH = (
    Path(__file__).resolve().parents[1] / "prazopay-monitor-relay.py"
)
SPEC = importlib.util.spec_from_file_location("prazopay_monitor_relay", MODULE_PATH)
RELAY = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(RELAY)


MILESTONE = "ikUaYZUARH3KXK9y98MgfgSVsZJu3tcgHfgeKnCTTqB"
CHANNEL = "1532408222730686565"
EVENT_ONE = "prazopay:11111111111111111111111111111111"
EVENT_TWO = "prazopay:22222222222222222222222222222222"
AGREEMENT_EXPLORER = (
    f"https://explorer.solana.com/address/{MILESTONE}?cluster=devnet"
)


def milestone_card(event_id, heading="PrazoPay Active Alert", event=None):
    lines = [
        heading,
        RELAY.MILESTONE_SCHEMA_GUARD,
        RELAY.MILESTONE_PROTOCOL_GUARD,
        RELAY.MILESTONE_ACCEPTANCE_POLICY_GUARD,
    ]
    if event is not None:
        lines.append(f"Event: {event}")
    lines.append(f"Event ID: {event_id}")
    return "\n".join(lines)


def agreement_provenance():
    return "\n".join(
        (RELAY.AGREEMENT_SCHEMA_GUARD, RELAY.AGREEMENT_PROTOCOL_GUARD)
    )


class RelayTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.sent = []
        self.disabled = 0
        self.now = 1_000_000
        store = RELAY.StateStore(
            Path(self.temporary.name) / "delivery-state.json", MILESTONE
        )

        def sender(content, recipient):
            self.sent.append((content, recipient))
            return True

        def disable():
            self.disabled += 1
            return True

        self.processor = RELAY.DeliveryProcessor(
            store, sender, disable, CHANNEL, clock=lambda: self.now
        )

    def tearDown(self):
        self.temporary.cleanup()

    def test_same_event_is_delivered_once(self):
        content = milestone_card(EVENT_ONE)
        first = self.processor.process(content, CHANNEL)
        second = self.processor.process(content, CHANNEL)
        self.assertEqual(first[0], 200)
        self.assertEqual(second[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.assertEqual(self.disabled, 0)

    def test_failed_delivery_is_retried_and_not_committed(self):
        attempts = []

        def flaky_sender(content, recipient):
            attempts.append((content, recipient))
            return len(attempts) > 1

        self.processor.sender = flaky_sender
        content = milestone_card(EVENT_ONE, "PrazoPay Delay Alert")
        first = self.processor.process(content, CHANNEL)
        second = self.processor.process(content, CHANNEL)
        self.assertEqual(first[0], 503)
        self.assertEqual(second[0], 200)
        self.assertEqual(len(attempts), 2)

    def test_terminal_delivery_closes_all_future_output(self):
        terminal = milestone_card(
            EVENT_ONE, "PrazoPay Final Outcome", "SETTLEMENT_SUCCESS"
        )
        later = milestone_card(EVENT_TWO)
        self.assertEqual(self.processor.process(terminal, CHANNEL)[0], 200)
        self.assertEqual(self.processor.process(later, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.assertEqual(self.disabled, 2)
        state = self.processor.state_store.load()
        self.assertTrue(state["closed"])
        self.assertEqual(state["terminal_event_id"], EVENT_ONE)

    def test_agreement_funding_keeps_journey_monitor_open_for_handoff(self):
        funded = (
            "PrazoPay Escrow Funded\n"
            f"{agreement_provenance()}\n"
            f"{RELAY.MILESTONE_HANDOFF_SCHEMA_GUARD}\n"
            f"{RELAY.MILESTONE_PROTOCOL_GUARD}\n"
            f"{RELAY.MILESTONE_ACCEPTANCE_POLICY_GUARD}\n"
            "Event: AGREEMENT_FUNDED\n"
            f"Explorer: {AGREEMENT_EXPLORER}\n"
            f"Event ID: {EVENT_ONE}"
        )
        self.assertEqual(self.processor.process(funded, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.assertEqual(self.disabled, 0)
        self.assertFalse(self.processor.state_store.load()["closed"])

    def test_agreement_expiry_closes_journey_monitor(self):
        terminal = (
            "PrazoPay Agreement Closed\n"
            f"{agreement_provenance()}\n"
            "Event: AGREEMENT_FUNDING_WINDOW_EXPIRED\n"
            f"Explorer: {AGREEMENT_EXPLORER}\n"
            f"Event ID: {EVENT_ONE}"
        )
        self.assertEqual(self.processor.process(terminal, CHANNEL)[0], 200)
        self.assertEqual(self.disabled, 1)
        self.assertTrue(self.processor.state_store.load()["closed"])

    def test_closed_state_retries_heartbeat_disable_without_resending(self):
        disable_results = iter([False, True])
        disable_attempts = []

        def eventually_disable():
            disable_attempts.append(True)
            return next(disable_results)

        self.processor.disable_heartbeat = eventually_disable
        terminal = milestone_card(
            EVENT_ONE, "PrazoPay Final Outcome", "MILESTONE_FAILED"
        )
        with self.assertLogs(level="WARNING") as captured:
            self.assertEqual(self.processor.process(terminal, CHANNEL)[0], 200)
        self.assertIn(
            "heartbeat config could not be disabled", captured.output[0]
        )
        self.assertEqual(self.processor.process(terminal, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.assertEqual(len(disable_attempts), 2)

    def test_missing_event_id_fails_closed_without_discord(self):
        result = self.processor.process("PrazoPay Active Alert", CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertEqual(self.sent, [])

    def test_agreement_card_requires_exact_explorer_url(self):
        wrong = (
            "PrazoPay Agreement Proposal\n"
            "Explorer: https://explorer.solana.com/address/not-the-agreement?cluster=devnet\n"
            f"Event ID: {EVENT_ONE}"
        )
        result = self.processor.process(wrong, CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertIn("Explorer URL", result[1])
        self.assertEqual(self.sent, [])

        correct = (
            "PrazoPay Agreement Proposal\n"
            f"{agreement_provenance()}\n"
            f"{RELAY.AGREEMENT_ACCEPTANCE_GUARD}\n"
            f"{RELAY.SILENCE_POLICY_GUARD}\n"
            f"Explorer: {AGREEMENT_EXPLORER}\n"
            f"Event ID: {EVENT_ONE}"
        )
        self.assertEqual(self.processor.process(correct, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)

    def test_agreement_card_requires_explicit_acceptance_semantics(self):
        missing_guard = (
            "PrazoPay Agreement Proposal\n"
            f"{agreement_provenance()}\n"
            f"Explorer: {AGREEMENT_EXPLORER}\n"
            f"Event ID: {EVENT_ONE}"
        )
        result = self.processor.process(missing_guard, CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertIn("explicit Worker-signed acceptance", result[1])
        self.assertEqual(self.sent, [])

        missing_scope = (
            "PrazoPay Agreement Accepted\n"
            f"{agreement_provenance()}\n"
            f"{RELAY.AGREEMENT_ACCEPTANCE_GUARD}\n"
            f"Explorer: {AGREEMENT_EXPLORER}\n"
            f"Event ID: {EVENT_ONE}"
        )
        result = self.processor.process(missing_scope, CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertIn("Funder review", result[1])
        self.assertEqual(self.sent, [])

    def test_milestone_card_rejects_ambiguous_or_legacy_provenance(self):
        ambiguous = milestone_card(EVENT_ONE) + "\nProtocol version: v1"
        result = self.processor.process(ambiguous, CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertIn("ambiguous", result[1])

        legacy_protocol = milestone_card(EVENT_ONE).replace(
            RELAY.MILESTONE_PROTOCOL_GUARD,
            RELAY.LEGACY_MILESTONE_PROTOCOL,
        )
        result = self.processor.process(legacy_protocol, CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertIn("legacy Milestone protocol", result[1])

        legacy_policy = milestone_card(EVENT_ONE).replace(
            RELAY.MILESTONE_ACCEPTANCE_POLICY_GUARD,
            RELAY.LEGACY_ACCEPTANCE_POLICY,
        )
        result = self.processor.process(legacy_policy, CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertIn("legacy acceptance policy", result[1])
        self.assertEqual(self.sent, [])

    def test_milestone_card_requires_all_exact_v2_provenance_fields(self):
        for missing_guard in (
            RELAY.MILESTONE_SCHEMA_GUARD,
            RELAY.MILESTONE_PROTOCOL_GUARD,
            RELAY.MILESTONE_ACCEPTANCE_POLICY_GUARD,
        ):
            content = milestone_card(EVENT_ONE).replace(
                f"{missing_guard}\n", ""
            )
            result = self.processor.process(content, CHANNEL)
            self.assertEqual(result[0], 422)
            self.assertIn("exact v2 provenance", result[1])
        self.assertEqual(self.sent, [])

    def test_exact_v2_milestone_provenance_is_delivered(self):
        result = self.processor.process(milestone_card(EVENT_ONE), CHANNEL)
        self.assertEqual(result[0], 200)
        self.assertEqual(len(self.sent), 1)

    def test_agreement_card_rejects_wrong_protocol(self):
        content = (
            "PrazoPay Agreement Closed\n"
            f"{RELAY.AGREEMENT_SCHEMA_GUARD}\n"
            "On-chain Agreement protocol: v1\n"
            f"Explorer: {AGREEMENT_EXPLORER}\n"
            f"Event ID: {EVENT_ONE}"
        )
        result = self.processor.process(content, CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertIn("exact v2 provenance", result[1])
        self.assertEqual(self.sent, [])

    def test_rpc_failure_alerts_sparsely_then_recovers(self):
        failure = "NO_REPLY[FAIL]: RPC_NETWORK_FAILED"
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.assertIn("MONITOR_RPC_DEGRADED", self.sent[-1][0])
        self.assertIn("Reminder stage: first", self.sent[-1][0])

        self.now += 10 * 60
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.now += 20 * 60
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 2)
        self.assertIn("Reminder stage: 30m", self.sent[-1][0])
        self.now = 1_000_000 + 2 * 60 * 60
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 3)
        self.assertIn("Reminder stage: 2h", self.sent[-1][0])
        self.now = 1_000_000 + 24 * 60 * 60
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 4)
        self.assertIn("Reminder stage: day_1", self.sent[-1][0])

        self.assertEqual(self.processor.process("NO_REPLY", CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 5)
        self.assertIn("MONITOR_RPC_RECOVERED", self.sent[-1][0])
        self.assertNotIn("monitor_health", self.processor.state_store.load())

    def test_rpc_alert_delivery_failure_retries_same_stage(self):
        attempts = []

        def flaky_sender(content, recipient):
            attempts.append((content, recipient))
            return len(attempts) > 1

        self.processor.sender = flaky_sender
        failure = "NO_REPLY[FAIL]: PRAZOPAY_STATUS_UNAVAILABLE"
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 503)
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 200)
        self.assertEqual(len(attempts), 2)
        self.assertEqual(attempts[0][0], attempts[1][0])

    def test_protocol_mismatch_is_an_integrity_block_not_an_rpc_claim(self):
        failure = "NO_REPLY[FAIL]: PRAZOPAY_PROTOCOL_MISMATCH"
        self.assertEqual(self.processor.process(failure, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.assertIn("PrazoPay Monitor Integrity Block", self.sent[-1][0])
        self.assertIn("MONITOR_INTEGRITY_DEGRADED", self.sent[-1][0])
        self.assertNotIn("MONITOR_RPC_DEGRADED", self.sent[-1][0])

        self.assertEqual(self.processor.process("NO_REPLY", CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 2)
        self.assertIn("MONITOR_INTEGRITY_RECOVERED", self.sent[-1][0])

    def test_actionable_duplicate_after_outage_emits_one_recovery(self):
        content = milestone_card(EVENT_ONE)
        self.assertEqual(self.processor.process(content, CHANNEL)[0], 200)
        self.assertEqual(
            self.processor.process(
                "NO_REPLY[FAIL]: PRAZOPAY_STATUS_UNAVAILABLE", CHANNEL
            )[0],
            200,
        )
        self.assertEqual(self.processor.process(content, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 3)
        self.assertIn("MONITOR_RPC_RECOVERED", self.sent[-1][0])

    def test_wrong_recipient_is_rejected(self):
        result = self.processor.process(
            milestone_card(EVENT_ONE),
            "1532408222730686566",
        )
        self.assertEqual(result[0], 400)
        self.assertEqual(self.sent, [])

        quiet = self.processor.process("NO_REPLY", "1532408222730686566")
        self.assertEqual(quiet[0], 400)
        failure = self.processor.process(
            "NO_REPLY[FAIL]: RPC_NETWORK_FAILED", "1532408222730686566"
        )
        self.assertEqual(failure[0], 400)
        self.assertEqual(self.sent, [])

    def test_malformed_quiet_control_fails_closed(self):
        result = self.processor.process("NO_REPLY maybe", CHANNEL)
        self.assertEqual(result[0], 422)
        self.assertEqual(self.sent, [])

    def test_corrupt_delivery_state_fails_closed(self):
        state_path = self.processor.state_store.path
        state_path.write_text(
            '{"schema":"prazopay.delivery-state.v1",'
            f'"milestone":"{MILESTONE}","closed":false,'
            '"delivered_event_ids":["not-an-event"]}\n',
            encoding="utf-8",
        )
        with self.assertRaises(ValueError):
            self.processor.process(
                milestone_card(EVENT_ONE), CHANNEL
            )
        self.assertEqual(self.sent, [])

    def test_http_endpoint_authenticates_and_deduplicates(self):
        handler = RELAY.build_handler(
            self.processor, "Bearer local-relay-test-token"
        )
        server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        endpoint = f"http://127.0.0.1:{server.server_port}/heartbeat"
        payload = json.dumps(
            {
                "content": milestone_card(EVENT_ONE),
                "recipient": CHANNEL,
            }
        ).encode("utf-8")

        try:
            unauthorized = urllib.request.Request(
                endpoint,
                data=payload,
                headers={
                    "Authorization": "Bearer wrong-token",
                    "Content-Type": "application/json",
                },
                method="POST",
            )
            with self.assertRaises(urllib.error.HTTPError) as error:
                urllib.request.urlopen(unauthorized, timeout=2)
            self.assertEqual(error.exception.code, 401)

            authorized = urllib.request.Request(
                endpoint,
                data=payload,
                headers={
                    "Authorization": "Bearer local-relay-test-token",
                    "Content-Type": "application/json",
                },
                method="POST",
            )
            first = urllib.request.urlopen(authorized, timeout=2)
            self.assertEqual(first.status, 200)

            duplicate = urllib.request.Request(
                endpoint,
                data=payload,
                headers={
                    "Authorization": "Bearer local-relay-test-token",
                    "Content-Type": "application/json",
                },
                method="POST",
            )
            second = urllib.request.urlopen(duplicate, timeout=2)
            self.assertEqual(second.status, 200)
            self.assertEqual(len(self.sent), 1)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
