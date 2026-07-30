#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cache_root="${XDG_CACHE_HOME:-$HOME/.cache}"
test_target="$cache_root/prazopay-target"
wasm_target="$cache_root/prazopay-wasm-target"

RUSTUP_TOOLCHAIN=1.97.1 cargo fmt --all -- --check
RUSTUP_TOOLCHAIN=1.97.1 \
  CARGO_TARGET_DIR="$test_target" \
  cargo test --workspace
RUSTUP_TOOLCHAIN=1.97.1 \
  CARGO_TARGET_DIR="$wasm_target" \
  cargo build -p prazopay-status --target wasm32-wasip2 --release

component="$wasm_target/wasm32-wasip2/release/prazopay_status.wasm"
printf '%s  %s\n' \
  'b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2' \
  'fixtures/prazopay-v1.so' | sha256sum --check -
echo "SBF_FIXTURE_HASH=PASS"

if command -v wasm-tools >/dev/null 2>&1; then
  wasm-tools validate "$component"
  echo "WASM_VALIDATE=PASS"
else
  echo "WASM_VALIDATE=SKIPPED (wasm-tools not installed)"
fi

bash -n scripts/zeroclaw-prazopay-monitor.sh
bash -n scripts/zeroclaw-prazopay-relay.sh
bash -n scripts/zeroclaw-prazopay-approval.sh
bash -n scripts/zeroclaw-prazopay-skill.sh
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
python3 scripts/verify_public_evidence.py

echo "REPRODUCE=PASS"
