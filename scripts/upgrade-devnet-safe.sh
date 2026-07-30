#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

program_id="${1:-DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm}"
program_file="${2:-target/deploy/prazopay.so}"
authority="${3:-$HOME/.config/solana/clawtrace-devnet.json}"
cluster="${4:-devnet}"

export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin"

for command_name in solana solana-keygen sha256sum cmp mktemp; do
  command -v "$command_name" >/dev/null
done

[[ -f "$program_file" ]]
[[ -f "$authority" ]]
[[ "$program_id" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]]
[[ "$cluster" == "devnet" ]]

temporary_directory="$(mktemp -d /tmp/prazopay-upgrade.XXXXXX)"
buffer_keypair="$temporary_directory/buffer-keypair.json"
buffer_dump="$temporary_directory/buffer-dump.so"
deployment_complete=false

cleanup() {
  case "$temporary_directory" in
    /tmp/prazopay-upgrade.*)
      rm -f -- "$buffer_dump"
      if [[ "$deployment_complete" == true ]]; then
        rm -f -- "$buffer_keypair"
        rmdir -- "$temporary_directory" 2>/dev/null || true
      else
        printf 'BUFFER_KEYPAIR_PRESERVED=%s\n' "$buffer_keypair" >&2
      fi
      ;;
  esac
}
trap cleanup EXIT

solana-keygen new \
  --no-bip39-passphrase \
  --force \
  --silent \
  --outfile "$buffer_keypair"

buffer_address="$(solana-keygen pubkey "$buffer_keypair")"
local_hash="$(sha256sum "$program_file" | cut -d " " -f 1)"

printf 'BUFFER_ADDRESS=%s\n' "$buffer_address"
printf 'LOCAL_SHA256=%s\n' "$local_hash"

solana \
  --keypair "$authority" \
  program write-buffer "$program_file" \
  --buffer "$buffer_keypair" \
  --buffer-authority "$authority" \
  --url "$cluster" \
  --commitment finalized \
  --use-rpc \
  --max-sign-attempts 10 \
  --output json-compact

solana \
  --keypair "$authority" \
  program dump "$buffer_address" "$buffer_dump" \
  --url "$cluster" \
  --commitment finalized

buffer_hash="$(sha256sum "$buffer_dump" | cut -d " " -f 1)"
printf 'BUFFER_SHA256=%s\n' "$buffer_hash"

if ! cmp -s "$program_file" "$buffer_dump"; then
  printf 'BUFFER_VERIFY=FAIL\n' >&2
  printf 'RECOVERABLE_BUFFER_ADDRESS=%s\n' "$buffer_address" >&2
  exit 1
fi

printf 'BUFFER_VERIFY=PASS\n'

solana \
  --keypair "$authority" \
  program deploy \
  --program-id "$program_id" \
  --buffer "$buffer_keypair" \
  --upgrade-authority "$authority" \
  --url "$cluster" \
  --commitment finalized \
  --use-rpc \
  --max-sign-attempts 10 \
  --output json-compact

deployment_complete=true
printf 'PROGRAM_UPGRADE=FINALIZED\n'
