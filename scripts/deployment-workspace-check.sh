#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cache_root="${XDG_CACHE_HOME:-$HOME/.cache}"
anchor_target="$cache_root/prazopay-anchor-target"
test_target="$cache_root/prazopay-target"
wasm_target="$cache_root/prazopay-wasm-target"
keypair="target/deploy/prazopay-keypair.json"

if [[ ! -f "$keypair" ]]; then
  echo "Missing local test program keypair: $keypair" >&2
  exit 1
fi

mkdir -p "$anchor_target/deploy" target/deploy target/idl
cp "$keypair" "$anchor_target/deploy/prazopay-keypair.json"

CARGO_TARGET_DIR="$anchor_target" anchor build
cp "$anchor_target/deploy/prazopay.so" target/deploy/prazopay.so
cp "$anchor_target/idl/prazopay.json" target/idl/prazopay.json

RUSTUP_TOOLCHAIN=1.97.1 \
  CARGO_TARGET_DIR="$wasm_target" \
  cargo build \
    -p prazopay-status \
    -p prazopay-agreement-status \
    --target wasm32-wasip2 \
    --release
cp \
  "$wasm_target/wasm32-wasip2/release/prazopay_status.wasm" \
  plugins/prazopay-status/prazopay-status.wasm
cp \
  "$wasm_target/wasm32-wasip2/release/prazopay_agreement_status.wasm" \
  plugins/prazopay-agreement-status/prazopay-agreement-status.wasm

wasm-tools validate plugins/prazopay-status/prazopay-status.wasm
wasm-tools validate \
  plugins/prazopay-agreement-status/prazopay-agreement-status.wasm

bash -n scripts/zeroclaw-prazopay-monitor.sh
bash -n scripts/zeroclaw-prazopay-agreement-monitor.sh
bash -n scripts/zeroclaw-prazopay-relay.sh
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
grep -Fq '"commitment": "finalized"' plugins/prazopay-status/src/lib.rs
grep -Fq '"commitment": "finalized"' \
  plugins/prazopay-agreement-status/src/lib.rs

RUSTUP_TOOLCHAIN=1.97.1 cargo fmt --all -- --check
RUSTUP_TOOLCHAIN=1.97.1 \
  CARGO_TARGET_DIR="$test_target" \
  cargo test --workspace

program_id="$(solana-keygen pubkey "$keypair")"
grep -Fq "$program_id" Anchor.toml
grep -Fq "$program_id" programs/prazopay/src/lib.rs
grep -Fq "$program_id" plugins/prazopay-status/src/status.rs
grep -Fq "$program_id" plugins/prazopay-agreement-status/src/agreement.rs
grep -Fq "$program_id" target/idl/prazopay.json

echo "DEPLOYMENT_WORKSPACE_CHECK=PASS"
echo "PROGRAM_ID=$program_id"
