#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cache_root="${XDG_CACHE_HOME:-$HOME/.cache}"
test_target="$cache_root/prazopay-target"
wasm_target="$cache_root/prazopay-wasm-target"
sbf_target="$cache_root/prazopay-sbf-target"
sbf_output="$cache_root/prazopay-sbf-output"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "MISSING_PREREQUISITE=$1" >&2
    echo "See docs/REPRODUCE.md for exact installation commands." >&2
    exit 1
  fi
}

for required in rustup cargo cargo-build-sbf python3 sha256sum cmp wasm-tools; do
  require_command "$required"
done

sbf_builder_version="$(cargo-build-sbf --version | head -n 1 | awk '{print $2}')"
if [[ "$sbf_builder_version" != "3.1.10" ]]; then
  echo "SOLANA_CLI_VERSION=FAIL expected=3.1.10 actual=$sbf_builder_version" >&2
  exit 1
fi

if ! rustup target list --toolchain 1.97.1 --installed | grep -Fxq wasm32-wasip2; then
  echo "RUST_TARGET=FAIL missing=wasm32-wasip2 toolchain=1.97.1" >&2
  exit 1
fi

echo "RUST_TOOLCHAIN=$(rustc +1.97.1 --version)"
echo "SOLANA_SBF_BUILDER=$(cargo-build-sbf --version)"
echo "WASM_TOOLS=$(wasm-tools --version)"

RUSTUP_TOOLCHAIN=1.97.1 cargo fmt --all -- --check
RUSTUP_TOOLCHAIN=1.97.1 \
  CARGO_TARGET_DIR="$test_target" \
  cargo test --workspace
RUSTUP_TOOLCHAIN=1.97.1 \
  CARGO_TARGET_DIR="$wasm_target" \
  cargo build -p prazopay-status --target wasm32-wasip2 --release

component="$wasm_target/wasm32-wasip2/release/prazopay_status.wasm"
mkdir -p "$sbf_output"
CARGO_TARGET_DIR="$sbf_target" \
  cargo build-sbf \
  --manifest-path programs/prazopay/Cargo.toml \
  --sbf-out-dir "$sbf_output"

rebuilt_sbf="$sbf_output/prazopay.so"
if ! cmp -s "$rebuilt_sbf" fixtures/prazopay-v1.so; then
  echo "SOURCE_TO_SBF=FAIL" >&2
  sha256sum "$rebuilt_sbf" fixtures/prazopay-v1.so >&2
  exit 1
fi
echo "SOURCE_TO_SBF=PASS"

printf '%s  %s\n' \
  'b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2' \
  'fixtures/prazopay-v1.so' | sha256sum --check -
echo "SBF_FIXTURE_HASH=PASS"

wasm-tools validate "$component"
echo "WASM_VALIDATE=PASS"

bash -n scripts/zeroclaw-prazopay-monitor.sh
bash -n scripts/zeroclaw-prazopay-relay.sh
bash -n scripts/zeroclaw-prazopay-approval.sh
bash -n scripts/zeroclaw-prazopay-skill.sh
bash -n scripts/verify-devnet-live.sh
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
python3 scripts/verify_public_evidence.py

echo "REPRODUCE=PASS"
