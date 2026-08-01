import hashlib
import struct
import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from verify_devnet_live import VerificationError, base58_encode  # noqa: E402
from verify_devnet_v2_live import (  # noqa: E402
    AGREEMENT_ACCOUNT_LEN,
    decode_agreement_account,
)


class V2LiveVerifierDecodingTests(unittest.TestCase):
    def agreement_bytes(self) -> bytes:
        data = bytearray(hashlib.sha256(b"account:Agreement").digest()[:8])
        data.extend(bytes([1]) * 32)
        data.extend(bytes([2]) * 32)
        data.extend(bytes([3]) * 32)
        data.extend(bytes([4]) * 32)
        data.extend(struct.pack("<QIIIqqq", 1, 3600, 600, 900, 100, 1900, 200))
        data.extend(bytes([5]) * 32)
        data.extend(bytes([1, 2, 9]))
        self.assertEqual(len(data), AGREEMENT_ACCOUNT_LEN)
        return bytes(data)

    def test_agreement_layout(self):
        decoded = decode_agreement_account(self.agreement_bytes())
        self.assertEqual(decoded["funder"], base58_encode(bytes([1]) * 32))
        self.assertEqual(decoded["worker"], base58_encode(bytes([2]) * 32))
        self.assertEqual(decoded["task_id_sha256"], (bytes([3]) * 32).hex())
        self.assertEqual(decoded["terms_sha256"], (bytes([4]) * 32).hex())
        self.assertEqual(decoded["amount_lamports"], 1)
        self.assertEqual(decoded["delivery_window_secs"], 3600)
        self.assertEqual(decoded["review_window_secs"], 600)
        self.assertEqual(decoded["funding_window_secs"], 900)
        self.assertEqual(decoded["proposed_at"], 100)
        self.assertEqual(decoded["proposal_expires_at"], 1900)
        self.assertEqual(decoded["accepted_at"], 200)
        self.assertEqual(decoded["milestone"], base58_encode(bytes([5]) * 32))
        self.assertTrue(decoded["silence_acceptance"])
        self.assertEqual(decoded["status"], "funded")
        self.assertEqual(decoded["bump"], 9)

    def test_agreement_wrong_length_fails_closed(self):
        with self.assertRaises(VerificationError):
            decode_agreement_account(self.agreement_bytes()[:-1])

    def test_agreement_wrong_discriminator_fails_closed(self):
        data = bytearray(self.agreement_bytes())
        data[0] ^= 0xFF
        with self.assertRaises(VerificationError):
            decode_agreement_account(bytes(data))


if __name__ == "__main__":
    unittest.main()
