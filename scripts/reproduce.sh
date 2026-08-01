#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cache_root="${XDG_CACHE_HOME:-$HOME/.cache}"
test_target="$cache_root/prazopay-target"
sbf_target="$cache_root/prazopay-sbf-target"
sbf_output="$cache_root/prazopay-sbf-output"

temp_root="${TMPDIR:-/tmp}"
wasm_target="$(mktemp -d "$temp_root/prazopay-wasm-target.XXXXXX")"
wasm_compare="$(mktemp -d "$temp_root/prazopay-wasm-compare.XXXXXX")"

cleanup_wasm_temp() {
  case "$wasm_target" in
    "$temp_root"/prazopay-wasm-target.*)
      rm -rf -- "$wasm_target"
      ;;
  esac
  case "$wasm_compare" in
    "$temp_root"/prazopay-wasm-compare.*)
      rm -rf -- "$wasm_compare"
      ;;
  esac
}

trap cleanup_wasm_temp EXIT

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
  cargo build \
    -p prazopay-status \
    -p prazopay-agreement-status \
    --target wasm32-wasip2 \
    --release

component="$wasm_target/wasm32-wasip2/release/prazopay_status.wasm"
agreement_component="$wasm_target/wasm32-wasip2/release/prazopay_agreement_status.wasm"

committed_component="plugins/prazopay-status/prazopay-status.wasm"
committed_agreement_component="plugins/prazopay-agreement-status/prazopay-agreement-status.wasm"

wasm-tools validate "$component"
wasm-tools validate "$agreement_component"
wasm-tools validate "$committed_component"
wasm-tools validate "$committed_agreement_component"

rebuilt_status_canonical="$wasm_compare/rebuilt-status.wasm"
committed_status_canonical="$wasm_compare/committed-status.wasm"
rebuilt_agreement_canonical="$wasm_compare/rebuilt-agreement.wasm"
committed_agreement_canonical="$wasm_compare/committed-agreement.wasm"

# Rust components may contain host-specific, non-semantic custom sections.
# Strip every custom section from both sides before the reproducibility check;
# the executable component structure and code remain covered byte for byte.
wasm-tools strip --all "$component" -o "$rebuilt_status_canonical"
wasm-tools strip --all "$committed_component" -o "$committed_status_canonical"
wasm-tools strip --all "$agreement_component" -o "$rebuilt_agreement_canonical"
wasm-tools strip --all \
  "$committed_agreement_component" \
  -o "$committed_agreement_canonical"

if [[ -n "${PRAZOPAY_WASM_ARTIFACT_DIR:-}" ]]; then
  mkdir -p "$PRAZOPAY_WASM_ARTIFACT_DIR"
  cp "$component" "$PRAZOPAY_WASM_ARTIFACT_DIR/rebuilt-status.raw.wasm"
  cp "$committed_component" "$PRAZOPAY_WASM_ARTIFACT_DIR/committed-status.raw.wasm"
  cp "$agreement_component" "$PRAZOPAY_WASM_ARTIFACT_DIR/rebuilt-agreement.raw.wasm"
  cp \
    "$committed_agreement_component" \
    "$PRAZOPAY_WASM_ARTIFACT_DIR/committed-agreement.raw.wasm"
  cp \
    "$rebuilt_status_canonical" \
    "$PRAZOPAY_WASM_ARTIFACT_DIR/rebuilt-status.canonical.wasm"
  cp \
    "$committed_status_canonical" \
    "$PRAZOPAY_WASM_ARTIFACT_DIR/committed-status.canonical.wasm"
  cp \
    "$rebuilt_agreement_canonical" \
    "$PRAZOPAY_WASM_ARTIFACT_DIR/rebuilt-agreement.canonical.wasm"
  cp \
    "$committed_agreement_canonical" \
    "$PRAZOPAY_WASM_ARTIFACT_DIR/committed-agreement.canonical.wasm"
  sha256sum "$PRAZOPAY_WASM_ARTIFACT_DIR"/*.wasm \
    >"$PRAZOPAY_WASM_ARTIFACT_DIR/sha256sums.txt"
fi

cmp "$rebuilt_status_canonical" "$committed_status_canonical"
cmp "$rebuilt_agreement_canonical" "$committed_agreement_canonical"
echo "WASM_STATUS_CANONICAL_SHA256=$(sha256sum "$rebuilt_status_canonical" | awk '{print $1}')"
echo "WASM_AGREEMENT_CANONICAL_SHA256=$(sha256sum "$rebuilt_agreement_canonical" | awk '{print $1}')"
echo "WASM_SOURCE_ARTIFACT=PASS components=2"
mkdir -p "$sbf_output"
CARGO_TARGET_DIR="$sbf_target" \
  cargo build-sbf \
  --manifest-path programs/prazopay/Cargo.toml \
  --sbf-out-dir "$sbf_output"

rebuilt_sbf="$sbf_output/prazopay.so"
PRAZOPAY_V2_SBF="$rebuilt_sbf" \
  RUSTUP_TOOLCHAIN=1.97.1 \
  CARGO_TARGET_DIR="$test_target" \
  cargo test -p prazopay --test v2_chain_execution -- --ignored
echo "V2_LIFECYCLE=PASS"

candidate_hash="$(sha256sum "$rebuilt_sbf" | awk '{print $1}')"
echo "CANDIDATE_SBF_SHA256=$candidate_hash"
echo "CANDIDATE_SBF_BUILD=PASS"

printf '%s  %s\n' \
  'a54c676c98f526425ba77b54cfdb64a6ddddab2cf218d12f732dfa95bb4d8294' \
  'fixtures/prazopay-v2.so' | sha256sum --check -
cmp "$rebuilt_sbf" fixtures/prazopay-v2.so
echo "DEPLOYED_V2_FIXTURE_MATCH=PASS"

printf '%s  %s\n' \
  'b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2' \
  'fixtures/prazopay-v1.so' | sha256sum --check -
echo "DEPLOYED_V1_FIXTURE_HASH=PASS"

echo "WASM_VALIDATE=PASS components=2"

bash -n scripts/zeroclaw-prazopay-monitor.sh
bash -n scripts/zeroclaw-prazopay-agreement-monitor.sh
bash -n scripts/zeroclaw-prazopay-relay.sh
grep -Fq 'set_config heartbeat.target webhook.default' \
  scripts/zeroclaw-prazopay-monitor.sh
bash -n scripts/zeroclaw-prazopay-approval.sh
bash -n scripts/zeroclaw-prazopay-skill.sh
bash -n scripts/verify-devnet-live.sh
bash -n scripts/verify-devnet-v2-live.sh
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
python3 scripts/verify_public_evidence.py

echo "REPRODUCE=PASS"
