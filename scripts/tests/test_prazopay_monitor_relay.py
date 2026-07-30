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


class RelayTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.sent = []
        self.disabled = 0
        store = RELAY.StateStore(
            Path(self.temporary.name) / "delivery-state.json", MILESTONE
        )

        def sender(content, recipient):
            self.sent.append((content, recipient))
            return True

        def disable():
            self.disabled += 1
            return True

        self.processor = RELAY.DeliveryProcessor(store, sender, disable, CHANNEL)

    def tearDown(self):
        self.temporary.cleanup()

    def test_same_event_is_delivered_once(self):
        content = f"PrazoPay Active Alert\nEvent ID: {EVENT_ONE}"
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
        content = f"PrazoPay Delay Alert\nEvent ID: {EVENT_ONE}"
        first = self.processor.process(content, CHANNEL)
        second = self.processor.process(content, CHANNEL)
        self.assertEqual(first[0], 503)
        self.assertEqual(second[0], 200)
        self.assertEqual(len(attempts), 2)

    def test_terminal_delivery_closes_all_future_output(self):
        terminal = (
            "PrazoPay Final Outcome\n"
            "Event: SETTLEMENT_SUCCESS\n"
            f"Event ID: {EVENT_ONE}"
        )
        later = f"PrazoPay Active Alert\nEvent ID: {EVENT_TWO}"
        self.assertEqual(self.processor.process(terminal, CHANNEL)[0], 200)
        self.assertEqual(self.processor.process(later, CHANNEL)[0], 200)
        self.assertEqual(len(self.sent), 1)
        self.assertEqual(self.disabled, 2)
        state = self.processor.state_store.load()
        self.assertTrue(state["closed"])
        self.assertEqual(state["terminal_event_id"], EVENT_ONE)

    def test_closed_state_retries_heartbeat_disable_without_resending(self):
        disable_results = iter([False, True])
        disable_attempts = []

        def eventually_disable():
            disable_attempts.append(True)
            return next(disable_results)

        self.processor.disable_heartbeat = eventually_disable
        terminal = (
            "PrazoPay Final Outcome\n"
            "Event: MILESTONE_FAILED\n"
            f"Event ID: {EVENT_ONE}"
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

    def test_quiet_failure_output_is_suppressed(self):
        result = self.processor.process(
            "NO_REPLY[FAIL]: RPC_NETWORK_FAILED", CHANNEL
        )
        self.assertEqual(result[0], 200)
        self.assertEqual(self.sent, [])

    def test_wrong_recipient_is_rejected(self):
        result = self.processor.process(
            f"PrazoPay Active Alert\nEvent ID: {EVENT_ONE}",
            "1532408222730686566",
        )
        self.assertEqual(result[0], 400)
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
                f"PrazoPay Active Alert\nEvent ID: {EVENT_ONE}", CHANNEL
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
                "content": f"PrazoPay Active Alert\nEvent ID: {EVENT_ONE}",
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
