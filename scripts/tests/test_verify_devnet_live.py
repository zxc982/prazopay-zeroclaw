import hashlib
import struct
import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from verify_devnet_live import (  # noqa: E402
    ACCOUNT_DATA_LEN,
    PROGRAM_ID,
    base58_encode,
    decode_milestone_account,
    decode_program_account,
    decode_programdata_account,
    display_rpc_url,
)


def base58_decode(value: str) -> bytes:
    alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    number = 0
    for character in value:
        number = number * 58 + alphabet.index(character)
    decoded = number.to_bytes((number.bit_length() + 7) // 8, "big")
    return (b"\0" * (len(value) - len(value.lstrip("1")))) + decoded


class LiveVerifierDecodingTests(unittest.TestCase):
    def test_base58_round_trip_preserves_leading_zeroes(self):
        raw = b"\0\0" + bytes(range(1, 33))
        self.assertEqual(base58_decode(base58_encode(raw)), raw)

    def test_program_and_programdata_layouts(self):
        programdata_address = bytes(range(32))
        program = struct.pack("<I", 2) + programdata_address
        self.assertEqual(
            decode_program_account(program),
            base58_encode(programdata_address),
        )

        authority = bytes(reversed(range(32)))
        executable = b"\x7fELF" + bytes(20)
        programdata = (
            struct.pack("<IQB", 3, 479_993_358, 1) + authority + executable
        )
        decoded = decode_programdata_account(programdata)
        self.assertEqual(decoded["deployed_slot"], 479_993_358)
        self.assertEqual(decoded["upgrade_authority"], base58_encode(authority))
        self.assertEqual(decoded["program_bytes"], executable)

    def test_milestone_layout(self):
        funder = base58_decode("CkNmityXoeSFZmJErqDHKbMF5mgcwLqEKiXtxsFY6ZF8")
        worker = base58_decode("F5M9qTSuXVipJY7Rd7ZG3oySxX7iuEfi5v55nPmtRmWw")
        discriminator = hashlib.sha256(b"account:Milestone").digest()[:8]
        data = bytearray(discriminator)
        data.extend(funder)
        data.extend(worker)
        data.extend(bytes([1]) * 32)
        data.extend(bytes([2]) * 32)
        data.extend(bytes([3]) * 32)
        data.extend(bytes([4]) * 32)
        data.extend(struct.pack("<QqIq", 1, 1_785_424_322, 60, 1_785_422_756))
        data.extend(bytes([0x80, 2, 255]))
        self.assertEqual(len(data), ACCOUNT_DATA_LEN)

        decoded = decode_milestone_account(bytes(data))
        self.assertEqual(decoded["funder"], base58_encode(funder))
        self.assertEqual(decoded["worker"], base58_encode(worker))
        self.assertEqual(decoded["amount_lamports"], 1)
        self.assertEqual(decoded["protocol_version"], 1)
        self.assertEqual(decoded["revision_count"], 0)
        self.assertEqual(decoded["status"], "paid")
        self.assertEqual(decoded["bump"], 255)

    def test_program_id_constant_is_expected(self):
        self.assertEqual(
            PROGRAM_ID,
            "DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm",
        )

    def test_display_rpc_url_redacts_credentials_path_and_query(self):
        self.assertEqual(
            display_rpc_url("https://user:secret@rpc.example:8899/private-key?token=x"),
            "https://rpc.example:8899",
        )


if __name__ == "__main__":
    unittest.main()
